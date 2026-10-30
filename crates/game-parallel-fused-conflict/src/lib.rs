//! Sparse conflict damage against unpublished physical blocks.
pub mod command;
pub mod packed;
use game_parallel::{Conflict, World};
use game_parallel_fused_data::Block;
use game_parallel_production_model::PersonSnapshot;
use rayon::prelude::*;
pub fn prepare(world: &World, people: &[PersonSnapshot], tick: u64) -> Option<Conflict> {
    world
        .conflict
        .as_ref()
        .map(|conflict| game_parallel_conflict::prepare(conflict, people, tick))
}
pub fn apply(conflict: Option<&Conflict>, blocks: &mut [Block], positions: &[(u64, usize, usize)]) {
    let Some(conflict) = conflict else {
        return;
    };
    let dense = positions.first().and_then(|first| {
        positions.last().and_then(|last| {
            (last.0.checked_sub(first.0) == Some((positions.len() - 1) as u64)).then_some(first.0)
        })
    });
    let locate = |id: u64| {
        dense
            .and_then(|first| id.checked_sub(first))
            .and_then(|offset| usize::try_from(offset).ok())
            .filter(|&index| index < positions.len())
            .or_else(|| {
                if dense.is_none() {
                    positions.binary_search_by_key(&id, |row| row.0).ok()
                } else {
                    None
                }
            })
    };
    // Conflict prepares events in target order, so arbitration can reduce them
    // directly without allocating and sorting a second per-strike buffer.
    let mut damage: Vec<_> = conflict
        .events
        .par_chunk_by(|a, b| a.target == b.target)
        .map(|group| {
            let index = locate(group[0].target).expect("validated conflict target");
            let amount = if group.len() >= 2048 {
                group
                    .par_iter()
                    .map(|event| event.damage)
                    .reduce(|| 0u32, u32::saturating_add)
            } else {
                group
                    .iter()
                    .fold(0u32, |sum, event| sum.saturating_add(event.damage))
            };
            (positions[index].1, positions[index].2, amount)
        })
        .collect();
    if damage
        .par_windows(2)
        .any(|pair| (pair[0].0, pair[0].1) > (pair[1].0, pair[1].1))
    {
        damage.par_sort_unstable_by_key(|row| (row.0, row.1));
    }
    blocks
        .par_iter_mut()
        .enumerate()
        .for_each(|(index, block)| {
            let start = damage.partition_point(|row| row.0 < index);
            let end = damage.partition_point(|row| row.0 <= index);
            for &(_, offset, amount) in &damage[start..end] {
                block.next_health[offset] = block.next_health[offset].saturating_sub(amount);
            }
        });
}
pub fn finish(
    conflict: Option<&mut Conflict>,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
) {
    if let Some(conflict) = conflict {
        let dense = positions.first().and_then(|first| {
            positions.last().and_then(|last| {
                (last.0.checked_sub(first.0) == Some((positions.len() - 1) as u64))
                    .then_some(first.0)
            })
        });
        game_parallel_conflict::finish(conflict, |id| {
            let index = dense
                .and_then(|first| id.checked_sub(first))
                .and_then(|offset| usize::try_from(offset).ok())
                .filter(|&index| index < positions.len())
                .or_else(|| {
                    if dense.is_none() {
                        positions.binary_search_by_key(&id, |row| row.0).ok()
                    } else {
                        None
                    }
                });
            index.is_some_and(|i| {
                let (_, block, offset) = positions[i];
                blocks[block].next_health[offset] > 0
            })
        });
    }
}

/// Reject an action if any actor is reserved by an active encounter.
pub fn check(world: &World, actors: &[u64]) -> Result<(), String> {
    let mut busy = world
        .conflict
        .as_ref()
        .map_or_else(Vec::new, |conflict| conflict.actors());
    if let Some(care) = &world.care {
        busy.extend(care.actors());
    }
    if !busy.is_empty() {
        busy.par_sort_unstable();
        if actors
            .par_iter()
            .any(|actor| busy.binary_search(actor).is_ok())
        {
            return Err("actor is busy".into());
        }
    }
    Ok(())
}

/// Prevent accepting an exchange while either party is engaged.
pub fn check_decisions(
    world: &World,
    decisions: &[game_parallel::TradeDecision],
) -> Result<(), String> {
    if world.conflict.is_none() && world.care.is_none() {
        return Ok(());
    }
    let Some(market) = &world.market else {
        return Ok(());
    };
    let actors: Vec<_> = decisions
        .iter()
        .filter(|decision| decision.outcome == game_parallel::TradeOutcome::Accepted)
        .filter_map(|decision| market.offers.iter().find(|offer| offer.id == decision.id))
        .flat_map(|offer| [offer.proposer, offer.recipient])
        .collect();
    check(world, &actors)
}

/// Start an encounter after reserving all participants against existing work.
pub fn begin(
    world: &mut World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    fight: game_parallel::CombatEncounter,
) -> Result<u64, String> {
    if world
        .lineage
        .as_ref()
        .and_then(|lineage| lineage.suspended_person(world.tick))
        .is_some_and(|person| {
            fight
                .fighters
                .iter()
                .any(|fighter| fighter.person == person)
        })
    {
        return Err("player control is suspended".into());
    }
    let people = game_parallel_fused_index::people(blocks, &world.regions, positions);
    let mut busy: Vec<_> = blocks
        .par_iter()
        .flat_map_iter(|block| {
            block
                .ids
                .iter()
                .zip(&block.work)
                .filter_map(|(&id, &handle)| {
                    (handle != game_parallel_fused_data::NONE).then_some(id)
                })
        })
        .collect();
    if let Some(care) = &world.care {
        busy.extend(care.actors());
    }
    world
        .conflict
        .as_mut()
        .ok_or("conflict is disabled")?
        .begin(&people, &busy, fight)
}
pub fn order(world: &mut World, commands: &[game_parallel::CombatCommand]) -> Result<(), String> {
    if world
        .lineage
        .as_ref()
        .and_then(|lineage| lineage.suspended_person(world.tick))
        .is_some_and(|person| commands.iter().any(|command| command.person == person))
    {
        return Err("player control is suspended".into());
    }
    let conflict = world.conflict.as_mut().ok_or("conflict is disabled")?;
    if commands.len() <= 64 {
        conflict.order_sorted(commands)
    } else {
        conflict.order(commands)
    }
}

#[macro_export]
macro_rules! host_methods {
    () => {
        game_parallel_fused_confrontation::methods!();
        pub fn begin_combat(&mut self, fight: CombatEncounter) -> Result<u64, String> {
            self.conflict_command(Command::Begin(fight, None))
        }
        pub fn begin_tactical_combat(
            &mut self,
            fight: CombatEncounter,
            battle: TacticalBattle,
        ) -> Result<u64, String> {
            self.conflict_command(Command::Begin(fight, Some(battle)))
        }
        pub fn tactical_moves(&mut self, commands: &[TacticalMove]) -> Result<(), String> {
            self.conflict_command(Command::Move(commands)).map(|_| ())
        }
        pub fn combat_orders(&mut self, commands: &[CombatCommand]) -> Result<(), String> {
            self.conflict_command(Command::Orders(commands)).map(|_| ())
        }
    };
}
