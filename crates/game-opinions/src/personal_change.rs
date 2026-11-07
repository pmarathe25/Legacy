//! Simultaneous, personally chosen signed changes to directed feelings.
use crate::{Event, Opinion, Opinions, Rules};
use rayon::prelude::*;

type Key = (u64, u64);
type Update = (Key, i128, i128, i128, i128);
const MERGE_CHUNK: usize = 1024;

fn key(opinion: &Opinion) -> Key {
    (opinion.observer, opinion.target)
}

/// Co-rank in the old-before-update merge, keeping equal keys in one partition.
fn merge_boundary(old: &[Opinion], updates: &[Update], rank: usize) -> (usize, usize) {
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
    unreachable!("sorted unique relationship streams have a merge boundary")
}

fn merge_partition(old: &[Opinion], updates: &[Update], rules: &Rules) -> Vec<Opinion> {
    let mut cursor = 0;
    let mut merged = Vec::with_capacity(old.len() + updates.len());
    for &(pair, trust, affinity, fear, obligation) in updates {
        while cursor < old.len() && key(&old[cursor]) < pair {
            merged.push(old[cursor].clone());
            cursor += 1;
        }
        let mut opinion = if cursor < old.len() && key(&old[cursor]) == pair {
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
        apply_update(&mut opinion, (trust, affinity, fear, obligation), rules);
        merged.push(opinion);
    }
    merged.extend_from_slice(&old[cursor..]);
    merged
}

fn apply_update(opinion: &mut Opinion, change: (i128, i128, i128, i128), rules: &Rules) {
    let (trust, affinity, fear, obligation) = change;
    opinion.trust = (i128::from(opinion.trust) + trust)
        .clamp(i128::from(rules.minimum), i128::from(rules.maximum)) as i32;
    if let Some(rules) = &rules.affinity {
        let limit = i128::from(rules.limit);
        opinion.affinity = (i128::from(opinion.affinity) + affinity).clamp(-limit, limit) as i32;
    }
    opinion.fear = (i128::from(opinion.fear) + fear).clamp(0, i128::from(u64::MAX)) as u64;
    opinion.obligation =
        (i128::from(opinion.obligation) + obligation).clamp(0, i128::from(u64::MAX)) as u64;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PersonalChange {
    pub observer: u64,
    pub target: u64,
    pub trust: i32,
    pub affinity: i32,
    pub fear: i64,
    pub obligation: i64,
}

fn new_opinion(row: PersonalChange, rules: &Rules) -> Opinion {
    let mut opinion = Opinion {
        observer: row.observer,
        target: row.target,
        trust: rules.initial,
        fear: 0,
        obligation: 0,
        affinity: 0,
    };
    apply_update(
        &mut opinion,
        (
            i128::from(row.trust),
            i128::from(row.affinity),
            i128::from(row.fear),
            i128::from(row.obligation),
        ),
        rules,
    );
    opinion
}

pub fn apply(state: &mut Opinions, changes: &[PersonalChange], tick: u64) {
    let rows = changes
        .par_iter()
        .copied()
        .filter(|row| effective(row, &state.rules))
        .collect();
    apply_filtered(state, rows, tick);
}

pub(crate) fn apply_owned(state: &mut Opinions, mut rows: Vec<PersonalChange>, tick: u64) {
    if rows.par_iter().any(|row| !effective(row, &state.rules)) {
        rows = rows
            .into_par_iter()
            .filter(|row| effective(row, &state.rules))
            .collect();
    }
    apply_filtered(state, rows, tick);
}

fn effective(row: &PersonalChange, rules: &Rules) -> bool {
    row.observer != row.target
        && (row.trust != 0
            || row.fear != 0
            || row.obligation != 0
            || (row.affinity != 0 && rules.affinity.is_some()))
}

fn apply_filtered(state: &mut Opinions, mut rows: Vec<PersonalChange>, tick: u64) {
    if rows.is_empty() {
        return;
    }
    if state.records.is_empty()
        && rows
            .par_windows(2)
            .all(|pair| (pair[0].observer, pair[0].target) < (pair[1].observer, pair[1].target))
    {
        let rules = &state.rules;
        let all_trust_changes = rows.par_iter().all(|row| {
            (row.trust > 0 && rules.initial < rules.maximum)
                || (row.trust < 0 && rules.initial > rules.minimum)
        });
        if all_trust_changes {
            let events: Vec<_> = rows
                .par_iter()
                .map(|row| Event {
                    tick,
                    observer: row.observer,
                    target: row.target,
                    before: rules.initial,
                    after: (i128::from(rules.initial) + i128::from(row.trust))
                        .clamp(i128::from(rules.minimum), i128::from(rules.maximum))
                        as i32,
                })
                .collect();
            state.records = rows
                .into_par_iter()
                .map(|row| new_opinion(row, rules))
                .collect();
            if state.events.is_empty() {
                state.events = events;
            } else {
                state.events.par_extend(events);
            }
            return;
        }
        let events: Vec<_> = rows
            .par_iter()
            .filter_map(|row| {
                let after = (i128::from(rules.initial) + i128::from(row.trust))
                    .clamp(i128::from(rules.minimum), i128::from(rules.maximum))
                    as i32;
                (after != rules.initial).then_some(Event {
                    tick,
                    observer: row.observer,
                    target: row.target,
                    before: rules.initial,
                    after,
                })
            })
            .collect();
        state.records = rows
            .into_par_iter()
            .filter_map(|row| {
                if row.trust == 0
                    && (row.affinity == 0 || rules.affinity.is_none())
                    && row.fear <= 0
                    && row.obligation <= 0
                {
                    return None;
                }
                Some(new_opinion(row, rules))
            })
            .collect();
        if state.events.is_empty() {
            state.events = events;
        } else {
            state.events.par_extend(events);
        }
        return;
    }
    if rows
        .par_windows(2)
        .any(|pair| (pair[0].observer, pair[0].target) > (pair[1].observer, pair[1].target))
    {
        rows.par_sort_unstable_by_key(|row| (row.observer, row.target));
    }
    let duplicate_pairs = rows
        .par_windows(2)
        .any(|pair| (pair[0].observer, pair[0].target) == (pair[1].observer, pair[1].target));
    let updates: Vec<_> = if duplicate_pairs {
        rows.par_chunk_by(|a, b| (a.observer, a.target) == (b.observer, b.target))
            .map(|group| {
                let (trust, affinity, fear, obligation) = group
                    .par_iter()
                    .map(|row| {
                        (
                            i128::from(row.trust),
                            i128::from(row.affinity),
                            i128::from(row.fear),
                            i128::from(row.obligation),
                        )
                    })
                    .reduce(
                        || (0, 0, 0, 0),
                        |a, b| (a.0 + b.0, a.1 + b.1, a.2 + b.2, a.3 + b.3),
                    );
                (
                    (group[0].observer, group[0].target),
                    trust,
                    affinity,
                    fear,
                    obligation,
                )
            })
            .filter(|row| {
                row.1 != 0
                    || row.3 != 0
                    || row.4 != 0
                    || (row.2 != 0 && state.rules.affinity.is_some())
            })
            .collect()
    } else {
        rows.into_par_iter()
            .map(|row| {
                (
                    (row.observer, row.target),
                    i128::from(row.trust),
                    i128::from(row.affinity),
                    i128::from(row.fear),
                    i128::from(row.obligation),
                )
            })
            .collect()
    };
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
    let updates: Vec<_> = updates
        .into_par_iter()
        .filter(|row| {
            row.1 != 0
                || (row.2 != 0 && state.rules.affinity.is_some())
                || row.3 > 0
                || row.4 > 0
                || old
                    .binary_search_by_key(&row.0, |opinion| (opinion.observer, opinion.target))
                    .is_ok()
        })
        .collect();
    if updates.is_empty() {
        state.records = old;
        return;
    }
    let rules = &state.rules;
    let events: Vec<_> = updates
        .par_iter()
        .filter_map(|&(pair, trust, _, _, _)| {
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
    // A sparse change to existing edges can mutate this owned draft in place.
    // Any insertion falls back to the balanced merge without touching old data.
    if old.len() >= MERGE_CHUNK && updates.len() <= old.len() / 8 {
        let indices: Vec<_> = updates
            .par_iter()
            .map(|row| old.binary_search_by_key(&row.0, key).ok())
            .collect();
        if indices.par_iter().all(Option::is_some) {
            old.par_chunks_mut(MERGE_CHUNK)
                .enumerate()
                .for_each(|(chunk_index, chunk)| {
                    let start = chunk_index * MERGE_CHUNK;
                    let end = start + chunk.len();
                    let first =
                        indices.partition_point(|index| index.is_some_and(|value| value < start));
                    let last =
                        indices.partition_point(|index| index.is_some_and(|value| value < end));
                    for index in first..last {
                        let update = updates[index];
                        let position =
                            indices[index].expect("all sparse updates target existing edges");
                        apply_update(
                            &mut chunk[position - start],
                            (update.1, update.2, update.3, update.4),
                            rules,
                        );
                    }
                });
            state.records = old;
            state.events.par_extend(events);
            return;
        }
    }
    let total = old
        .len()
        .checked_add(updates.len())
        .expect("relationship merge size overflow");
    let workers = rayon::current_num_threads();
    let chunk_size = if workers == 1 {
        total
    } else {
        total.div_ceil(workers.saturating_mul(8)).max(MERGE_CHUNK)
    };
    state.records = (0..total.div_ceil(chunk_size))
        .into_par_iter()
        .flat_map_iter(|chunk| {
            let start = chunk * chunk_size;
            let end = start.saturating_add(chunk_size).min(total);
            let (old_start, update_start) = merge_boundary(&old, &updates, start);
            let (old_end, update_end) = merge_boundary(&old, &updates, end);
            merge_partition(
                &old[old_start..old_end],
                &updates[update_start..update_end],
                rules,
            )
        })
        .collect();
    state.events.par_extend(events);
}

#[cfg(test)]
mod merge_tests {
    use super::*;

    #[test]
    fn merge_boundaries_keep_equal_keys_together_for_all_small_inputs() {
        let rules = Rules {
            minimum: -10,
            maximum: 10,
            initial: 0,
            fulfilled_gain: 0,
            default_loss: 0,
            theft_loss: 0,
            encounter_gain: 0,
            minimum_report_trust: None,
            affinity: None,
            newborn_obligation_development: None,
            fear_per_damage: 0,
            fear_decay_per_tick: 0,
            gratitude_gain: 0,
            obligation_relief: 0,
        };
        for old_mask in 0u32..64 {
            for update_mask in 0u32..64 {
                let old: Vec<_> = (0..6)
                    .filter(|i| old_mask & (1 << i) != 0)
                    .map(|i| Opinion {
                        observer: i + 1,
                        target: 99,
                        trust: 0,
                        affinity: 0,
                        fear: 0,
                        obligation: 0,
                    })
                    .collect();
                let updates: Vec<Update> = (0..6)
                    .filter(|i| update_mask & (1 << i) != 0)
                    .map(|i| ((i + 1, 99), 1, 0, 0, 0))
                    .collect();
                let total = old.len() + updates.len();
                let expected = merge_partition(&old, &updates, &rules);
                let mut previous = (0, 0);
                for rank in 0..=total {
                    let (i, j) = merge_boundary(&old, &updates, rank);
                    assert!(i >= previous.0 && j >= previous.1);
                    assert!(i + j == rank || i + j + 1 == rank);
                    assert!(!(i > 0 && j < updates.len() && key(&old[i - 1]) == updates[j].0));
                    assert!(!(j > 0 && i < old.len() && updates[j - 1].0 == key(&old[i])));
                    previous = (i, j);
                }
                for width in 1..=4 {
                    let mut actual = Vec::new();
                    for chunk in 0..total.div_ceil(width) {
                        let (a, b) = merge_boundary(&old, &updates, chunk * width);
                        let (c, d) =
                            merge_boundary(&old, &updates, ((chunk + 1) * width).min(total));
                        actual.extend(merge_partition(&old[a..c], &updates[b..d], &rules));
                    }
                    assert_eq!(
                        actual, expected,
                        "old={old_mask} updates={update_mask} width={width}"
                    );
                }
            }
        }
    }
}
