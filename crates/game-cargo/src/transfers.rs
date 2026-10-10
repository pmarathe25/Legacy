//! Closing cargo rules over scoped borrowed actual stocks. Opening spending
//! budgets are separate from current occupied receiving space. Effects commit
//! once to the same authoritative map; observations/receipts belong to callers.
use crate::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TransferIntent {
    pub actor: PersonId,
    pub source: InventoryId,
    pub destination: InventoryId,
    pub quantity: Quantity,
}
#[derive(Clone, Copy, Debug)]
pub struct ClosingFacts {
    pub alive: bool,
    pub health: u64,
    pub source_contact: bool,
    pub source_access: bool,
    pub receiver_contact: bool,
    pub receiver_access: bool,
    pub receiver_accepted: bool,
    pub record_permission: bool,
}
impl ClosingFacts {
    fn eligible(self) -> bool {
        self.alive
            && self.health > 0
            && self.source_contact
            && self.source_access
            && self.receiver_contact
            && (self.receiver_access || self.receiver_accepted)
            && self.record_permission
    }
}
#[derive(Clone, Debug)]
pub struct TransferRequest {
    pub intent: TransferIntent,
    pub facts: ClosingFacts,
}
impl LoadJob {
    /// Admission rights may have changed; elapsed time alone does not transfer
    /// anything. The current owner facts are evaluated by the common phase.
    pub fn closing_request(
        &self,
        now: SimTime,
        facts: ClosingFacts,
    ) -> Result<TransferRequest, Error> {
        if self.completes > now {
            return Err(Error::Unavailable);
        }
        Ok(TransferRequest {
            intent: TransferIntent {
                actor: self.actor,
                source: self.source,
                destination: self.destination,
                quantity: self.quantity,
            },
            facts,
        })
    }
}
#[derive(Debug)]
struct TransferEffect {
    intent: TransferIntent,
    granted: bool,
    source_after: Quantity,
    destination_after: Quantity,
}
#[derive(Debug)]
pub struct PreparedTransfers {
    decisions: Vec<(TransferIntent, bool)>,
    guards: BTreeMap<InventoryId, Inventory>,
    effects: Vec<TransferEffect>,
}
/// A guarded ordered phase. The coordinator retains its foreign epoch and may
/// publish each receipt between effects; it must not change the touched stocks.
/// Only this owner mutates balances, and each effect is consumed exactly once.
#[derive(Debug)]
pub struct ValidatedTransfers {
    remaining: std::vec::IntoIter<TransferEffect>,
}
impl ValidatedTransfers {
    pub fn apply_next(
        &mut self,
        stocks: &mut BTreeMap<InventoryId, Inventory>,
    ) -> Option<(TransferIntent, bool)> {
        let effect = self.remaining.next()?;
        if effect.granted {
            let source = stocks
                .get_mut(&effect.intent.source)
                .expect("phase source was guarded");
            source.quantity = effect.source_after;
            source.last_granted = Some(effect.intent.actor);
            stocks
                .get_mut(&effect.intent.destination)
                .expect("phase destination was guarded")
                .quantity = effect.destination_after;
        }
        Some((effect.intent, effect.granted))
    }
}
impl PreparedTransfers {
    pub fn decisions(&self) -> &[(TransferIntent, bool)] {
        &self.decisions
    }
    /// Guard every referenced actual stock before any mutation. The caller
    /// separately guards its foreign body/permission epoch through publication.
    pub fn begin(
        self,
        stocks: &BTreeMap<InventoryId, Inventory>,
    ) -> Result<ValidatedTransfers, Error> {
        if self
            .guards
            .iter()
            .any(|(id, stock)| stocks.get(id) != Some(stock))
        {
            return Err(Error::Unavailable);
        }
        Ok(ValidatedTransfers {
            remaining: self.effects.into_iter(),
        })
    }
    /// A phase without foreign receipt interleaving uses the same ordered owner
    /// effects. There is no separate arithmetic or alternate admission path.
    pub fn commit(
        self,
        stocks: &mut BTreeMap<InventoryId, Inventory>,
    ) -> Result<Vec<(TransferIntent, bool)>, Error> {
        let mut phase = self.begin(stocks)?;
        Ok(std::iter::from_fn(|| phase.apply_next(stocks)).collect())
    }
}
/// Only referenced stock rows are borrowed. `free_body_room` is the current
/// aggregate carrying room after all other closing effects; outgoing goods do
/// not replenish it. None means the fixture has no physical carrying model.
/// The reserve callback operates only on a temporary transport-capacity frame.
pub fn prepare_transfers(
    stocks: &BTreeMap<InventoryId, &Inventory>,
    opening: Option<&BTreeMap<InventoryId, u64>>,
    requests: Vec<TransferRequest>,
    free_body_room: Option<&BTreeMap<PersonId, u64>>,
    mut reserve_transport: impl FnMut(&TransferIntent) -> bool,
) -> Result<PreparedTransfers, Error> {
    let balances: BTreeMap<_, _> = stocks
        .iter()
        .map(|(id, stock)| {
            (
                *id,
                Balance {
                    stock: stock.quantity.0,
                    spendable: opening
                        .and_then(|rows| rows.get(id))
                        .copied()
                        .unwrap_or(stock.quantity.0)
                        .min(stock.quantity.0),
                    capacity: stock.capacity.0,
                    last_granted: stock.last_granted,
                },
            )
        })
        .collect();
    let carriers: BTreeMap<_, _> = stocks
        .iter()
        .filter_map(|(id, stock)| match stock.location {
            InventoryLocation::Carried(holder) if free_body_room.is_some() => Some((*id, holder)),
            _ => None,
        })
        .collect();
    let owned = requests
        .into_iter()
        .map(|r| {
            let eligible = r.intent.actor.0 > 0
                && r.intent.source.0 > 0
                && r.intent.destination.0 > 0
                && r.facts.eligible()
                && stocks
                    .get(&r.intent.source)
                    .zip(stocks.get(&r.intent.destination))
                    .is_some_and(|(from, to)| {
                        from.id == r.intent.source
                            && to.id == r.intent.destination
                            && from.good == to.good
                            && from.room().is_ok()
                            && to.room().is_ok()
                    });
            Request {
                actor: r.intent.actor,
                source: r.intent.source,
                destination: r.intent.destination,
                amount: r.intent.quantity.0,
                eligible,
            }
        })
        .collect();
    let mut effects = BTreeMap::<InventoryId, (Quantity, Option<PersonId>)>::new();
    let resolved = arbitrate_with_constraints(
        &balances,
        owned,
        &carriers,
        free_body_room.unwrap_or(&BTreeMap::new()),
        |r| {
            if carriers
                .get(&r.destination)
                .is_none_or(|holder| carriers.get(&r.source) == Some(holder))
            {
                return true;
            }
            reserve_transport(&TransferIntent {
                actor: r.actor,
                source: r.source,
                destination: r.destination,
                quantity: Quantity(r.amount),
            })
        },
    );
    let mut decisions = Vec::with_capacity(resolved.len());
    let mut ordered_effects = Vec::with_capacity(resolved.len());
    for (r, granted) in resolved {
        let intent = TransferIntent {
            actor: r.actor,
            source: r.source,
            destination: r.destination,
            quantity: Quantity(r.amount),
        };
        if granted {
            let source = effects.entry(r.source).or_insert((
                Quantity(balances[&r.source].stock),
                balances[&r.source].last_granted,
            ));
            source.0.0 = source.0.0.checked_sub(r.amount).ok_or(Error::Overflow)?;
            source.1 = Some(r.actor);
            let destination = effects.entry(r.destination).or_insert((
                Quantity(balances[&r.destination].stock),
                balances[&r.destination].last_granted,
            ));
            destination.0.0 = destination
                .0
                .0
                .checked_add(r.amount)
                .ok_or(Error::Overflow)?;
        }
        ordered_effects.push(TransferEffect {
            intent: intent.clone(),
            granted,
            source_after: effects
                .get(&r.source)
                .map_or(Quantity(0), |effect| effect.0),
            destination_after: effects
                .get(&r.destination)
                .map_or(Quantity(0), |effect| effect.0),
        });
        decisions.push((intent, granted));
    }
    if effects
        .iter()
        .any(|(id, (quantity, _))| quantity.0 > balances[id].capacity)
    {
        return Err(Error::Unavailable);
    }
    Ok(PreparedTransfers {
        decisions,
        guards: stocks
            .iter()
            .map(|(id, stock)| (*id, (*stock).clone()))
            .collect(),
        effects: ordered_effects,
    })
}
#[cfg(test)]
mod tests;
