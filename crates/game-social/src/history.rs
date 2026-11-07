//! Balanced publication of retained relationships and new encounters.
use crate::{Encounter, Relationship, Rules};
use rayon::prelude::*;

fn relationship_key(row: &Relationship) -> (u64, u64) {
    (row.a, row.b)
}
fn event_key(row: &Encounter) -> (u64, u64) {
    (row.a, row.b)
}

// Equal keys stay in the same partition so an encounter updates its existing row.
fn boundary(old: &[Relationship], events: &[Encounter], rank: usize) -> (usize, usize) {
    let mut low = rank.saturating_sub(events.len());
    let mut high = rank.min(old.len());
    while low <= high {
        let i = low + (high - low) / 2;
        let j = rank - i;
        if i > 0 && j < events.len() && relationship_key(&old[i - 1]) > event_key(&events[j]) {
            high = i - 1;
        } else if j > 0 && i < old.len() && event_key(&events[j - 1]) >= relationship_key(&old[i]) {
            low = i + 1;
        } else {
            return if i > 0
                && j < events.len()
                && relationship_key(&old[i - 1]) == event_key(&events[j])
            {
                (i - 1, j)
            } else {
                (i, j)
            };
        }
    }
    unreachable!("sorted unique social history and encounters")
}

pub fn merge(
    old: &[Relationship],
    events: &[Encounter],
    rules: &Rules,
    tick: u64,
) -> Vec<Relationship> {
    if events.is_empty() {
        return old.par_iter().cloned().collect();
    }
    let total = old
        .len()
        .checked_add(events.len())
        .expect("social history merge size overflow");
    let workers = rayon::current_num_threads();
    let chunk_size = if workers == 1 {
        total
    } else {
        total.div_ceil(workers.saturating_mul(8)).max(1024)
    };
    (0..total.div_ceil(chunk_size))
        .into_par_iter()
        .flat_map_iter(|chunk| {
            let rank_start = chunk * chunk_size;
            let rank_end = rank_start.saturating_add(chunk_size).min(total);
            let (old_start, event_start) = boundary(old, events, rank_start);
            let (old_end, event_end) = boundary(old, events, rank_end);
            let mut cursor = old_start;
            let mut result = Vec::with_capacity(old_end - old_start + event_end - event_start);
            for event in &events[event_start..event_end] {
                let key = event_key(event);
                while cursor < old_end && relationship_key(&old[cursor]) < key {
                    result.push(old[cursor].clone());
                    cursor += 1;
                }
                if cursor < old_end && relationship_key(&old[cursor]) == key {
                    let mut relationship = old[cursor].clone();
                    relationship.familiarity = relationship
                        .familiarity
                        .saturating_add(rules.familiarity_gain)
                        .min(rules.familiarity_max);
                    relationship.encounters += 1;
                    relationship.last_met = tick;
                    result.push(relationship);
                    cursor += 1;
                } else {
                    result.push(Relationship {
                        a: event.a,
                        b: event.b,
                        familiarity: rules.familiarity_gain.min(rules.familiarity_max),
                        encounters: 1,
                        last_met: tick,
                    });
                }
            }
            result.extend_from_slice(&old[cursor..old_end]);
            result
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn balanced_history_merge_handles_sparse_and_dense_meetings() {
        let rules = Rules {
            interval: 1,
            min_health: 1,
            max_hunger: 10,
            familiarity_gain: 2,
            familiarity_max: 10,
            relationship_retention: None,
        };
        let old: Vec<_> = (0..5000u64)
            .map(|i| Relationship {
                a: i * 2 + 1,
                b: 20_001,
                familiarity: if i % 2 == 0 { 10 } else { 3 },
                encounters: i + 1,
                last_met: 1,
            })
            .collect();
        for step in [3usize, 997usize] {
            let events: Vec<_> = (1..=10_001u64)
                .step_by(step)
                .map(|a| Encounter {
                    a,
                    b: 20_001,
                    tick: 9,
                    region: 1,
                    pressured: None,
                    also_pressured: None,
                })
                .collect();
            let mut expected: BTreeMap<_, _> = old
                .iter()
                .cloned()
                .map(|row| (relationship_key(&row), row))
                .collect();
            for event in &events {
                let row = expected.entry(event_key(event)).or_insert(Relationship {
                    a: event.a,
                    b: event.b,
                    familiarity: 0,
                    encounters: 0,
                    last_met: 0,
                });
                row.familiarity = row
                    .familiarity
                    .saturating_add(rules.familiarity_gain)
                    .min(rules.familiarity_max);
                row.encounters += 1;
                row.last_met = 9;
            }
            let expected: Vec<_> = expected.into_values().collect();
            for workers in [1, 2, 4, 6] {
                let actual = rayon::ThreadPoolBuilder::new()
                    .num_threads(workers)
                    .build()
                    .unwrap()
                    .install(|| merge(&old, &events, &rules, 9));
                assert_eq!(actual, expected);
            }
        }
    }
}
