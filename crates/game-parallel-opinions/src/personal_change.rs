//! Simultaneous, personally chosen signed changes to directed feelings.
use crate::{Event, Opinion, Opinions};
use rayon::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PersonalChange {
    pub observer: u64,
    pub target: u64,
    pub trust: i32,
    pub affinity: i32,
}

pub fn apply(state: &mut Opinions, changes: &[PersonalChange], tick: u64) {
    let mut rows: Vec<_> = changes
        .par_iter()
        .copied()
        .filter(|row| {
            row.observer != row.target
                && (row.trust != 0 || (row.affinity != 0 && state.rules.affinity.is_some()))
        })
        .collect();
    if rows.is_empty() {
        return;
    }
    rows.par_sort_unstable_by_key(|row| (row.observer, row.target));
    let updates: Vec<_> = rows
        .par_chunk_by(|a, b| (a.observer, a.target) == (b.observer, b.target))
        .map(|group| {
            let (trust, affinity) = group
                .par_iter()
                .map(|row| (i128::from(row.trust), i128::from(row.affinity)))
                .reduce(|| (0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
            ((group[0].observer, group[0].target), trust, affinity)
        })
        .filter(|row| row.1 != 0 || (row.2 != 0 && state.rules.affinity.is_some()))
        .collect();
    if updates.is_empty() {
        return;
    }
    let mut old = std::mem::take(&mut state.records);
    if old
        .par_windows(2)
        .any(|pair| (pair[0].observer, pair[0].target) > (pair[1].observer, pair[1].target))
    {
        old.par_sort_unstable_by_key(|row| (row.observer, row.target));
    }
    let rules = &state.rules;
    let events: Vec<_> = updates
        .par_iter()
        .filter_map(|&(pair, trust, _)| {
            if trust == 0 {
                return None;
            }
            let before = old
                .binary_search_by_key(&pair, |row| (row.observer, row.target))
                .ok()
                .map_or(rules.initial, |i| old[i].trust);
            let after = (i128::from(before) + trust)
                .clamp(i128::from(rules.minimum), i128::from(rules.maximum))
                as i32;
            (before != after).then_some(Event {
                tick,
                observer: pair.0,
                target: pair.1,
                before,
                after,
            })
        })
        .collect();
    state.records = updates
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
            for &(pair, trust, affinity) in chunk {
                while cursor < end && (old[cursor].observer, old[cursor].target) < pair {
                    merged.push(old[cursor].clone());
                    cursor += 1;
                }
                let mut opinion =
                    if cursor < end && (old[cursor].observer, old[cursor].target) == pair {
                        let row = old[cursor].clone();
                        cursor += 1;
                        row
                    } else {
                        Opinion {
                            observer: pair.0,
                            target: pair.1,
                            trust: rules.initial,
                            fear: 0,
                            obligation: 0,
                            affinity: 0,
                        }
                    };
                opinion.trust = (i128::from(opinion.trust) + trust)
                    .clamp(i128::from(rules.minimum), i128::from(rules.maximum))
                    as i32;
                if let Some(rules) = &rules.affinity {
                    let limit = i128::from(rules.limit);
                    opinion.affinity =
                        (i128::from(opinion.affinity) + affinity).clamp(-limit, limit) as i32;
                }
                merged.push(opinion);
            }
            merged.extend_from_slice(&old[cursor..end]);
            merged
        })
        .collect();
    state.events.par_extend(events);
}
