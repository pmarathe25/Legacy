//! Isolate consumptions whose custody is touched by no other transaction.
use game_lot_model::{Place, State, Transaction};
use game_lot_planning::Draft;
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};

pub fn prepare(
    state: &State,
    instance: u64,
    seed: u64,
    transactions: &[Transaction],
    advance: bool,
) -> Result<Option<Draft>, String> {
    if transactions.len() < 256 {
        return Ok(None);
    }
    let candidate = |row: &Transaction| {
        row.movements.len() == 1 && row.creations.is_empty() && {
            let m = row.movements[0];
            m.to.is_none()
                && m.item.is_none()
                && m.id != 0
                && m.quantity != 0
                && m.from.accepts(m.good)
        }
    };
    if transactions.par_iter().filter(|row| candidate(row)).count() < 128 {
        return Ok(None);
    }
    // A single hot source cannot contribute independent consumptions. Avoid
    // classifying and sorting the full batch in this common contested case.
    let source = transactions
        .par_iter()
        .find_any(|row| candidate(row))
        .expect("candidate count was checked")
        .movements[0]
        .from;
    if transactions
        .par_iter()
        .filter(|row| candidate(row))
        .all(|row| row.movements[0].from == source)
    {
        return Ok(None);
    }
    // All movement and creation IDs share one namespace, including across partitions.
    let mut ids: Vec<_> = transactions
        .par_iter()
        .flat_map_iter(|row| {
            row.movements
                .iter()
                .map(|m| m.id)
                .chain(row.creations.iter().map(|c| c.id))
        })
        .collect();
    if ids.par_windows(2).any(|p| p[0] > p[1]) {
        ids.par_sort_unstable();
    }
    if ids.first() == Some(&0) || ids.par_windows(2).any(|p| p[0] == p[1]) {
        return Ok(None);
    }
    let mut endpoints: Vec<(Place, usize)> = transactions
        .par_iter()
        .enumerate()
        .flat_map_iter(|(index, row)| {
            row.movements
                .iter()
                .flat_map(move |m| {
                    std::iter::once((m.from, index)).chain(m.to.map(|to| (to, index)))
                })
                .chain(row.creations.iter().map(move |c| (c.to, index)))
        })
        .collect();
    endpoints.par_sort_unstable();
    let contested: Vec<_> = (0..transactions.len())
        .into_par_iter()
        .map(|_| AtomicBool::new(false))
        .collect();
    endpoints
        .par_chunk_by(|a, b| a.0 == b.0)
        .filter(|group| group.len() > 1)
        .for_each(|group| {
            group.par_iter().for_each(|&(_, index)| {
                contested[index].store(true, Ordering::Relaxed);
            });
        });
    let eligible: Vec<_> = transactions
        .par_iter()
        .enumerate()
        .map(|(i, row)| !contested[i].load(Ordering::Relaxed) && candidate(row))
        .collect();
    let selected: Vec<_> = transactions
        .par_iter()
        .zip(eligible.par_iter())
        .filter_map(|(row, &yes)| yes.then_some(row))
        .collect();
    if selected.len() < 128 || selected.len() == transactions.len() {
        return Ok(None);
    }
    let remaining: Vec<_> = transactions
        .par_iter()
        .zip(eligible.par_iter())
        .filter_map(|(row, &yes)| (!yes).then_some(row))
        .collect();
    let (fast, general) = rayon::join(
        || super::independent::prepare(state, instance, &selected, &[], advance),
        || super::prepare_general(state, instance, seed, &remaining, advance, &[], &[]),
    );
    let Some(mut fast) = fast? else {
        return Ok(None);
    };
    let mut general = general?;
    let mut fast_changes = std::mem::take(&mut fast.changes).into_flat();
    let mut other_changes = std::mem::take(&mut general.changes).into_flat();
    if !general.spoiled.is_empty() {
        let mut sources: Vec<_> = selected
            .par_iter()
            .filter_map(|row| {
                state
                    .containers
                    .binary_search_by_key(&row.movements[0].from, |container| container.place)
                    .ok()
            })
            .collect();
        sources.par_sort_unstable();
        fast_changes = fast_changes
            .into_par_iter()
            .filter(|row| sources.binary_search(&row.0).is_ok())
            .collect();
        other_changes = other_changes
            .into_par_iter()
            .filter(|row| sources.binary_search(&row.0).is_err())
            .collect();
    }
    fast_changes.par_extend(other_changes);
    fast_changes.par_sort_unstable_by_key(|row| row.0);
    general.changes = fast_changes.into();
    general.outcomes.append(fast.outcomes);
    general.outcomes.sort_by_id();
    general.transactions.par_extend(fast.transactions);
    general.transactions.par_sort_unstable_by_key(|row| row.id);
    Ok(Some(general))
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_lot_model::{Container, Creation, Lot, Movement};

    #[test]
    fn mixed_consumptions_match_general_arbiter_with_conflicts_and_expiry() {
        let mut containers: Vec<_> = (1..=256)
            .map(|id| {
                Container::new(
                    Place::Person(id),
                    10,
                    vec![
                        Lot {
                            batch: None,
                            item: None,
                            good: 1,
                            quantity: 1,
                            expires_at: Some(1),
                        },
                        Lot {
                            batch: None,
                            item: None,
                            good: 1,
                            quantity: 2,
                            expires_at: Some(3),
                        },
                        Lot {
                            batch: None,
                            item: None,
                            good: 2,
                            quantity: 1,
                            expires_at: None,
                        },
                    ],
                )
                .unwrap()
            })
            .collect();
        containers.push(Container::new(Place::Person(999), 100, vec![]).unwrap());
        containers.sort_unstable_by_key(|row| row.place);
        let state = State {
            tick: 0,
            revision: 0,
            containers,
        };
        let mut actions: Vec<_> = (1..=256)
            .rev()
            .map(|id| Transaction {
                id,
                movements: vec![Movement {
                    id,
                    from: Place::Person(id),
                    to: None,
                    good: 1,
                    quantity: id % 4 + 1,
                    item: None,
                }],
                creations: vec![],
            })
            .collect();
        actions.push(Transaction {
            id: 300,
            movements: vec![Movement {
                id: 300,
                from: Place::Person(1),
                to: Some(Place::Person(999)),
                good: 1,
                quantity: 1,
                item: None,
            }],
            creations: vec![],
        });
        actions.push(Transaction {
            id: 301,
            movements: vec![],
            creations: vec![Creation {
                id: 301,
                to: Place::Person(2),
                lot: Lot {
                    batch: None,
                    item: None,
                    good: 1,
                    quantity: 1,
                    expires_at: Some(3),
                },
            }],
        });
        for workers in [1, 2, 4, 6] {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    for advance in [false, true] {
                        let fast = prepare(&state, 1, 7, &actions, advance).unwrap().unwrap();
                        let (general, _) = crate::prepare_unpartitioned_validated(
                            &state,
                            1,
                            7,
                            &actions,
                            advance,
                            vec![],
                            &[],
                        )
                        .unwrap();
                        assert_eq!(fast.transactions, general.transactions);
                        assert_eq!(fast.spoiled, general.spoiled);
                        assert_eq!(fast.creations, general.creations);
                        assert_eq!(
                            fast.outcomes.iter().collect::<Vec<_>>(),
                            general.outcomes.iter().collect::<Vec<_>>()
                        );
                        assert_eq!(fast.changes.into_flat(), general.changes.into_flat());
                    }
                });
        }
        let mut shared = actions.clone();
        for action in shared.iter_mut().filter(|row| row.id <= 256) {
            action.movements[0].from = Place::Person(1);
        }
        assert!(prepare(&state, 1, 7, &shared, true).unwrap().is_none());
        let mut duplicate = actions.clone();
        duplicate
            .iter_mut()
            .find(|row| row.id == 300)
            .unwrap()
            .movements[0]
            .id = 1;
        assert!(prepare(&state, 1, 7, &duplicate, true).unwrap().is_none());
    }
}
