//! Closing cargo rules over scoped borrowed actual stocks. Opening spending
//! budgets are separate from current occupied receiving space. Effects commit
//! once to the same authoritative component store; observations/receipts belong to callers.
use crate::*;
use game_ecs::{Batch, StableId, Store};
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
    rows: Option<([Inventory; 2], [Inventory; 2])>,
}
impl TransferEffect {
    fn batch(&self) -> Batch {
        let mut batch = Batch::new();
        if let Some((_, after)) = &self.rows {
            for row in after {
                batch.insert(StableId(row.id.0), row.clone());
            }
        }
        batch
    }
    fn validate_opening(&self, store: &Store) -> Result<(), Error> {
        if self.rows.as_ref().is_some_and(|(before, _)| {
            before
                .iter()
                .any(|row| inventory(store, row.id) != Some(row))
        }) {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
}
#[derive(Debug)]
pub struct PreparedTransfers {
    decisions: Vec<(TransferIntent, bool)>,
    guards: BTreeMap<InventoryId, Inventory>,
    effects: Vec<TransferEffect>,
}
/// Guarded ordered effects. Foreign receipts may update other components, but
/// must not change the phase's touched Inventory rows. Every receipt follows
/// its own consumed stock effect; later effects verify their expected prefix.
#[derive(Debug)]
pub struct ValidatedTransfers {
    remaining: std::vec::IntoIter<TransferEffect>,
}
impl ValidatedTransfers {
    pub fn stage_next_guarded_into(
        &mut self,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<Option<(TransferIntent, bool, game_ecs::ReadGuard)>, Error> {
        let Some(effect) = self.remaining.as_slice().first() else {
            return Ok(None);
        };
        effect.validate_opening(store)?;
        let mut reads = game_ecs::ReadSet::new();
        let mut admitted = game_ecs::ReadFootprint::new();
        if let Some((before, after)) = &effect.rows {
            for row in before {
                let id = StableId(row.id.0);
                reads.expect_row(id, Some(row.clone()));
                if batch.touches::<Inventory>(id) {
                    return Err(Error::Unavailable);
                }
            }
            if effect.granted {
                for row in after {
                    admitted.protect::<Inventory>(StableId(row.id.0));
                }
            }
        }
        let result = self
            .stage_next_into(store, batch)?
            .expect("checked phase effect");
        let guard = reads.bind(store, batch, &admitted)?;
        Ok(Some((result.0, result.1, guard)))
    }
    /// Stage one ordered physical transfer for composition with a receiving
    /// owner's receipt. Publishing the caller's combined batch precedes the
    /// next call; a rejected batch abandons this transient phase.
    pub fn stage_next_into(
        &mut self,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<Option<(TransferIntent, bool)>, Error> {
        let Some(effect) = self.remaining.as_slice().first() else {
            return Ok(None);
        };
        effect.validate_opening(store)?;
        if effect.granted
            && let Some((_, after)) = &effect.rows
        {
            for row in after {
                batch.insert(StableId(row.id.0), row.clone());
            }
        }
        let effect = self.remaining.next().expect("checked phase effect");
        Ok(Some((effect.intent, effect.granted)))
    }
    pub fn apply_next(
        &mut self,
        store: &mut Store,
    ) -> Result<Option<(TransferIntent, bool)>, Error> {
        let Some(effect) = self.remaining.as_slice().first() else {
            return Ok(None);
        };
        effect.validate_opening(store)?;
        if effect.granted {
            store.apply_batch(effect.batch())?;
        }
        let effect = self.remaining.next().expect("checked phase effect");
        Ok(Some((effect.intent, effect.granted)))
    }
}
impl PreparedTransfers {
    /// Consume the prepared stable sequence as exact provisional row effects.
    /// Native owner journals apply these rows without reconstructing a Store.
    pub fn into_row_effects(
        self,
    ) -> Vec<(TransferIntent, Option<([Inventory; 2], [Inventory; 2])>)> {
        self.effects
            .into_iter()
            .map(|effect| (effect.intent, effect.rows))
            .collect()
    }
    pub fn decisions(&self) -> &[(TransferIntent, bool)] {
        &self.decisions
    }
    /// Final canonical rows, after Cargo's stable arbitration and conservation.
    pub fn closing_stocks(&self) -> BTreeMap<InventoryId, &Inventory> {
        let mut rows = BTreeMap::new();
        for effect in &self.effects {
            if let Some((_, after)) = &effect.rows {
                for row in after {
                    rows.insert(row.id, row);
                }
            }
        }
        rows
    }
    pub fn stage_guarded_into(
        self,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<(Vec<(TransferIntent, bool)>, game_ecs::ReadGuard), Error> {
        self.validate(store)?;
        let mut reads = game_ecs::ReadSet::new();
        let mut admitted = game_ecs::ReadFootprint::new();
        for (id, row) in &self.guards {
            let id = StableId(id.0);
            reads.expect_row(id, Some(row.clone()));
            if batch.touches::<Inventory>(id) {
                return Err(Error::Unavailable);
            }
        }
        for id in self.closing_stocks().keys() {
            admitted.protect::<Inventory>(StableId(id.0));
        }
        let decisions = self.stage_into(store, batch)?;
        let guard = reads.bind(store, batch, &admitted)?;
        Ok((decisions, guard))
    }
    fn validate(&self, store: &Store) -> Result<(), Error> {
        if self
            .guards
            .iter()
            .any(|(id, row)| inventory(store, *id) != Some(row))
        {
            return Err(Error::Unavailable);
        }
        // Validate every intermediate prefix, not only the final balances.
        // This invokes the actual schema's callbacks before the first debit.
        for effect in &self.effects {
            store.validate_batch(&effect.batch())?;
        }
        Ok(())
    }
    pub fn begin(self, store: &Store) -> Result<ValidatedTransfers, Error> {
        self.validate(store)?;
        Ok(ValidatedTransfers {
            remaining: self.effects.into_iter(),
        })
    }
    /// Append final stock rows to a caller-owned combined batch after every
    /// opening and intermediate-prefix guard. No row is published by staging.
    pub fn stage_into(
        self,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<Vec<(TransferIntent, bool)>, Error> {
        self.validate(store)?;
        let mut final_rows = BTreeMap::new();
        for effect in &self.effects {
            if let Some((_, after)) = &effect.rows {
                for row in after {
                    final_rows.insert(row.id, row.clone());
                }
            }
        }
        for (id, row) in final_rows {
            batch.insert(StableId(id.0), row);
        }
        Ok(self.decisions)
    }
    /// Without receipt interleaving, publish all final rows atomically.
    pub fn commit(self, store: &mut Store) -> Result<Vec<(TransferIntent, bool)>, Error> {
        let mut batch = Batch::new();
        let (decisions, guard) = self.stage_guarded_into(store, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)?;
        Ok(decisions)
    }
}
/// Only referenced stock rows are borrowed. `free_body_room` is the current
/// aggregate carrying room after all other closing effects; outgoing goods do
/// not replenish it. None means the fixture has no physical carrying model.
/// The reserve callback operates only on a temporary transport-capacity frame.
pub fn prepare_transfers(
    store: &Store,
    opening: Option<&BTreeMap<InventoryId, u64>>,
    requests: Vec<TransferRequest>,
    free_body_room: Option<&BTreeMap<PersonId, u64>>,
    reserve_transport: impl FnMut(&TransferIntent) -> bool,
) -> Result<PreparedTransfers, Error> {
    let stocks: BTreeMap<_, _> = requests
        .iter()
        .flat_map(|request| [request.intent.source, request.intent.destination])
        .filter_map(|id| inventory(store, id).map(|row| (id, row)))
        .collect();
    prepare_transfers_rows(
        &stocks,
        opening,
        requests,
        free_body_room,
        reserve_transport,
    )
}
/// Native transactions supply only the exact inventory rows they already read.
/// The same stable arbitration and opening budget govern legacy and native callers.
pub fn prepare_transfers_rows(
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
    // A sparse planned prefix, never a second authoritative ledger.
    let mut prefix: BTreeMap<_, Inventory> = stocks
        .iter()
        .map(|(id, row)| (*id, (*row).clone()))
        .collect();
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
        let rows = if granted {
            let before = [prefix[&r.source].clone(), prefix[&r.destination].clone()];
            let mut after = before.clone();
            after[0].quantity = effects[&r.source].0;
            after[0].last_granted = Some(r.actor);
            after[1].quantity = effects[&r.destination].0;
            prefix.insert(r.source, after[0].clone());
            prefix.insert(r.destination, after[1].clone());
            Some((before, after))
        } else {
            None
        };
        ordered_effects.push(TransferEffect {
            intent: intent.clone(),
            granted,
            rows,
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
