//! Resolve goals from actual social encounters and physical gifts.
use game_parallel_missions::{
    Event as MissionEvent, EventKind as MissionEventKind, Missions, Objective as MissionObjective,
    Phase as MissionPhase,
};
use game_parallel_social::Encounter;
use rayon::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gift {
    pub region: u64,
    pub giver: u64,
    pub recipient: u64,
    pub good: u32,
    pub quantity: u64,
}

/// Credit accepted gifts at the destination, retaining partial progress.
pub fn deliveries(state: &mut Missions, gifts: &[Gift], tick: u64) -> Vec<MissionEvent> {
    if gifts.is_empty()
        || !state.entries.par_iter().any(|mission| {
            matches!(mission.objective, Some(MissionObjective::Deliver { .. }))
                && !mission.objective_met
                && !matches!(
                    mission.phase,
                    MissionPhase::Ready | MissionPhase::Completed | MissionPhase::Failed
                )
        })
    {
        return Vec::new();
    }
    let mut rows: Vec<_> = gifts
        .par_iter()
        .map(|gift| {
            (
                (gift.region, gift.giver, gift.recipient, gift.good),
                gift.quantity,
            )
        })
        .collect();
    if rows.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        rows.par_sort_unstable_by_key(|row| row.0);
    }
    let sums = if rows.par_windows(2).any(|pair| pair[0].0 == pair[1].0) {
        rows.par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| {
                (
                    group[0].0,
                    group
                        .par_iter()
                        .map(|row| row.1)
                        .reduce(|| 0, u64::saturating_add),
                )
            })
            .collect()
    } else {
        rows
    };
    state
        .entries
        .par_iter_mut()
        .filter_map(|mission| {
            let Some(MissionObjective::Deliver {
                person,
                good,
                quantity,
                delivered,
            }) = mission.objective
            else {
                return None;
            };
            if mission.objective_met
                || matches!(
                    mission.phase,
                    MissionPhase::Ready | MissionPhase::Completed | MissionPhase::Failed
                )
            {
                return None;
            }
            let index = sums
                .binary_search_by_key(
                    &(mission.destination, mission.person, person, good),
                    |row| row.0,
                )
                .ok()?;
            let delivered = delivered.saturating_add(sums[index].1).min(quantity);
            mission.objective = Some(MissionObjective::Deliver {
                person,
                good,
                quantity,
                delivered,
            });
            if delivered < quantity {
                return None;
            }
            mission.objective_met = true;
            Some(MissionEvent {
                tick,
                mission: mission.id,
                person: mission.person,
                kind: MissionEventKind::ObjectiveMet,
            })
        })
        .collect()
}

pub fn meetings(state: &mut Missions, encounters: &[Encounter], tick: u64) -> Vec<MissionEvent> {
    if encounters.is_empty()
        || !state.entries.par_iter().any(|mission| {
            matches!(mission.objective, Some(MissionObjective::Meet { .. }))
                && !mission.objective_met
                && !matches!(
                    mission.phase,
                    MissionPhase::Ready | MissionPhase::Completed | MissionPhase::Failed
                )
        })
    {
        return Vec::new();
    }
    let mut pairs: Vec<_> = encounters
        .par_iter()
        .filter(|event| event.tick == tick)
        .map(|event| (event.region, event.a.min(event.b), event.a.max(event.b)))
        .collect();
    pairs.par_sort_unstable();
    state
        .entries
        .par_iter_mut()
        .filter_map(|mission| {
            let Some(MissionObjective::Meet { person }) = mission.objective else {
                return None;
            };
            if mission.objective_met
                || matches!(
                    mission.phase,
                    MissionPhase::Ready | MissionPhase::Completed | MissionPhase::Failed
                )
            {
                return None;
            }
            let key = (
                mission.destination,
                mission.person.min(person),
                mission.person.max(person),
            );
            if pairs.binary_search(&key).is_err() {
                return None;
            }
            mission.objective_met = true;
            Some(MissionEvent {
                tick,
                mission: mission.id,
                person: mission.person,
                kind: MissionEventKind::ObjectiveMet,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "../tests/support/owner.rs"]
mod tests;
