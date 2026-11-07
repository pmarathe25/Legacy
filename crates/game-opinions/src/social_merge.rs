//! Balanced merge of sorted social trust updates with existing directed opinions.
use crate::{Event, Opinion, Rules};
use rayon::prelude::*;

type Update = ((u64, u64), i128);
fn key(row: &Opinion) -> (u64, u64) {
    (row.observer, row.target)
}

// Keep an existing row and its equal-key update in the same partition.
fn boundary(old: &[Opinion], updates: &[Update], rank: usize) -> (usize, usize) {
    let mut low = rank.saturating_sub(updates.len());
    let mut high = rank.min(old.len());
    while low <= high {
        let i = low + (high - low) / 2;
        let j = rank - i;
        if i > 0 && j < updates.len() && key(&old[i - 1]) > updates[j].0 {
            high = i - 1;
        } else if j > 0 && i < old.len() && updates[j - 1].0 >= key(&old[i]) {
            low = i + 1;
        } else {
            return if i > 0 && j < updates.len() && key(&old[i - 1]) == updates[j].0 {
                (i - 1, j)
            } else {
                (i, j)
            };
        }
    }
    unreachable!("validated sorted opinion and update streams")
}

pub fn merge(
    old: Vec<Opinion>,
    updates: &[Update],
    rules: &Rules,
    tick: u64,
) -> (Vec<Opinion>, Vec<Event>) {
    let events = updates
        .par_iter()
        .map(|&((observer, target), delta)| {
            let before = old
                .binary_search_by_key(&(observer, target), key)
                .ok()
                .map_or(rules.initial, |index| old[index].trust);
            let after = (i128::from(before) + delta)
                .clamp(i128::from(rules.minimum), i128::from(rules.maximum))
                as i32;
            Event {
                tick,
                observer,
                target,
                before,
                after,
            }
        })
        .collect();
    let total = old
        .len()
        .checked_add(updates.len())
        .expect("opinion merge size overflow");
    if total == 0 {
        return (old, events);
    }
    let workers = rayon::current_num_threads();
    let chunk_size = if workers == 1 {
        total
    } else {
        total.div_ceil(workers.saturating_mul(8)).max(1024)
    };
    let rows = (0..total.div_ceil(chunk_size))
        .into_par_iter()
        .flat_map_iter(|chunk| {
            let start = chunk * chunk_size;
            let end = start.saturating_add(chunk_size).min(total);
            let (old_start, update_start) = boundary(&old, updates, start);
            let (old_end, update_end) = boundary(&old, updates, end);
            let mut cursor = old_start;
            let mut merged = Vec::with_capacity(old_end - old_start + update_end - update_start);
            for &((observer, target), delta) in &updates[update_start..update_end] {
                while cursor < old_end && key(&old[cursor]) < (observer, target) {
                    merged.push(old[cursor].clone());
                    cursor += 1;
                }
                let mut opinion = if cursor < old_end && key(&old[cursor]) == (observer, target) {
                    let row = old[cursor].clone();
                    cursor += 1;
                    row
                } else {
                    Opinion {
                        observer,
                        target,
                        trust: rules.initial,
                        fear: 0,
                        obligation: 0,
                        affinity: 0,
                    }
                };
                opinion.trust = (i128::from(opinion.trust) + delta)
                    .clamp(i128::from(rules.minimum), i128::from(rules.maximum))
                    as i32;
                merged.push(opinion);
            }
            merged.extend_from_slice(&old[cursor..old_end]);
            merged
        })
        .collect();
    (rows, events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn balanced_merge_preserves_existing_feelings_and_all_trust_events() {
        let rules = Rules {
            minimum: -10,
            maximum: 10,
            initial: 1,
            fulfilled_gain: 0,
            default_loss: 0,
            theft_loss: 0,
            encounter_gain: 0,
            fear_per_damage: 0,
            fear_decay_per_tick: 0,
            gratitude_gain: 0,
            obligation_relief: 0,
            minimum_report_trust: None,
            affinity: None,
            newborn_obligation_development: None,
        };
        let old: Vec<_> = (0..5000u64)
            .map(|i| Opinion {
                observer: i * 2 + 1,
                target: 20_001,
                trust: if i % 2 == 0 { 10 } else { -2 },
                fear: i,
                obligation: i + 1,
                affinity: -3,
            })
            .collect();
        for step in [3usize, 997usize] {
            let updates: Vec<_> = (1..=10_001u64)
                .step_by(step)
                .map(|observer| ((observer, 20_001), if observer % 2 == 0 { -4 } else { 5 }))
                .collect();
            let mut reference: BTreeMap<_, _> =
                old.iter().cloned().map(|row| (key(&row), row)).collect();
            let mut expected_events = Vec::new();
            for &(pair, delta) in &updates {
                let row = reference.entry(pair).or_insert(Opinion {
                    observer: pair.0,
                    target: pair.1,
                    trust: rules.initial,
                    fear: 0,
                    obligation: 0,
                    affinity: 0,
                });
                let before = row.trust;
                row.trust = (i128::from(before) + delta)
                    .clamp(i128::from(rules.minimum), i128::from(rules.maximum))
                    as i32;
                expected_events.push(Event {
                    tick: 9,
                    observer: pair.0,
                    target: pair.1,
                    before,
                    after: row.trust,
                });
            }
            let expected_rows: Vec<_> = reference.into_values().collect();
            for workers in [1, 2, 4, 6] {
                let (rows, events) = rayon::ThreadPoolBuilder::new()
                    .num_threads(workers)
                    .build()
                    .unwrap()
                    .install(|| merge(old.clone(), &updates, &rules, 9));
                assert_eq!(rows, expected_rows);
                assert_eq!(events, expected_events);
            }
        }
    }
}
