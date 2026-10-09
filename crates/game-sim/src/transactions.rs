//! Pure arbitration over a consistent physical ledger. Each output is committed
//! once; newly incoming stock cannot be spent again during the same phase.
use game_api::{InventoryId, PersonId};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub struct Balance {
    pub stock: u64,
    pub capacity: u64,
    pub last_granted: Option<PersonId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Request {
    pub actor: PersonId,
    pub source: InventoryId,
    pub destination: InventoryId,
    pub amount: u64,
    pub eligible: bool,
}

pub fn arbitrate(
    balances: &BTreeMap<InventoryId, Balance>,
    requests: Vec<Request>,
) -> Vec<(Request, bool)> {
    arbitrate_with_capacity(balances, requests, &BTreeMap::new(), &BTreeMap::new())
}

pub fn arbitrate_with_capacity(
    balances: &BTreeMap<InventoryId, Balance>,
    mut requests: Vec<Request>,
    carriers: &BTreeMap<InventoryId, PersonId>,
    free_capacity: &BTreeMap<PersonId, u64>,
) -> Vec<(Request, bool)> {
    requests.sort_by_key(|r| {
        let cursor = balances.get(&r.source).and_then(|b| b.last_granted);
        (r.source, cursor.is_some_and(|last| r.actor <= last), *r)
    });
    let mut outgoing = BTreeMap::<InventoryId, u64>::new();
    let mut incoming = BTreeMap::<InventoryId, u64>::new();
    let mut carried_incoming = BTreeMap::<PersonId, u64>::new();
    requests
        .into_iter()
        .map(|r| {
            let granted = if let (Some(source), Some(destination)) =
                (balances.get(&r.source), balances.get(&r.destination))
            {
                let capacity = carriers.get(&r.destination).is_none_or(|actor| {
                    carriers.get(&r.source) == Some(actor)
                        || free_capacity
                            .get(actor)
                            .and_then(|v| {
                                v.checked_sub(carried_incoming.get(actor).copied().unwrap_or(0))
                            })
                            .is_some_and(|v| r.amount <= v)
                });
                r.eligible
                    && capacity
                    && r.amount > 0
                    && r.source != r.destination
                    && source
                        .stock
                        .checked_sub(outgoing.get(&r.source).copied().unwrap_or(0))
                        .is_some_and(|v| r.amount <= v)
                    && destination
                        .capacity
                        .checked_sub(destination.stock)
                        .and_then(|v| {
                            v.checked_sub(incoming.get(&r.destination).copied().unwrap_or(0))
                        })
                        .is_some_and(|v| r.amount <= v)
            } else {
                false
            };
            if granted {
                *outgoing.entry(r.source).or_default() += r.amount;
                *incoming.entry(r.destination).or_default() += r.amount;
                if let Some(actor) = carriers.get(&r.destination)
                    && carriers.get(&r.source) != Some(actor)
                {
                    *carried_incoming.entry(*actor).or_default() += r.amount;
                }
            }
            (r, granted)
        })
        .collect()
}
