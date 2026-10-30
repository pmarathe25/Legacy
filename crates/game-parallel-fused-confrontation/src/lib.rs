//! Independent decisions to begin real conflict, based on personal assessments.
use game_parallel::{CombatEncounter, CombatFighter, CombatOrder, World};
use game_parallel_fused_data::{Block, NONE};
use rayon::prelude::*;

pub struct Draft {
    pub actors: Vec<u64>,
    fights: Vec<CombatEncounter>,
    next_id: u64,
}
pub fn prepare(
    world: &World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    occupied: &[u64],
    controlled: Option<u64>,
    tick: u64,
) -> Result<Option<Draft>, &'static str> {
    let Some(opinions) = world.opinions.as_ref().filter(|owner| {
        owner.sensitivities.par_iter().any(|row| {
            row.confrontation
                .as_ref()
                .is_some_and(|policy| policy.enabled && tick.is_multiple_of(policy.interval))
        })
    }) else {
        return Ok(None);
    };
    let conflict = world
        .conflict
        .as_ref()
        .ok_or("personal confrontation requires conflict")?;
    let mut busy = occupied.to_vec();
    let mut engaged = conflict.actors();
    if let Some(care) = &world.care {
        engaged.extend(care.actors());
    }
    engaged.par_sort_unstable();
    busy.extend_from_slice(&engaged);
    busy.par_sort_unstable();
    let locate = |person| {
        let index = positions.binary_search_by_key(&person, |row| row.0).ok()?;
        let (_, block, slot) = positions[index];
        Some((&blocks[block], slot))
    };
    let mut known: Vec<_> = opinions.records.par_iter().collect();
    known.par_sort_unstable_by_key(|row| (row.observer, row.trust, row.target));
    let mut choices: Vec<_> = opinions
        .sensitivities
        .par_iter()
        .filter_map(|preference| {
            let policy = preference.confrontation.as_ref()?;
            let actor = preference.person;
            if !policy.enabled
                || Some(actor) == controlled
                || !tick.is_multiple_of(policy.interval)
                || busy.binary_search(&actor).is_ok()
            {
                return None;
            }
            let (a, ai) = locate(actor)?;
            if a.journey.is_some()
                || a.health[ai] == 0
                || a.work[ai] != NONE
                || a.fatigue.as_ref().is_some_and(|f| f.flags[ai] & 2 != 0)
            {
                return None;
            }
            let start = known.partition_point(|row| row.observer < actor);
            let end = known.partition_point(|row| row.observer <= actor);
            known[start..end].iter().take(32).find_map(|opinion| {
                if opinion.trust > policy.trust_at_most
                    || opinion.fear > policy.fear_at_most
                    || engaged.binary_search(&opinion.target).is_ok()
                    || (Some(opinion.target) != controlled
                        && busy.binary_search(&opinion.target).is_ok())
                {
                    return None;
                }
                let (b, bi) = locate(opinion.target)?;
                (b.journey.is_none()
                    && b.health[bi] > 0
                    && b.work[bi] == NONE
                    && a.region == b.region)
                    .then_some((actor, opinion.target, world.regions[a.region].id))
            })
        })
        .collect();
    if choices.is_empty() {
        return Ok(None);
    }
    choices.par_sort_unstable();
    let mut claims: Vec<_> = choices
        .par_iter()
        .enumerate()
        .flat_map_iter(|(i, &(actor, target, _))| [(actor, i), (target, i)])
        .collect();
    claims.par_sort_unstable();
    let selected: Vec<_> = choices
        .par_iter()
        .enumerate()
        .filter_map(|(i, &choice)| {
            let first_actor = claims.partition_point(|row| row.0 < choice.0);
            let first_target = claims.partition_point(|row| row.0 < choice.1);
            (claims[first_actor].1 == i && claims[first_target].1 == i).then_some(choice)
        })
        .collect();
    if selected.is_empty() {
        return Ok(None);
    }
    let next_id = conflict
        .next_id
        .checked_add(selected.len() as u64)
        .ok_or("conflict id overflow")?;
    let fights = selected
        .par_iter()
        .enumerate()
        .map(|(i, &(actor, target, region))| {
            let mut fighters = vec![
                CombatFighter {
                    person: actor,
                    side: 1,
                    order: CombatOrder::Attack { target },
                },
                CombatFighter {
                    person: target,
                    side: 2,
                    order: CombatOrder::Defend,
                },
            ];
            fighters.sort_unstable_by_key(|fighter| fighter.person);
            CombatEncounter {
                id: conflict.next_id + i as u64,
                region,
                fighters,
            }
        })
        .collect();
    let mut actors: Vec<_> = selected
        .par_iter()
        .flat_map_iter(|&(a, b, _)| [a, b])
        .collect();
    actors.par_sort_unstable();
    Ok(Some(Draft {
        actors,
        fights,
        next_id,
    }))
}
impl Draft {
    pub fn commit(
        self,
        world: &mut World,
        blocks: &[Block],
        positions: &[(u64, usize, usize)],
        runtime: &mut game_parallel_fused_conflict::packed::Runtime,
    ) {
        let alive = |person| {
            positions
                .binary_search_by_key(&person, |row| row.0)
                .ok()
                .is_some_and(|i| {
                    let (_, block, slot) = positions[i];
                    blocks[block].journey.is_none() && blocks[block].health[slot] > 0
                })
        };
        let fights: Vec<_> = self
            .fights
            .into_par_iter()
            .filter(|fight| fight.fighters.iter().all(|fighter| alive(fighter.person)))
            .collect();
        let conflict = world.conflict.as_mut().expect("prepared conflict");
        conflict.next_id = self.next_id;
        if !fights.is_empty() {
            conflict.encounters.par_extend(fights);
            runtime.invalidate();
        }
    }
}

#[macro_export]
macro_rules! publish {
    ($host:expr, $draft:expr) => {
        if let Some(confrontation) = $draft.decisions.confrontation {
            confrontation.commit(
                &mut $host.template,
                &$host.blocks,
                &$host.positions,
                $host
                    .owners
                    .conflict
                    .as_mut()
                    .expect("prepared conflict runtime"),
            );
        }
    };
}

#[macro_export]
macro_rules! methods {
    () => {
        pub fn set_confrontation(&mut self, person: u64, enabled: bool) -> Result<(), String> {
            let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
            if self
                .template
                .lineage
                .as_ref()
                .and_then(|lineage| lineage.suspended_person(self.template.tick))
                == Some(person)
            {
                return Err("player control is suspended".into());
            }
            let index = self
                .positions
                .binary_search_by_key(&person, |row| row.0)
                .map_err(|_| "unknown person")?;
            let (_, block, slot) = self.positions[index];
            if self.blocks[block].health[slot] == 0 {
                return Err("person is dead".into());
            }
            let policy = self
                .template
                .opinions
                .as_mut()
                .and_then(|owner| {
                    owner
                        .sensitivities
                        .iter_mut()
                        .find(|row| row.person == person)
                })
                .and_then(|row| row.confrontation.as_mut())
                .ok_or("no confrontation preference")?;
            policy.enabled = enabled;
            self.revision = revision;
            Ok(())
        }
    };
}
