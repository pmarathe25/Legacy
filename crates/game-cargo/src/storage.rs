//! Conserved inventories live on shared stock entities.
use crate::{Inventory, InventoryLocation};
use game_core::{Error, InventoryId};
use game_ecs::{Schema, StableId, Store};

impl Inventory {
    pub fn validate(&self) -> Result<(), Error> {
        let location = match self.location {
            InventoryLocation::Area(id) => id.0,
            InventoryLocation::Carried(id) => id.0,
        };
        if self.id.0 == 0
            || location == 0
            || self.quantity > self.capacity
            || self.access.iter().any(|actor| actor.0 == 0)
            || self.last_granted.is_some_and(|actor| actor.0 == 0)
        {
            return Err(Error::InvalidSave("invalid inventory component".into()));
        }
        Ok(())
    }
}
pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Inventory>("cargo/inventory", Inventory::validate)
}
pub fn inventory(store: &Store, id: InventoryId) -> Option<&Inventory> {
    store.get(StableId(id.0))
}
pub fn inventory_mut(store: &mut Store, id: InventoryId) -> Option<&mut Inventory> {
    store.get_mut(StableId(id.0))
}
pub fn inventories(store: &Store) -> impl Iterator<Item = (InventoryId, &Inventory)> {
    store
        .iter::<Inventory>()
        .map(|(id, row)| (InventoryId(id.0), row))
}
pub fn insert_inventory(store: &mut Store, row: Inventory) -> Result<(), Error> {
    row.validate()?;
    store.insert_or_spawn(StableId(row.id.0), row)
}
pub fn remove_inventory(store: &mut Store, id: InventoryId) -> Result<Option<Inventory>, Error> {
    if !store.contains(StableId(id.0)) {
        return Ok(None);
    }
    store.remove(StableId(id.0))
}

/// Validate this owner's rows and their stable identity. Foreign entity
/// references and clock constraints belong to the composed save validator.
pub fn validate_storage(store: &Store) -> Result<(), Error> {
    for (id, row) in inventories(store) {
        row.validate()?;
        if row.id != id {
            return Err(Error::InvalidSave(
                "inventory component identity differs from entity".into(),
            ));
        }
    }
    Ok(())
}

/// Creation and physical splits retain Cargo's canonical balance and identity.
#[derive(Clone, Debug)]
pub struct PreparedInventoryCreation {
    source: Option<(Inventory, Inventory)>,
    receiving: Inventory,
}
pub fn prepare_inventory_creation(
    store: &Store,
    receiving: Inventory,
) -> Result<PreparedInventoryCreation, Error> {
    receiving.validate()?;
    if store.contains(StableId(receiving.id.0)) {
        return Err(Error::Unavailable);
    }
    Ok(PreparedInventoryCreation {
        source: None,
        receiving,
    })
}
/// Split into an empty, newly allocated compartment. The caller supplies
/// factual authorization; Cargo owns quantity, good, identity, and capacity.
/// A split leaves the original compartment's last-granted history unchanged.
pub fn prepare_inventory_split(
    store: &Store,
    source: InventoryId,
    mut receiving: Inventory,
    quantity: game_core::Quantity,
) -> Result<PreparedInventoryCreation, Error> {
    receiving.validate()?;
    let before = inventory(store, source).ok_or(Error::Unavailable)?;
    before.validate()?;
    if quantity.0 == 0
        || receiving.quantity.0 != 0
        || receiving.good != before.good
        || store.contains(StableId(receiving.id.0))
        || receiving.room()? < quantity
    {
        return Err(Error::Unavailable);
    }
    let mut after = before.clone();
    after.quantity.0 = after
        .quantity
        .0
        .checked_sub(quantity.0)
        .ok_or(Error::Unavailable)?;
    receiving.quantity = quantity;
    Ok(PreparedInventoryCreation {
        source: Some((before.clone(), after)),
        receiving,
    })
}
impl PreparedInventoryCreation {
    pub fn stage_guarded_into(
        self,
        store: &Store,
        batch: &mut game_ecs::Batch,
    ) -> Result<game_ecs::ReadGuard, Error> {
        self.stage_guarded(store, batch, false)
    }
    fn stage_guarded(
        self,
        store: &Store,
        batch: &mut game_ecs::Batch,
        exclusive: bool,
    ) -> Result<game_ecs::ReadGuard, Error> {
        let mut reads = game_ecs::ReadSet::new();
        let mut admitted = game_ecs::ReadFootprint::new();
        let receiving = StableId(self.receiving.id.0);
        if exclusive {
            reads.exclusive_entity(store, receiving)?;
        } else {
            reads.entity(store, receiving);
        }
        reads.expect_row::<Inventory>(receiving, None);
        admitted.protect::<Inventory>(receiving);
        if let Some((before, _)) = &self.source {
            reads.expect_row(StableId(before.id.0), Some(before.clone()));
            admitted.protect::<Inventory>(StableId(before.id.0));
        }
        self.stage_into(store, batch)?;
        reads.bind(store, batch, &admitted)
    }
    pub fn receiving(&self) -> &Inventory {
        &self.receiving
    }
    pub fn stage_into(
        self,
        store: &Store,
        batch: &mut game_ecs::Batch,
    ) -> Result<InventoryCreationGuard, Error> {
        if store.contains(StableId(self.receiving.id.0))
            || batch.touches::<Inventory>(StableId(self.receiving.id.0))
            || self
                .source
                .as_ref()
                .is_some_and(|(before, _)| inventory(store, before.id) != Some(before))
            || self
                .source
                .as_ref()
                .is_some_and(|(before, _)| batch.touches::<Inventory>(StableId(before.id.0)))
        {
            return Err(Error::Unavailable);
        }
        let source = self.source.as_ref().map(|(_, after)| after.id);
        let receiving = self.receiving.id;
        if let Some((_, after)) = self.source {
            batch.insert(StableId(after.id.0), after);
        }
        batch.insert(StableId(self.receiving.id.0), self.receiving);
        Ok(InventoryCreationGuard {
            source,
            receiving,
            batch: batch.key(),
        })
    }
}

