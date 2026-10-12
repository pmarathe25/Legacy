//! Whole record identity moves with physical cargo through an owner-prepared patch.
use crate::*;
use game_core::*;
use game_ecs::{Batch, StableId, Store};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedRecordTransfer {
    source: InventoryId,
    source_rows: Vec<RecordId>,
    patches: Vec<(Record, Record)>,
}
fn source_rows(store: &Store, source: InventoryId) -> Vec<RecordId> {
    if game_cargo::inventory(store, source).is_none_or(|stock| stock.good != Good::Record) {
        return Vec::new();
    }
    records(store)
        .filter(|(_, row)| row.intact && row.inventory == source)
        .map(|(id, _)| id)
        .collect()
}
pub fn record_quantity_increment(store: &Store, source: InventoryId) -> Quantity {
    Quantity(
        if game_cargo::inventory(store, source).is_some_and(|stock| stock.good == Good::Record) {
            1000
        } else {
            1
        },
    )
}
pub fn record_transfer_allowed(store: &Store, source: InventoryId, quantity: Quantity) -> bool {
    let Some(stock) = game_cargo::inventory(store, source) else {
        return false;
    };
    stock.good != Good::Record
        || quantity.0 > 0
            && quantity.0.is_multiple_of(1000)
            && source_rows(store, source).len() as u128 >= u128::from(quantity.0 / 1000)
}
impl Runtime {
    pub fn record_quantity_increment(&self, store: &Store, source: InventoryId) -> Quantity {
        record_quantity_increment(store, source)
    }
    pub fn record_transfer_allowed(
        &self,
        store: &Store,
        source: InventoryId,
        quantity: Quantity,
    ) -> bool {
        record_transfer_allowed(store, source, quantity)
    }
    pub fn prepare_record_transfer(
        &self,
        store: &Store,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
        selected: Option<RecordId>,
    ) -> Result<PreparedRecordTransfer, Error> {
        let rows = source_rows(store, source);
        let Some(stock) = game_cargo::inventory(store, source) else {
            return Err(Error::Unavailable);
        };
        if stock.good != Good::Record {
            return Ok(PreparedRecordTransfer {
                source,
                source_rows: rows,
                patches: Vec::new(),
            });
        }
        if quantity.0 == 0
            || !quantity.0.is_multiple_of(1000)
            || (rows.len() as u128) < u128::from(quantity.0 / 1000)
            || destination.0 == 0
        {
            return Err(Error::Unavailable);
        }
        let selected_rows = if let Some(id) = selected {
            if quantity != Quantity(1000) || !rows.contains(&id) {
                return Err(Error::Unavailable);
            }
            vec![id]
        } else {
            rows.iter()
                .copied()
                .take((quantity.0 / 1000) as usize)
                .collect()
        };
        let patches = selected_rows
            .into_iter()
            .map(|id| {
                let before = record(store, id).unwrap().clone();
                let mut after = before.clone();
                after.inventory = destination;
                (before, after)
            })
            .collect();
        Ok(PreparedRecordTransfer {
            source,
            source_rows: rows,
            patches,
        })
    }
    /// A receiver may have staged its record effects with Cargo already. The
    /// physical source balance then has no outstanding record movement to apply.
    pub fn prepare_record_receipt(
        &self,
        store: &Store,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    ) -> Result<PreparedRecordTransfer, Error> {
        let stock = game_cargo::inventory(store, source).ok_or(Error::Unavailable)?;
        let rows = source_rows(store, source);
        if stock.good == Good::Record && rows.len() as u128 * 1000 == u128::from(stock.quantity.0) {
            return Ok(PreparedRecordTransfer {
                source,
                source_rows: rows,
                patches: Vec::new(),
            });
        }
        self.prepare_record_transfer(store, source, destination, quantity, None)
    }
}
impl PreparedRecordTransfer {
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<(), Error> {
        if source_rows(store, self.source) != self.source_rows
            || self
                .patches
                .iter()
                .any(|(before, _)| record(store, before.id) != Some(before))
        {
            return Err(Error::Unavailable);
        }
        for (_, after) in self.patches {
            batch.insert(StableId(after.id.0), after);
        }
        Ok(())
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        self.stage_into(store, &mut batch)?;
        store.apply_batch(batch)
    }
}

#[cfg(test)]
mod tests;
