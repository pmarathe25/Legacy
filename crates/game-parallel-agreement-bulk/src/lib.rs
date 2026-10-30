//! Explicit bulk fulfillment of accepted physical obligations.
use game_parallel_agreements::{Agreements, Contract, Event, EventKind, Status};
use game_parallel_goods_cargo::{GoodsCargo, GoodsHolding};
use game_parallel_production_model::{Amount, PersonSnapshot, person_index};
use rayon::prelude::*;

const CHUNK: usize = 1024;
struct Candidate<'a> {
    contract: &'a Contract,
    quantity: u64,
    granted: bool,
}
pub struct Draft {
    pub actors: Vec<u64>,
    pub updates: Vec<Contract>,
    pub events: Vec<Event>,
    pub holdings: Vec<(u64, Option<GoodsHolding>)>,
}
/// Stage independent recipients against one committed custody snapshot.
/// A recipient receives at most one obligation; authorized debtors may pay many.
pub fn prepare(
    state: &Agreements,
    cargo: &GoodsCargo,
    people: &[PersonSnapshot],
    busy: &[u64],
    tick: u64,
    opinions: Option<&game_parallel_opinions::Opinions>,
) -> Option<Draft> {
    if state.bulk_debtors.is_empty() {
        return None;
    }
    let mut bulk = state.bulk_debtors.clone();
    bulk.par_sort_unstable();
    let thresholds = if state.policies.par_iter().any(|policy| {
        policy.minimum_honor_trust.is_some()
            || policy.fear_honor_threshold.is_some()
            || policy.obligation_honor_threshold.is_some()
    }) {
        let mut rows: Vec<_> = state
            .policies
            .par_iter()
            .filter(|policy| {
                policy.minimum_honor_trust.is_some()
                    || policy.fear_honor_threshold.is_some()
                    || policy.obligation_honor_threshold.is_some()
            })
            .collect();
        rows.par_sort_unstable_by_key(|row| row.person);
        Some((rows, opinions.map(|owner| owner.index())))
    } else {
        None
    };
    let mut busy = busy.to_vec();
    if busy.par_windows(2).any(|p| p[0] > p[1]) {
        busy.par_sort_unstable();
    }
    let mut holdings: Vec<_> = cargo.holdings.par_iter().collect();
    if holdings.par_windows(2).any(|p| p[0].person > p[1].person) {
        holdings.par_sort_unstable_by_key(|h| h.person);
    }
    let dense = holdings.first().and_then(|first| {
        (holdings.last()?.person.checked_sub(first.person) == Some((holdings.len() - 1) as u64))
            .then_some(first.person)
    });
    let lookup = |person: u64| {
        dense
            .and_then(|first| person.checked_sub(first))
            .and_then(|n| usize::try_from(n).ok())
            .filter(|&index| index < holdings.len())
            .or_else(|| {
                dense
                    .is_none()
                    .then(|| {
                        holdings
                            .binary_search_by_key(&person, |row| row.person)
                            .ok()
                    })
                    .flatten()
            })
            .map(|index| holdings[index])
    };
    let mut candidates: Vec<_> = state
        .contracts
        .par_iter()
        .filter_map(|contract| {
            if contract.status != Status::Active
                || contract.due < tick
                || bulk.binary_search(&contract.debtor).is_err()
                || busy.binary_search(&contract.debtor).is_ok()
                || busy.binary_search(&contract.creditor).is_ok()
                || thresholds.as_ref().is_some_and(|(rows, index)| {
                    rows.binary_search_by_key(&contract.debtor, |row| row.person)
                        .ok()
                        .is_some_and(|i| {
                            let assessment = index
                                .as_ref()
                                .map(|index| index.assessment(contract.debtor, contract.creditor));
                            let obligation = index
                                .as_ref()
                                .map(|index| index.obligation(contract.debtor, contract.creditor));
                            !rows[i].honors_with_feelings(
                                assessment.map(|row| row.0),
                                assessment.map(|row| row.1),
                                obligation,
                            )
                        })
                })
            {
                return None;
            }
            let a = people.get(person_index(people, contract.debtor)?)?;
            let b = people.get(person_index(people, contract.creditor)?)?;
            if a.2 == 0 || b.2 == 0 || a.1.is_none() || a.1 != b.1 {
                return None;
            }
            let quantity = contract
                .quantity
                .checked_sub(contract.delivered)
                .filter(|&q| q > 0)?;
            let from = lookup(contract.debtor)?;
            if !from
                .items
                .iter()
                .any(|item| item.good == contract.good && item.quantity >= quantity)
            {
                return None;
            }
            let to = lookup(contract.creditor);
            let carried: u128 = to.map_or(0, |holding| {
                holding
                    .items
                    .iter()
                    .map(|item| u128::from(item.quantity))
                    .sum()
            });
            if carried + u128::from(quantity) > u128::from(cargo.capacity)
                || to
                    .and_then(|holding| {
                        holding.items.iter().find(|item| item.good == contract.good)
                    })
                    .is_some_and(|item| item.quantity.checked_add(quantity).is_none())
            {
                return None;
            }
            Some(Candidate {
                contract,
                quantity,
                granted: false,
            })
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }
    // Debtors acting as payers do not simultaneously collect another payer's transfer.
    let mut debtors: Vec<_> = candidates
        .par_chunks(CHUNK)
        .flat_map_iter(|chunk| {
            let mut ids: Vec<_> = chunk.iter().map(|row| row.contract.debtor).collect();
            ids.sort_unstable();
            ids.dedup();
            ids
        })
        .collect();
    debtors.par_sort_unstable();
    debtors.dedup();
    candidates = candidates
        .into_par_iter()
        .filter(|row| debtors.binary_search(&row.contract.creditor).is_err())
        .collect();
    if candidates.par_windows(2).any(|rows| {
        (rows[0].contract.creditor, rows[0].contract.id)
            > (rows[1].contract.creditor, rows[1].contract.id)
    }) {
        candidates.par_sort_unstable_by_key(|row| (row.contract.creditor, row.contract.id));
    }
    candidates = candidates
        .par_chunk_by(|a, b| a.contract.creditor == b.contract.creditor)
        .map(|group| Candidate {
            contract: group[0].contract,
            quantity: group[0].quantity,
            granted: false,
        })
        .collect();
    if candidates.par_windows(2).any(|rows| {
        (
            rows[0].contract.debtor,
            rows[0].contract.good,
            rows[0].contract.id,
        ) > (
            rows[1].contract.debtor,
            rows[1].contract.good,
            rows[1].contract.id,
        )
    }) {
        candidates.par_sort_unstable_by_key(|row| {
            (row.contract.debtor, row.contract.good, row.contract.id)
        });
    }
    candidates
        .par_chunk_by_mut(|a, b| {
            (a.contract.debtor, a.contract.good) == (b.contract.debtor, b.contract.good)
        })
        .for_each(|group| {
            let first = group[0].contract;
            let available = u128::from(
                lookup(first.debtor)
                    .unwrap()
                    .items
                    .iter()
                    .find(|item| item.good == first.good)
                    .unwrap()
                    .quantity,
            );
            let sums: Vec<u128> = group
                .par_chunks(CHUNK)
                .map(|chunk| chunk.iter().map(|row| u128::from(row.quantity)).sum())
                .collect();
            let mut total = 0u128;
            let starts: Vec<_> = sums
                .into_iter()
                .map(|sum| {
                    let start = total;
                    total = total.saturating_add(sum);
                    start
                })
                .collect();
            group
                .par_chunks_mut(CHUNK)
                .zip(starts.into_par_iter())
                .for_each(|(chunk, mut used)| {
                    for row in chunk {
                        used = used.saturating_add(u128::from(row.quantity));
                        row.granted = used <= available;
                    }
                });
        });
    let mut selected: Vec<_> = candidates
        .into_par_iter()
        .filter(|row| row.granted)
        .collect();
    if selected.is_empty() {
        return None;
    }
    let debits: Vec<_> = selected
        .par_chunk_by(|a, b| {
            (a.contract.debtor, a.contract.good) == (b.contract.debtor, b.contract.good)
        })
        .map(|group| {
            (
                group[0].contract.debtor,
                group[0].contract.good,
                group.par_iter().map(|row| row.quantity).sum::<u64>(),
            )
        })
        .collect();
    let mut patches: Vec<_> = debits
        .par_chunk_by(|a, b| a.0 == b.0)
        .map(|group| {
            let debtor = group[0].0;
            let mut from = lookup(debtor).unwrap().clone();
            from.items.par_iter_mut().for_each(|item| {
                if let Ok(index) = group.binary_search_by_key(&item.good, |row| row.1) {
                    item.quantity -= group[index].2;
                }
            });
            from.items.retain(|item| item.quantity > 0);
            (debtor, (!from.items.is_empty()).then_some(from))
        })
        .collect();
    patches.par_extend(selected.par_iter().map(|row| {
        let actor = row.contract.creditor;
        let mut to = lookup(actor).cloned().unwrap_or_else(|| GoodsHolding {
            person: actor,
            items: Default::default(),
        });
        if let Some(item) = to
            .items
            .iter_mut()
            .find(|item| item.good == row.contract.good)
        {
            item.quantity += row.quantity;
        } else {
            to.items.push(Amount {
                good: row.contract.good,
                quantity: row.quantity,
            });
            to.items.sort_unstable_by_key(|item| item.good);
        }
        (actor, Some(to))
    }));
    if patches.par_windows(2).any(|rows| rows[0].0 > rows[1].0) {
        patches.par_sort_unstable_by_key(|patch| patch.0);
    }
    let actors = patches.par_iter().map(|row| row.0).collect();
    if selected
        .par_windows(2)
        .any(|rows| rows[0].contract.id > rows[1].contract.id)
    {
        selected.par_sort_unstable_by_key(|row| row.contract.id);
    }
    let updates: Vec<_> = selected
        .par_iter()
        .map(|row| {
            let mut contract = row.contract.clone();
            contract.delivered = contract.quantity;
            contract.status = Status::Fulfilled;
            contract
        })
        .collect();
    let events = selected
        .par_iter()
        .map(|row| Event {
            tick,
            contract: row.contract.id,
            actor: row.contract.debtor,
            kind: EventKind::Fulfilled,
            quantity: row.quantity,
        })
        .collect();
    Some(Draft {
        actors,
        updates,
        events,
        holdings: patches,
    })
}
#[cfg(test)]
#[path = "../tests/support/owner.rs"]
mod tests;