pub struct InventoryCreationGuard {
    source: Option<InventoryId>,
    receiving: InventoryId,
    batch: game_ecs::BatchKey,
}
impl InventoryCreationGuard {
    pub fn validate_batch(&self, batch: &game_ecs::Batch) -> Result<(), Error> {
        if self.batch != batch.key()
            || [Some(self.receiving), self.source]
                .into_iter()
                .flatten()
                .any(|id| {
                    batch
                        .edited_ids::<Inventory>()
                        .filter(|edited| *edited == StableId(id.0))
                        .count()
                        != 1
                })
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
}

/// Owner construction for a forced physical split. The coercion owner supplies
/// placement and access; Cargo validates material, balances and the fresh ID.
/// Applied material starts without grant or spoil scheduling metadata.
#[derive(Clone, Debug)]
pub struct PreparedForcedSplit {
    creation: PreparedInventoryCreation,
}
pub fn prepare_forced_split(
    store: &Store,
    source: InventoryId,
    quantity: game_core::Quantity,
    new_id: InventoryId,
    location: InventoryLocation,
    access: std::collections::BTreeSet<game_core::PersonId>,
) -> Result<PreparedForcedSplit, Error> {
    let source_row = inventory(store, source).ok_or(Error::Unavailable)?;
    let receiving = Inventory {
        id: new_id,
        location,
        access,
        quantity: game_core::Quantity(0),
        capacity: quantity,
        last_granted: None,
        good: source_row.good,
        spoils_at: None,
    };
    Ok(PreparedForcedSplit {
        creation: prepare_inventory_split(store, source, receiving, quantity)?,
    })
}
impl PreparedForcedSplit {
    pub fn receiving(&self) -> &Inventory {
        self.creation.receiving()
    }
    pub fn stage_into(
        self,
        store: &Store,
        batch: &mut game_ecs::Batch,
    ) -> Result<game_ecs::ReadGuard, Error> {
        self.creation.stage_guarded(store, batch, true)
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        let mut batch = game_ecs::Batch::new();
        let guard = self.stage_into(store, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)
    }
}

/// Owner-authored empty carried food compartment assigned at birth.
#[derive(Clone, Debug)]
pub struct PreparedNewbornInventory {
    creation: PreparedInventoryCreation,
}
pub fn prepare_newborn_inventory(
    store: &Store,
    id: InventoryId,
    child: game_core::PersonId,
) -> Result<PreparedNewbornInventory, Error> {
    let row = Inventory {
        id,
        location: InventoryLocation::Carried(child),
        quantity: game_core::Quantity(0),
        capacity: game_core::Quantity(10_000),
        access: std::collections::BTreeSet::from([child]),
        last_granted: None,
        good: game_core::Good::Food,
        spoils_at: None,
    };
    Ok(PreparedNewbornInventory {
        creation: prepare_inventory_creation(store, row)?,
    })
}
impl PreparedNewbornInventory {
    pub fn receiving(&self) -> &Inventory {
        self.creation.receiving()
    }
    pub fn stage_into(
        self,
        store: &Store,
        batch: &mut game_ecs::Batch,
    ) -> Result<game_ecs::ReadGuard, Error> {
        self.creation.stage_guarded(store, batch, false)
    }
}
