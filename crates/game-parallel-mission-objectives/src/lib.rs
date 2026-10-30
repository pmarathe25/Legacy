//! Resolve meeting goals from actual, consenting social encounters.
use game_parallel_missions::{
    Event as MissionEvent, EventKind as MissionEventKind, Missions, Objective as MissionObjective,
    Phase as MissionPhase,
};
use game_parallel_social::Encounter;
use rayon::prelude::*;

pub fn meetings(state: &mut Missions, encounters: &[Encounter], tick: u64) -> Vec<MissionEvent> {
    if encounters.is_empty()
        || !state.entries.par_iter().any(|mission| {
            mission.objective.is_some()
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
