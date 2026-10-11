//! Checked debits of already-authorized quantities, staged with other owners.
use crate::{Inventory, inventory};
use game_core::{Error, InventoryId, Quantity};
use game_ecs::{Batch, StableId, Store};
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct PreparedConsumptions {
    rows: BTreeMap<InventoryId, (Inventory, Inventory)>,
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
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        self.stage_into(store, &mut batch)?;
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
                before.validate()?;
                if before.id != *id {
                    return Err(Error::Unavailable);
                }
                entry.insert((before.clone(), before.clone()))
            }
        };
        after.quantity = Quantity(
            after
                .quantity
                .0
                .checked_sub(quantity.0)
                .ok_or(Error::Unavailable)?,
        );
    }
    Ok(PreparedConsumptions { rows })
}

#[cfg(test)]
mod tests;
