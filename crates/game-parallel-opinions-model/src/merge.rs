//! Parallel merges of sorted directed feeling updates.
use crate::{Opinion, Rules};
use rayon::prelude::*;

pub enum Change {
    Fear,
    Help,
    Relief,
    Theft,
    AffinityGain,
    AffinityLoss,
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
    *records = updates
        .par_chunks(1024)
        .enumerate()
        .flat_map_iter(|(chunk_index, chunk)| {
            let start = if chunk_index == 0 {
                0
            } else {
                old.partition_point(|row| (row.observer, row.target) < chunk[0].0)
            };
            let end = if (chunk_index + 1) * 1024 >= updates.len() {
                old.len()
            } else {
                old.partition_point(|row| {
                    (row.observer, row.target) < updates[(chunk_index + 1) * 1024].0
                })
            };
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
                    Change::Theft => {
                        row.trust = (i128::from(row.trust) - i128::from(delta))
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
