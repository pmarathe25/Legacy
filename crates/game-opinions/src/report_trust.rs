//! Personal trust responses to observed differences, without inferring dishonesty.
use crate::{Event, Opinions};
use game_opinions_model::Change;
use rayon::prelude::*;

pub fn enabled(state: &Opinions) -> bool {
    state.sensitivities.par_iter().any(|person| {
        person
            .trust_report_discrepancy
            .is_some_and(|rate| rate != 0)
            || person
                .relationship_cases
                .iter()
                .any(|case| case.report_discrepancy_trust.is_some_and(|rate| rate != 0))
    })
}

pub fn apply(state: &mut Opinions, observations: &[(u64, u64)], tick: u64) {
    if observations.is_empty() || !enabled(state) {
        return;
    }
    let mut people: Vec<_> = state.sensitivities.par_iter().collect();
    if people.par_windows(2).any(|p| p[0].person > p[1].person) {
        people.par_sort_unstable_by_key(|p| p.person);
    }
    let dense = people.first().zip(people.last()).and_then(|(first, last)| {
        (last.person.checked_sub(first.person) == Some((people.len() - 1) as u64))
            .then_some(first.person)
    });
    let mut cases: Vec<_> = state
        .sensitivities
        .par_iter()
        .flat_map_iter(|person| {
            person
                .relationship_cases
                .iter()
                .map(move |case| (person.person, case))
        })
        .collect();
    if cases
        .par_windows(2)
        .any(|p| (p[0].0, p[0].1.target) > (p[1].0, p[1].1.target))
    {
        cases.par_sort_unstable_by_key(|row| (row.0, row.1.target));
    }
    let mut rows: Vec<_> = observations
        .par_iter()
        .copied()
        .filter(|&(a, b)| a != b)
        .collect();
    if rows.par_windows(2).any(|p| p[0] > p[1]) {
        rows.par_sort_unstable();
    }
    let updates: Vec<_> = rows
        .par_chunk_by(|a, b| a == b)
        .filter_map(|group| {
            let pair = group[0];
            let person = if let Some(first) = dense {
                pair.0
                    .checked_sub(first)
                    .and_then(|i| usize::try_from(i).ok())
                    .and_then(|i| people.get(i))
                    .copied()
            } else {
                people
                    .binary_search_by_key(&pair.0, |p| p.person)
                    .ok()
                    .map(|i| people[i])
            };
            let rate = cases
                .binary_search_by_key(&pair, |row| (row.0, row.1.target))
                .ok()
                .and_then(|i| cases[i].1.report_discrepancy_trust)
                .or_else(|| person.and_then(|p| p.trust_report_discrepancy))
                .unwrap_or(0);
            (rate != 0).then_some((
                pair,
                (group.len() as u64).saturating_mul(u64::from(rate.unsigned_abs())),
                rate > 0,
            ))
        })
        .collect();
    let mut previous: Vec<_> = state.records.par_iter().collect();
    if previous
        .par_windows(2)
        .any(|p| (p[0].observer, p[0].target) > (p[1].observer, p[1].target))
    {
        previous.par_sort_unstable_by_key(|row| (row.observer, row.target));
    }
    let rules = &state.rules;
    let events: Vec<_> = updates
        .par_chunks(1024)
        .flat_map_iter(|chunk| {
            let previous = &previous;
            let mut index = previous.partition_point(|row| (row.observer, row.target) < chunk[0].0);
            chunk
                .iter()
                .map(move |&((observer, target), amount, positive)| {
                    let pair = (observer, target);
                    while index < previous.len()
                        && (previous[index].observer, previous[index].target) < pair
                    {
                        index += 1;
                    }
                    let before = previous
                        .get(index)
                        .filter(|row| (row.observer, row.target) == pair)
                        .map_or(rules.initial, |row| row.trust);
                    let delta = if positive {
                        i128::from(amount)
                    } else {
                        -i128::from(amount)
                    };
                    Event {
                        tick,
                        observer,
                        target,
                        before,
                        after: (i128::from(before) + delta)
                            .clamp(i128::from(rules.minimum), i128::from(rules.maximum))
                            as i32,
                    }
                })
        })
        .collect();
    state.events.par_extend(events);
    let gains: Vec<_> = updates
        .par_iter()
        .filter_map(|&(pair, amount, positive)| positive.then_some((pair, amount)))
        .collect();
    let losses: Vec<_> = updates
        .par_iter()
        .filter_map(|&(pair, amount, positive)| (!positive).then_some((pair, amount)))
        .collect();
    game_opinions_model::merge(&mut state.records, &state.rules, &gains, Change::TrustGain);
    game_opinions_model::merge(&mut state.records, &state.rules, &losses, Change::TrustLoss);
}
