//! Integrate personal work responses with ordinary production arbitration.
use game_parallel::Autowork;
use game_parallel_autowork::{
    Plan, Policy,
    requests::{Draft, Event, Outcome},
};
use rayon::prelude::*;

pub fn include(base: &Autowork, goals: &mut Option<Autowork>, draft: &Draft, busy: &mut Vec<u64>) {
    busy.par_extend(
        draft
            .decisions
            .par_iter()
            .filter(|row| !row.help)
            .map(|row| row.worker),
    );
    let mut accepted: Vec<_> = draft.decisions.par_iter().filter(|row| row.help).collect();
    if accepted.is_empty() {
        return;
    }
    if accepted
        .par_windows(2)
        .any(|pair| pair[0].worker > pair[1].worker)
    {
        accepted.par_sort_unstable_by_key(|row| row.worker);
    }
    let combined = goals.get_or_insert_with(|| Autowork {
        next_id: base.next_id,
        policies: base.policies.par_iter().cloned().collect(),
        requests: None,
    });
    combined.policies = std::mem::take(&mut combined.policies)
        .into_par_iter()
        .filter(|row| {
            accepted
                .binary_search_by_key(&row.person, |item| item.worker)
                .is_err()
        })
        .collect();
    combined
        .policies
        .par_extend(accepted.par_iter().map(|row| Policy {
            person: row.worker,
            recipe: row.recipe,
            enabled: true,
            disregard_claims: row.disregard_claims,
        }));
    if combined
        .policies
        .par_windows(2)
        .any(|pair| pair[0].person > pair[1].person)
    {
        combined.policies.par_sort_unstable_by_key(|row| row.person);
    }
}

pub fn outcomes(draft: Draft, plan: Option<&Plan>, tick: u64) -> (Vec<Event>, Vec<u64>) {
    let mut starts: Vec<_> = plan.map_or_else(Vec::new, |plan| {
        plan.requests
            .par_iter()
            .map(|row| (row.worker, row.id, row.region, row.recipe))
            .collect()
    });
    if starts.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        starts.par_sort_unstable_by_key(|row| row.0);
    }
    let mut events: Vec<_> = draft
        .decisions
        .par_iter()
        .filter_map(|row| {
            let outcome = if !row.help {
                Outcome::Refused
            } else {
                let index = starts
                    .binary_search_by_key(&row.worker, |start| start.0)
                    .ok()?;
                let start = starts[index];
                if start.2 != row.region || start.3 != row.recipe {
                    return None;
                }
                Outcome::Started { job: start.1 }
            };
            Some(Event {
                tick,
                request: row.request,
                worker: row.worker,
                outcome,
            })
        })
        .collect();
    events.par_extend(draft.expired);
    events.par_sort_unstable_by_key(|event| event.request);
    let actors = events
        .par_iter()
        .filter_map(|event| matches!(event.outcome, Outcome::Refused).then_some(event.worker))
        .collect();
    (events, actors)
}

pub fn publish(auto: &mut Autowork, events: Vec<Event>, witnessed: &[u64], tick: u64) {
    let Some(requests) = &mut auto.requests else {
        return;
    };
    requests.record_starts(&events);
    requests.observe_refusals(&events, witnessed, tick);
    let ids: Vec<_> = events.par_iter().map(|event| event.request).collect();
    if !ids.is_empty() {
        requests.pending = std::mem::take(&mut requests.pending)
            .into_par_iter()
            .filter(|row| ids.binary_search(&row.id).is_err())
            .collect();
    }
    requests.events = events;
}
