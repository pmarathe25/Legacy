//! Checked debits of already-authorized quantities, staged with other owners.
use crate::{Inventory, inventory};
use game_core::{Error, InventoryId, Quantity};
use game_ecs::{Batch, StableId, Store};
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct PreparedConsumptions {
    rows: BTreeMap<InventoryId, (Inventory, Inventory)>,
}

/// Return a checked debit of one already-selected inventory row.
///
/// Selection, access, reservations, and the reason for consuming remain with
/// the requesting owner. Cargo validates the referenced row and conserves its
/// quantity without reading or mutating a Store.
pub fn consume_inventory_row(
    id: InventoryId,
    row: &Inventory,
    quantity: Quantity,
) -> Result<Inventory, Error> {
    row.validate()?;
    if row.id != id {
        return Err(Error::Unavailable);
    }
    let mut after = row.clone();
    after.quantity = Quantity(
        after
            .quantity
            .0
            .checked_sub(quantity.0)
            .ok_or(Error::Unavailable)?,
    );
    Ok(after)
}

impl PreparedConsumptions {
    pub fn validate(&self, store: &Store) -> Result<(), Error> {
        if self
            .rows
            .iter()
            .any(|(id, (before, _))| inventory(store, *id) != Some(before))
        {
            return Err(Error::Unavailable);
        }
        let mut batch = Batch::new();
        for (id, (_, after)) in &self.rows {
            batch.insert(StableId(id.0), after.clone());
        }
        store.validate_batch(&batch)
    }
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<(), Error> {
        self.validate(store)?;
        for (id, (_, after)) in self.rows {
            batch.insert(StableId(id.0), after);
        }
        Ok(())
    }
    pub fn stage_guarded_into(
        self,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<game_ecs::ReadGuard, Error> {
        self.validate(store)?;
        let mut reads = game_ecs::ReadSet::new();
        let mut admitted = game_ecs::ReadFootprint::new();
        for (id, (before, _)) in &self.rows {
            let id = StableId(id.0);
            reads.expect_row(id, Some(before.clone()));
            if batch.touches::<Inventory>(id) {
                return Err(Error::Unavailable);
            }
            admitted.protect::<Inventory>(id);
        }
        self.stage_into(store, batch)?;
        reads.bind(store, batch, &admitted)
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        let guard = self.stage_guarded_into(store, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)
    }
}
/// Access and the reason to consume belong to the requesting owner. This owner
/// checks only the actual referenced stocks and their conserved quantities.
pub fn prepare_consumptions(
    store: &Store,
    requests: &[(InventoryId, Quantity)],
) -> Result<PreparedConsumptions, Error> {
    let mut rows = BTreeMap::<InventoryId, (Inventory, Inventory)>::new();
    for (id, quantity) in requests {
        let (_, after) = match rows.entry(*id) {
            std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::btree_map::Entry::Vacant(entry) => {
                let before = inventory(store, *id).ok_or(Error::Unavailable)?;
                entry.insert((before.clone(), before.clone()))
            }
        };
        *after = consume_inventory_row(*id, after, *quantity)?;
    }
    Ok(PreparedConsumptions { rows })
}

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug)]
pub struct ConsumptionFacts {
    /// Material required by the receiving body/care operation.
    pub good: game_core::Good,
    pub accessible: bool,
    pub contact: bool,
    pub unreserved: Quantity,
}
/// Admission against the canonical stock and the common owner-supplied budget.
/// Checking availability never reserves or spends the portion.
pub fn portion_available(stock: &Inventory, quantity: Quantity, facts: ConsumptionFacts) -> bool {
    stock.id.0 > 0
        && quantity.0 > 0
        && stock.good == facts.good
        && facts.accessible
        && facts.contact
        && facts.unreserved >= quantity
        && stock.quantity >= quantity
        && stock.room().is_ok()
}
#[derive(Debug)]
pub struct PreparedPortionConsumption {
    consumption: PreparedConsumptions,
}
pub fn prepare_portion_consumption(
    store: &Store,
    actor: game_core::PersonId,
    source: InventoryId,
    quantity: Quantity,
    facts: ConsumptionFacts,
) -> Result<PreparedPortionConsumption, Error> {
    let stock = inventory(store, source).ok_or(Error::Unavailable)?;
    if actor.0 == 0 || stock.id != source || !portion_available(stock, quantity, facts) {
        return Err(Error::Unavailable);
    }
    let mut consumption = prepare_consumptions(store, &[(source, quantity)])?;
    consumption
        .rows
        .get_mut(&source)
        .expect("prepared consumption row")
        .1
        .last_granted = Some(actor);
    Ok(PreparedPortionConsumption { consumption })
}
pub fn prepare_food_consumption(
    store: &Store,
    actor: game_core::PersonId,
    source: InventoryId,
    quantity: Quantity,
    facts: ConsumptionFacts,
) -> Result<PreparedPortionConsumption, Error> {
    if facts.good != game_core::Good::Food {
        return Err(Error::Unavailable);
    }
    prepare_portion_consumption(store, actor, source, quantity, facts)
}
impl PreparedPortionConsumption {
    /// Bind admission without spending a delayed portion. Source stock must
    /// remain unchanged through the initial activity/interruption publication.
    pub fn guard_admission(
        &self,
        store: &Store,
        batch: &Batch,
    ) -> Result<game_ecs::ReadGuard, Error> {
        self.consumption.validate(store)?;
        let mut reads = game_ecs::ReadSet::new();
        for (id, (before, _)) in &self.consumption.rows {
            reads.expect_row(StableId(id.0), Some(before.clone()));
        }
        reads.bind(store, batch, &game_ecs::ReadFootprint::new())
    }
    pub fn stage_into(
        self,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<game_ecs::ReadGuard, Error> {
        self.consumption.stage_guarded_into(store, batch)
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        self.consumption.commit(store)
    }
}

/// A deadline loss clears stock and its spoil deadline while retaining every
/// other field. Preparation does not consume the scheduled event or publish.
#[derive(Clone, Debug)]
pub struct PreparedSpoilage {
    before: Inventory,
    after: Inventory,
}
pub fn prepare_spoilage(store: &Store, source: InventoryId) -> Result<PreparedSpoilage, Error> {
    let before = inventory(store, source)
        .cloned()
        .ok_or(Error::Unavailable)?;
    before.validate()?;
    if before.id != source {
        return Err(Error::Unavailable);
    }
    let mut after = before.clone();
    after.quantity = Quantity(0);
    after.spoils_at = None;
    Ok(PreparedSpoilage { before, after })
}
impl PreparedSpoilage {
    pub fn inventory(&self) -> InventoryId {
        self.before.id
    }
    pub fn quantity(&self) -> Quantity {
        self.before.quantity
    }
    pub fn closing(&self) -> &Inventory {
        &self.after
    }
    pub fn stage_into(
        self,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<game_ecs::ReadGuard, Error> {
        if inventory(store, self.before.id) != Some(&self.before) {
            return Err(Error::Unavailable);
        }
        let id = StableId(self.before.id.0);
        if batch.touches::<Inventory>(id) {
            return Err(Error::Unavailable);
        }
        let mut reads = game_ecs::ReadSet::new();
        reads.expect_row(id, Some(self.before));
        let mut admitted = game_ecs::ReadFootprint::new();
        admitted.protect::<Inventory>(id);
        batch.insert(id, self.after);
        reads.bind(store, batch, &admitted)
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        let guard = self.stage_into(store, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)
    }
}
