//! Parallel merges of sorted directed feeling updates.
use crate::{Opinion, Rules};
use rayon::prelude::*;

#[derive(Clone, Copy)]
pub enum Change {
    Fear,
    Help,
    Relief,
    Theft,
    TrustGain,
    TrustLoss,
    AffinityGain,
    AffinityLoss,
}

type Update = ((u64, u64), u64);
fn key(row: &Opinion) -> (u64, u64) {
    (row.observer, row.target)
}

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
    unreachable!("sorted unique directed feeling updates")
}

pub fn merge(
    records: &mut Vec<Opinion>,
    rules: &Rules,
    updates: &[((u64, u64), u64)],
    kind: Change,
) {
    if updates.is_empty() {
        return;
    }
    let mut old = std::mem::take(records);
    if old
        .par_windows(2)
        .any(|pair| (pair[0].observer, pair[0].target) > (pair[1].observer, pair[1].target))
    {
        old.par_sort_unstable_by_key(|row| (row.observer, row.target));
    }
    let total = old
        .len()
        .checked_add(updates.len())
        .expect("feeling merge size overflow");
    let workers = rayon::current_num_threads();
    let chunk_size = if workers == 1 {
        total
    } else {
        total.div_ceil(workers.saturating_mul(8)).max(1024)
    };
    *records = (0..total.div_ceil(chunk_size))
        .into_par_iter()
        .flat_map_iter(|chunk_index| {
            let rank_start = chunk_index * chunk_size;
            let rank_end = rank_start.saturating_add(chunk_size).min(total);
            let (start, update_start) = boundary(&old, updates, rank_start);
            let (end, update_end) = boundary(&old, updates, rank_end);
            let chunk = &updates[update_start..update_end];
            let mut cursor = start;
            let mut merged = Vec::with_capacity(end - start + chunk.len());
            for &((observer, target), delta) in chunk {
                while cursor < end
                    && (old[cursor].observer, old[cursor].target) < (observer, target)
                {
                    merged.push(old[cursor].clone());
                    cursor += 1;
                }
                let mut row = if cursor < end
                    && (old[cursor].observer, old[cursor].target) == (observer, target)
                {
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
                match kind {
                    Change::Fear => row.fear = row.fear.saturating_add(delta),
                    Change::Help => row.obligation = row.obligation.saturating_add(delta),
                    Change::Relief => row.obligation = row.obligation.saturating_sub(delta),
                    Change::AffinityGain | Change::AffinityLoss => {
                        let limit =
                            i128::from(rules.affinity.as_ref().expect("affinity rules").limit);
                        let signed = if matches!(kind, Change::AffinityGain) {
                            i128::from(delta)
                        } else {
                            -i128::from(delta)
                        };
                        row.affinity =
                            (i128::from(row.affinity) + signed).clamp(-limit, limit) as i32;
                    }
                    Change::Theft | Change::TrustLoss => {
                        row.trust = (i128::from(row.trust) - i128::from(delta))
                            .clamp(i128::from(rules.minimum), i128::from(rules.maximum))
                            as i32
                    }
                    Change::TrustGain => {
                        row.trust = (i128::from(row.trust) + i128::from(delta))
                            .clamp(i128::from(rules.minimum), i128::from(rules.maximum))
                            as i32
                    }
                }
                merged.push(row);
            }
            merged.extend_from_slice(&old[cursor..end]);
            merged
        })
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AffinityRules;
    use std::collections::BTreeMap;

    #[test]
    fn balanced_feeling_merges_preserve_each_signed_and_saturating_rule() {
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
            affinity: Some(AffinityRules {
                limit: 10,
                encounter_change: 0,
                pressured_encounter_change: 0,
                help_change: 0,
                harm_change_per_damage: 0,
            }),
            newborn_obligation_development: None,
        };
        let old: Vec<_> = (0..5000u64)
            .map(|i| Opinion {
                observer: i * 2 + 1,
                target: 20_001,
                trust: if i % 2 == 0 { 9 } else { -9 },
                fear: if i % 2 == 0 { u64::MAX - 1 } else { 1 },
                obligation: if i % 2 == 0 { u64::MAX - 1 } else { 1 },
                affinity: if i % 2 == 0 { 9 } else { -9 },
            })
            .collect();
        for step in [3usize, 997usize] {
            let updates: Vec<_> = (1..=10_001u64)
                .step_by(step)
                .map(|observer| ((observer, 20_001), 2))
                .collect();
            for kind in [
                Change::Fear,
                Change::Help,
                Change::Relief,
                Change::Theft,
                Change::TrustGain,
                Change::TrustLoss,
                Change::AffinityGain,
                Change::AffinityLoss,
            ] {
                let mut expected: BTreeMap<_, _> =
                    old.iter().cloned().map(|row| (key(&row), row)).collect();
                for &(pair, delta) in &updates {
                    let row = expected.entry(pair).or_insert(Opinion {
                        observer: pair.0,
                        target: pair.1,
                        trust: rules.initial,
                        fear: 0,
                        obligation: 0,
                        affinity: 0,
                    });
                    match kind {
                        Change::Fear => row.fear = row.fear.saturating_add(delta),
                        Change::Help => row.obligation = row.obligation.saturating_add(delta),
                        Change::Relief => row.obligation = row.obligation.saturating_sub(delta),
                        Change::Theft | Change::TrustLoss => {
                            row.trust =
                                (i128::from(row.trust) - i128::from(delta)).clamp(-10, 10) as i32
                        }
                        Change::TrustGain => {
                            row.trust =
                                (i128::from(row.trust) + i128::from(delta)).clamp(-10, 10) as i32
                        }
                        Change::AffinityGain => {
                            row.affinity =
                                (i128::from(row.affinity) + i128::from(delta)).clamp(-10, 10) as i32
                        }
                        Change::AffinityLoss => {
                            row.affinity =
                                (i128::from(row.affinity) - i128::from(delta)).clamp(-10, 10) as i32
                        }
                    }
                }
                let expected: Vec<_> = expected.into_values().collect();
                for workers in [1, 2, 4, 6] {
                    let mut actual = old.clone();
                    rayon::ThreadPoolBuilder::new()
                        .num_threads(workers)
                        .build()
                        .unwrap()
                        .install(|| merge(&mut actual, &rules, &updates, kind));
                    assert_eq!(actual, expected);
                }
            }
        }
    }
}
