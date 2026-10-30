//! Compose tick-start decision and actor reservations.
mod knowledge;
mod lineage;
mod society;
use game_parallel::World;
use game_parallel_fused_data::Block;
use game_parallel_fused_metadata::Store;
use rayon::prelude::*;
type Runtimes<'a> = (
    Option<&'a mut game_parallel_fused_conflict::packed::Runtime>,
    Option<&'a mut game_parallel_knowledge_columns::Runtime>,
    Option<&'a mut game_parallel_social_edges::Runtime>,
    Option<&'a mut game_parallel_fused_autonomy::Runtime>,
    Option<&'a game_parallel_family_lifecycle::Runtime>,
);
pub struct Draft {
    pub lineage: lineage::Draft,
    pub conflict: Option<game_parallel_fused_conflict::packed::Draft>,
    pub confrontation: Option<game_parallel_fused_confrontation::Draft>,
    pub trade: Option<game_parallel_market_auto::Draft>,
    pub agreements: Option<game_parallel_agreement_auto::Draft>,
    pub autowork: Option<game_parallel_fused_autowork::Draft>,
    pub goal_blocked: Vec<game_parallel::GoalCompletion>,
    pub farm_restricted: Vec<u64>,
    pub society: society::Draft,
    pub planned: Vec<u64>,
    pub care: Option<game_parallel_fused_care::Draft>,
    pub caregiving: Option<game_parallel_dependent_care::Draft>,
    pub missions: Option<game_parallel::MissionDraft>,
    pub conversation_accepted: Option<bool>,
}
pub fn prepare(
    world: &mut World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    jobs: &Store,
    food: &[u32],
    time: (u64, Option<(u64, u64)>),
    runtimes: Runtimes<'_>,
) -> Result<Draft, &'static str> {
    let (next, conversation) = time;
    let (conflict_runtime, knowledge_runtime, social_runtime, autonomy_runtime, family_runtime) =
        runtimes;
    let lineage = lineage::prepare(world, blocks, positions)?;
    let mut occupied: Vec<_> = lineage.controlled.into_iter().collect();
    if (world.agreements.is_some()
        || world.caregiving.is_some()
        || world.missions.is_some()
        || world.market.is_some()
        || world.autowork.is_some()
        || world.social.is_some()
        || world.autonomy.is_some()
        || world.agriculture.is_some())
        && world.fatigue.is_some()
    {
        occupied.extend(
            blocks
                .par_iter()
                .flat_map_iter(|block| {
                    let flags = &block.fatigue.as_ref().expect("fatigue columns").flags;
                    block
                        .ids
                        .iter()
                        .zip(flags)
                        .filter_map(|(&id, &flag)| (flag & 2 != 0).then_some(id))
                })
                .collect::<Vec<_>>(),
        );
    }
    let people = if society::needs_people(world) {
        game_parallel_fused_index::people(blocks, &world.regions, positions)
    } else {
        Vec::new()
    };
    let prepare_conflict = || -> Result<_, &'static str> {
        match (world.conflict.as_ref(), conflict_runtime) {
            (Some(conflict), Some(runtime)) => Ok(Some(runtime.prepare(
                conflict,
                world.tactics.as_ref(),
                blocks,
                positions,
                &world.regions,
                next,
            )?)),
            (None, None) => Ok(None),
            _ => Err("conflict runtime missing"),
        }
    };
    let prepare_care = || game_parallel_fused_care::prepare(world, &people, next);
    let (conflict, care) = if world.conflict.is_some() && world.care.is_some() {
        let (conflict, care) = rayon::join(prepare_conflict, prepare_care);
        (conflict?, care)
    } else {
        (prepare_conflict()?, prepare_care())
    };
    if let Some(care) = &world.care {
        occupied.extend(care.actors());
    }
    if (world.agreements.is_some()
        || world.caregiving.is_some()
        || world.missions.is_some()
        || world.market.is_some()
        || world.autowork.is_some()
        || world.social.is_some()
        || world.autonomy.is_some()
        || world.agriculture.is_some())
        && let Some(conflict) = &world.conflict
    {
        occupied.extend(conflict.actors());
    }
    let confrontation = game_parallel_fused_confrontation::prepare(
        world,
        blocks,
        positions,
        &occupied,
        if conversation.is_some() {
            None
        } else {
            lineage.controlled
        },
        next,
    )?;
    if let Some(confrontation) = &confrontation {
        occupied.extend_from_slice(&confrontation.actors);
    }
    let caregiving = game_parallel_fused_dependent_care::prepare(
        world,
        family_runtime,
        blocks,
        positions,
        &people,
        &occupied,
        next,
    )?;
    if let Some(caregiving) = &caregiving {
        occupied.extend_from_slice(&caregiving.actors);
    }
    let forced = if let Some((actor, target)) = conversation {
        let social = world.social.as_ref().ok_or("social is disabled")?;
        let mut busy = occupied.clone();
        busy.extend(
            blocks
                .par_iter()
                .flat_map_iter(|block| {
                    block
                        .ids
                        .iter()
                        .zip(&block.work)
                        .filter_map(|(&id, &work)| {
                            (work != game_parallel_fused_data::NONE).then_some(id)
                        })
                })
                .collect::<Vec<_>>(),
        );
        let personality = world.personality.as_ref().map(|owner| owner.index());
        let event = game_parallel_social_edges::explicit::resolve(
            social,
            &people,
            (actor, target, world.socially_pressured(target, actor)),
            &busy,
            lineage.controlled,
            next,
            |person, isolation| {
                personality
                    .as_ref()
                    .is_none_or(|traits| traits.social(person, isolation, world.seed, next))
            },
        )?;
        occupied.push(actor);
        if event.is_some() {
            occupied.push(target);
        }
        event
    } else {
        None
    };
    let missions = game_parallel_fused_missions::prepare(world, blocks, positions, &occupied, next);
    if let Some(missions) = &missions {
        occupied.extend_from_slice(&missions.actors);
    }
    let trade = game_parallel_fused_trade::prepare(world, blocks, &people, next, &occupied)?;
    if let Some(trade) = &trade {
        occupied.extend_from_slice(&trade.actors);
    }
    let agreements = if let Some(agreements) = world
        .agreements
        .as_ref()
        .filter(|owner| owner.policies.iter().any(|policy| policy.automatic()))
    {
        let cargo = world
            .goods_cargo
            .as_ref()
            .ok_or("agreements require goods cargo")?;
        let mut busy: Vec<_> = blocks
            .par_iter()
            .flat_map_iter(|block| {
                block
                    .ids
                    .iter()
                    .zip(&block.work)
                    .filter_map(|(&id, &work)| {
                        (work != game_parallel_fused_data::NONE).then_some(id)
                    })
            })
            .collect();
        busy.extend_from_slice(&occupied);
        game_parallel_agreement_auto::prepare_validated(
            agreements,
            cargo,
            world.opinions.as_ref(),
            &people,
            &busy,
            next,
        )
        .map_err(|_| "invalid automatic agreement")?
    } else {
        None
    };
    if let Some(agreements) = &agreements {
        occupied.extend_from_slice(&agreements.actors);
    }
    let restricted = game_parallel_fused_autowork::restricted(world, family_runtime, next);
    let work_reserved = if restricted.is_empty() {
        None
    } else {
        let mut rows = occupied.clone();
        rows.extend_from_slice(&restricted);
        Some(rows)
    };
    let (autowork, goal_blocked) = game_parallel_fused_autowork::prepare(
        world,
        blocks,
        jobs,
        &people,
        work_reserved.as_deref().unwrap_or(&occupied),
    )?;
    if let Some(work) = &autowork {
        occupied.extend_from_slice(&work.actors);
    }
    occupied.par_sort_unstable();
    let explicit_accepted = forced.is_some();
    let mut forced: Vec<_> = forced.into_iter().collect();
    let automatic =
        game_parallel_fused_social_pressure::prepare(world, blocks, positions, &occupied, next);
    if !automatic.is_empty() {
        occupied.par_extend(
            automatic
                .par_iter()
                .flat_map_iter(|event| [event.a, event.b]),
        );
        occupied.par_sort_unstable();
        forced.par_extend(automatic);
    }
    let society = society::prepare(
        world,
        blocks,
        food,
        &people,
        next,
        (&occupied, &forced),
        knowledge::Context {
            runtime: knowledge_runtime,
            social: social_runtime,
            autonomy: autonomy_runtime,
            positions,
        },
    )?;
    occupied.extend_from_slice(&society.actors);
    if occupied.par_windows(2).any(|pair| pair[0] > pair[1]) {
        occupied.par_sort_unstable();
    }
    Ok(Draft {
        lineage,
        conflict,
        confrontation,
        trade,
        agreements,
        autowork,
        goal_blocked,
        farm_restricted: restricted,
        society,
        planned: occupied,
        care,
        caregiving,
        missions,
        conversation_accepted: conversation.map(|_| explicit_accepted),
    })
}

impl Draft {
    game_parallel_fused_trade::exchange_methods!();
}

#[macro_export]
macro_rules! pressure_methods {
    () => {
        pub fn set_social_pressure(&mut self, person: u64, enabled: bool) -> Result<(), String> {
            self.edit_world(|world| world.set_social_pressure(person, enabled))
        }
    };
}
