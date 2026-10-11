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
