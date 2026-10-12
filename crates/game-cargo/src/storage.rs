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
    pub fn receiving(&self) -> &Inventory {
        &self.receiving
    }
    pub fn stage_into(self, store: &Store, batch: &mut game_ecs::Batch) -> Result<(), Error> {
        if store.contains(StableId(self.receiving.id.0))
            || self
                .source
                .as_ref()
                .is_some_and(|(before, _)| inventory(store, before.id) != Some(before))
        {
            return Err(Error::Unavailable);
        }
        if let Some((_, after)) = self.source {
            batch.insert(StableId(after.id.0), after);
        }
        batch.insert(StableId(self.receiving.id.0), self.receiving);
        Ok(())
    }
}
