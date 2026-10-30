//! Parallel, physically feasible fulfillment of opted-in obligations.
mod arbitration;
mod bulk;
use game_parallel_agreements::{Agreements, Contract, Event, EventKind, Status};
use game_parallel_goods_cargo::{GoodsCargo, GoodsHolding};
use game_parallel_production_model::{Amount, PersonSnapshot, person_index};
use rayon::prelude::*;

pub struct Draft {
    pub actors: Vec<u64>,
    updates: Vec<Contract>,
    events: Vec<Event>,
    holdings: Vec<(u64, Option<GoodsHolding>)>,
}
impl Draft {
    /// Other committed activities have reserved disjoint people.
    pub fn commit(self, agreements: &mut Agreements, cargo: &mut GoodsCargo) {
        let updates = &self.updates;
        let dense = updates.first().and_then(|first| {
            updates.last().and_then(|last| {
                (last.id.checked_sub(first.id) == Some((updates.len() - 1) as u64))
                    .then_some(first.id)
            })
        });
        agreements.contracts.par_iter_mut().for_each(|contract| {
            let index = dense
                .and_then(|first| contract.id.checked_sub(first))
                .and_then(|n| usize::try_from(n).ok())
                .filter(|&i| i < updates.len())
                .or_else(|| {
                    if dense.is_none() {
                        updates
                            .binary_search_by_key(&contract.id, |update| update.id)
                            .ok()
                    } else {
                        None
                    }
                });
            if let Some(i) = index {
                contract.delivered = updates[i].delivered;
                contract.status = updates[i].status;
            }
        });
        if agreements.events.is_empty() {
            agreements.events = self.events;
        } else {
            agreements.events.extend(self.events);
            if agreements
                .events
                .par_windows(2)
                .any(|p| p[0].contract > p[1].contract)
            {
                agreements
                    .events
                    .par_sort_unstable_by_key(|event| event.contract);
            }
        }
        if self.holdings.is_empty() {
            return;
        }
        let mut old = std::mem::take(&mut cargo.holdings);
        if old
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            old.par_sort_unstable_by_key(|row| row.person);
        }
        let patches = &self.holdings;
        cargo.holdings = patches
            .par_chunks(4096)
            .enumerate()
            .flat_map_iter(|(chunk_index, chunk)| {
                let start = if chunk_index == 0 {
                    0
                } else {
                    old.partition_point(|row| row.person < chunk[0].0)
                };
                let end = if (chunk_index + 1) * 4096 >= patches.len() {
                    old.len()
                } else {
                    old.partition_point(|row| row.person < patches[(chunk_index + 1) * 4096].0)
                };
                let mut merged = Vec::with_capacity(end - start + chunk.len());
                let mut cursor = start;
                for (id, value) in chunk {
                    while cursor < end && old[cursor].person < *id {
                        merged.push(old[cursor].clone());
                        cursor += 1;
                    }
                    if cursor < end && old[cursor].person == *id {
                        cursor += 1;
                    }
                    if let Some(holding) = value {
                        merged.push(holding.clone());
                    }
                }
                merged.extend_from_slice(&old[cursor..end]);
                merged
            })
            .collect();
    }
}
/// Match the lowest eligible contract for both actors. Losers may retry later.
/// The private host has validated identities, goods and custody at import.
pub fn prepare_validated(
    state: &Agreements,
    cargo: &GoodsCargo,
    opinions: Option<&game_parallel_opinions::Opinions>,
    people: &[PersonSnapshot],
    busy: &[u64],
    tick: u64,
) -> Result<Option<Draft>, String> {
    if state.policies.is_empty() {
        return Ok(None);
    }
    let bulk = game_parallel_agreement_bulk::prepare(state, cargo, people, busy, tick, opinions);
    let mut occupied = busy.to_vec();
    if let Some(bulk) = &bulk {
        occupied.extend_from_slice(&bulk.actors);
    }
    if occupied.par_windows(2).any(|pair| pair[0] > pair[1]) {
        occupied.par_sort_unstable();
    }
    let mut policies: Vec<_> = state.policies.par_iter().collect();
    if policies
        .par_windows(2)
        .any(|pair| pair[0].person > pair[1].person)
    {
        policies.par_sort_unstable_by_key(|policy| policy.person);
    }
    let dense = policies.first().and_then(|first| {
        policies.last().and_then(|last| {
            (last.person.checked_sub(first.person) == Some((policies.len() - 1) as u64))
                .then_some(first.person)
        })
    });
    let policy = |person: u64| {
        dense
            .and_then(|first| person.checked_sub(first))
            .and_then(|n| usize::try_from(n).ok())
            .filter(|&i| i < policies.len())
            .or_else(|| {
                if dense.is_none() {
                    policies.binary_search_by_key(&person, |p| p.person).ok()
                } else {
                    None
                }
            })
            .map(|i| policies[i])
    };
    let trust = opinions
        .filter(|_| {
            policies.par_iter().any(|policy| {
                (policy.accept && policy.minimum_trust.is_some())
                    || (policy.honor && policy.minimum_honor_trust.is_some())
                    || policy.fear_honor_threshold.is_some()
                    || policy.obligation_honor_threshold.is_some()
                    || policy.accept_pressure_threshold.is_some()
            })
        })
        .map(|opinions| opinions.index());
    let mut candidates: Vec<_> = state
        .contracts
        .par_iter()
        .filter(|c| match c.status {
            Status::Active => {
                c.due >= tick
                    && policy(c.debtor).is_some_and(|p| {
                        let assessment = trust
                            .as_ref()
                            .map(|index| index.assessment(c.debtor, c.creditor));
                        let obligation = trust
                            .as_ref()
                            .map(|index| index.obligation(c.debtor, c.creditor));
                        p.honors_with_feelings(
                            assessment.map(|row| row.0),
                            assessment.map(|row| row.1),
                            obligation,
                        )
                    })
            }
            Status::Proposed => {
                c.due > tick
                    && policy(c.creditor).is_some_and(|p| {
                        let assessment = trust
                            .as_ref()
                            .map(|index| index.assessment(c.creditor, c.debtor));
                        let obligation = trust
                            .as_ref()
                            .map(|index| index.obligation(c.creditor, c.debtor));
                        p.accepts(
                            assessment.map(|row| row.0),
                            assessment.map(|row| row.1),
                            obligation,
                        )
                    })
            }
            _ => false,
        })
        .collect();
    if candidates.is_empty() {
        return Ok(bulk::combine(bulk, None));
    }
    if candidates.par_windows(2).any(|p| p[0].id > p[1].id) {
        candidates.par_sort_unstable_by_key(|c| c.id);
    }
    let mut existing: Vec<_> = cargo.holdings.par_iter().collect();
    if existing.par_windows(2).any(|p| p[0].person > p[1].person) {
        existing.par_sort_unstable_by_key(|h| h.person);
    }
    let dense = existing.first().and_then(|first| {
        existing.last().and_then(|last| {
            (last.person.checked_sub(first.person) == Some((existing.len() - 1) as u64))
                .then_some(first.person)
        })
    });
    let lookup = |id: u64| {
        dense
            .and_then(|first| id.checked_sub(first))
            .and_then(|n| usize::try_from(n).ok())
            .filter(|&i| i < existing.len())
            .or_else(|| {
                if dense.is_none() {
                    existing.binary_search_by_key(&id, |h| h.person).ok()
                } else {
                    None
                }
            })
            .map(|i| existing[i])
    };
    let busy = &occupied;
    let feasible: Vec<_> = candidates
        .par_iter()
        .map(|contract| {
            let Some(a) = person_index(people, contract.debtor).map(|i| people[i]) else {
                return false;
            };
            let Some(b) = person_index(people, contract.creditor).map(|i| people[i]) else {
                return false;
            };
            if a.2 == 0
                || b.2 == 0
                || a.1.is_none()
                || a.1 != b.1
                || busy.binary_search(&contract.debtor).is_ok()
                || busy.binary_search(&contract.creditor).is_ok()
            {
                return false;
            }
            if contract.status == Status::Proposed {
                return true;
            }
            let Some(quantity) = contract
                .quantity
                .checked_sub(contract.delivered)
                .filter(|&q| q > 0)
            else {
                return false;
            };
            let Some(from) = lookup(contract.debtor) else {
                return false;
            };
            if !from
                .items
                .iter()
                .any(|item| item.good == contract.good && item.quantity >= quantity)
            {
                return false;
            }
            let to = lookup(contract.creditor);
            let carried: u128 = to.map_or(0, |holding| {
                holding
                    .items
                    .iter()
                    .map(|item| u128::from(item.quantity))
                    .sum()
            });
            carried + u128::from(quantity) <= u128::from(cargo.capacity)
                && to
                    .and_then(|holding| {
                        holding.items.iter().find(|item| item.good == contract.good)
                    })
                    .is_none_or(|item| item.quantity.checked_add(quantity).is_some())
        })
        .collect();
    let (selected, chosen) = arbitration::select(&candidates, &feasible, people);
    if selected.is_empty() {
        return Ok(bulk::combine(bulk, None));
    }
    let dense = selected.first().and_then(|first| {
        selected.last().and_then(|last| {
            (last.id.checked_sub(first.id) == Some((selected.len() - 1) as u64)).then_some(first.id)
        })
    });
    let selected_index = |id: u64| {
        dense
            .and_then(|first| id.checked_sub(first))
            .and_then(|n| usize::try_from(n).ok())
            .filter(|&i| i < selected.len())
            .or_else(|| {
                if dense.is_none() {
                    selected
                        .binary_search_by_key(&id, |contract| contract.id)
                        .ok()
                } else {
                    None
                }
            })
    };
    let actors: Vec<_> = chosen.par_iter().map(|claim| claim.0).collect();
    // Chosen claims are already actor-ordered and each actor appears once.
    let holdings: Vec<_> = chosen
        .par_iter()
        .filter_map(|&(actor, id)| {
            let contract = selected[selected_index(id).expect("selected claim")];
            if contract.status == Status::Proposed {
                return None;
            }
            let quantity = contract.quantity - contract.delivered;
            if actor == contract.debtor {
                let mut from = lookup(actor).expect("assessed debtor holding").clone();
                let item = from
                    .items
                    .iter_mut()
                    .find(|item| item.good == contract.good)
                    .expect("assessed good");
                item.quantity -= quantity;
                from.items.retain(|item| item.quantity > 0);
                Some((actor, (!from.items.is_empty()).then_some(from)))
            } else {
                let mut to = lookup(actor).cloned().unwrap_or_else(|| GoodsHolding {
                    person: actor,
                    items: Default::default(),
                });
                if let Some(item) = to.items.iter_mut().find(|item| item.good == contract.good) {
                    item.quantity += quantity;
                } else {
                    to.items.push(Amount {
                        good: contract.good,
                        quantity,
                    });
                    to.items.sort_unstable_by_key(|item| item.good);
                }
                Some((actor, Some(to)))
            }
        })
        .collect();
    let updates: Vec<_> = selected
        .par_iter()
        .map(|contract| {
            let mut next = (*contract).clone();
            if next.status == Status::Proposed {
                next.status = Status::Active;
            } else {
                next.delivered = next.quantity;
                next.status = Status::Fulfilled;
            }
            next
        })
        .collect();
    let events = selected
        .par_iter()
        .map(|c| Event {
            tick,
            contract: c.id,
            actor: if c.status == Status::Proposed {
                c.creditor
            } else {
                c.debtor
            },
            kind: if c.status == Status::Proposed {
                EventKind::Accepted
            } else {
                EventKind::Fulfilled
            },
            quantity: if c.status == Status::Proposed {
                0
            } else {
                c.quantity - c.delivered
            },
        })
        .collect();
    Ok(bulk::combine(
        bulk,
        Some(Draft {
            actors,
            updates,
            events,
            holdings,
        }),
    ))
}
