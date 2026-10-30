//! Prepare sparse mission edits and independent departure intents.
use crate::{Departure, Draft, Event, EventKind, Mission, Missions, Phase, party, routing};
use game_parallel_production_model::PersonSnapshot;
use game_parallel_transport::Route;
use rayon::prelude::*;

pub(crate) struct Edit {
    pub index: usize,
    pub phase: Phase,
    pub remaining: u64,
    pub event: Option<Event>,
}
fn decide(
    mission: &Mission,
    index: usize,
    person: Option<PersonSnapshot>,
    busy: &[u64],
    paths: &routing::Routing,
    tick: u64,
) -> (Option<Edit>, Option<Departure>) {
    if matches!(mission.phase, Phase::Completed | Phase::Failed) {
        return (None, None);
    }
    let Some((_, location, health, _)) = person else {
        return (None, None);
    };
    let mut phase = mission.phase;
    let mut remaining = mission.remaining;
    let mut event = None;
    let mut departure = None;
    if health == 0 {
        phase = Phase::Failed;
        remaining = 0;
        event = Some(EventKind::Died);
    } else {
        match mission.phase {
            Phase::Ready | Phase::Outbound
                if location.is_some_and(|region| region != mission.destination)
                    && (mission.phase == Phase::Outbound || location == Some(mission.origin))
                    && busy.binary_search(&mission.person).is_err() =>
            {
                let source = location.expect("local departure");
                departure = paths
                    .next(source, mission.destination)
                    .map(|destination| Departure {
                        mission: mission.id,
                        person: mission.person,
                        party: mission.party,
                        index,
                        source,
                        destination,
                        outbound: true,
                    });
            }
            Phase::Outbound if location == Some(mission.destination) => {
                phase = Phase::Visiting;
                remaining = mission.visit_ticks;
                event = Some(EventKind::Arrival);
            }
            Phase::Visiting if location == Some(mission.destination) => {
                remaining = mission.remaining.saturating_sub(1);
                if remaining == 0 {
                    phase = Phase::Returning;
                    event = Some(EventKind::VisitComplete);
                }
            }
            Phase::Returning
                if location.is_some_and(|region| region != mission.origin)
                    && busy.binary_search(&mission.person).is_err() =>
            {
                let source = location.expect("local departure");
                departure = paths
                    .next(source, mission.origin)
                    .map(|destination| Departure {
                        mission: mission.id,
                        person: mission.person,
                        party: mission.party,
                        index,
                        source,
                        destination,
                        outbound: false,
                    });
            }
            Phase::Returning if location == Some(mission.origin) => {
                let failed = mission.objective.is_some() && !mission.objective_met;
                phase = if failed {
                    Phase::Failed
                } else {
                    Phase::Completed
                };
                event = Some(if failed {
                    EventKind::ObjectiveFailed
                } else {
                    EventKind::Completed
                });
            }
            _ => {}
        }
    }
    let edit = (phase != mission.phase || remaining != mission.remaining).then(|| Edit {
        index,
        phase,
        remaining,
        event: event.map(|kind| Event {
            tick,
            mission: mission.id,
            person: mission.person,
            kind,
        }),
    });
    (edit, departure)
}

pub(crate) fn prepare(
    owner: &Missions,
    person_view: &(impl Fn(u64) -> Option<PersonSnapshot> + Sync),
    busy: &[u64],
    routes: &[Route],
    tick: u64,
) -> Option<Draft> {
    if owner
        .entries
        .par_iter()
        .all(|mission| matches!(mission.phase, Phase::Completed | Phase::Failed))
    {
        return None;
    }
    let targets = routing::targets(&owner.entries, true);
    let paths = routing::Routing::new(routes, &targets);
    let mut busy = busy.to_vec();
    if busy.par_windows(2).any(|pair| pair[0] > pair[1]) {
        busy.par_sort_unstable();
    }
    let (mut edits, mut departures) = owner
        .entries
        .par_iter()
        .enumerate()
        .fold(
            || (Vec::new(), Vec::new()),
            |(mut edits, mut departures), (index, mission)| {
                if !matches!(mission.phase, Phase::Completed | Phase::Failed) {
                    let (edit, departure) = decide(
                        mission,
                        index,
                        person_view(mission.person),
                        &busy,
                        &paths,
                        tick,
                    );
                    edits.extend(edit);
                    departures.extend(departure);
                }
                (edits, departures)
            },
        )
        .reduce(
            || (Vec::new(), Vec::new()),
            |(mut a, mut b), (c, d)| {
                a.extend(c);
                b.extend(d);
                (a, b)
            },
        );
    if edits
        .par_windows(2)
        .any(|pair| pair[0].index > pair[1].index)
    {
        edits.par_sort_unstable_by_key(|edit| edit.index);
    }
    if departures
        .par_windows(2)
        .any(|pair| pair[0].index > pair[1].index)
    {
        departures.par_sort_unstable_by_key(|row| row.index);
    }
    if departures
        .par_iter()
        .any(|departure| departure.party.is_some())
    {
        let ready = party::ready(&owner.entries, &departures, &edits);
        departures = departures
            .into_par_iter()
            .filter(|departure| {
                departure
                    .party
                    .is_none_or(|id| ready.binary_search(&id).is_ok())
            })
            .collect();
    }
    let actors = departures
        .par_iter()
        .map(|departure| departure.person)
        .collect();
    Some(Draft {
        edits,
        departures,
        actors,
    })
}
