//! Physical control of a declared inventory is distinct from asserted claims.
use game_core::{Error, InventoryId, PersonId};
use game_ecs::{Batch, Schema, StableId, Store};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryCustody {
    pub custodian: PersonId,
}
impl InventoryCustody {
    pub fn validate(&self) -> Result<(), Error> {
        if self.custodian.0 == 0 {
            return Err(Error::InvalidConfiguration(
                "invalid inventory custody".into(),
            ));
        }
        Ok(())
    }
}
pub fn register_custody(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<InventoryCustody>("property/inventory_custody", InventoryCustody::validate)
}
pub fn inventory_custodian(store: &Store, id: InventoryId) -> Option<PersonId> {
    store
        .get::<InventoryCustody>(StableId(id.0))
        .map(|row| row.custodian)
}
pub fn custody_rows(store: &Store) -> impl Iterator<Item = (InventoryId, &InventoryCustody)> {
    store
        .iter::<InventoryCustody>()
        .map(|(id, row)| (InventoryId(id.0), row))
}
/// Startup declaration; play changes are prepared by the owner below.
pub fn declare_custody(
    store: &mut Store,
    id: InventoryId,
    custodian: PersonId,
) -> Result<(), Error> {
    store.insert(StableId(id.0), InventoryCustody { custodian })
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedCustody {
    id: InventoryId,
    before: Option<InventoryCustody>,
    after: InventoryCustody,
}
pub fn prepare_custody(
    store: &Store,
    id: InventoryId,
    custodian: PersonId,
) -> Result<PreparedCustody, Error> {
    if !store.contains(StableId(id.0)) {
        return Err(Error::Unavailable);
    }
    let after = InventoryCustody { custodian };
    after.validate()?;
    Ok(PreparedCustody {
        id,
        before: store.get(StableId(id.0)).cloned(),
        after,
    })
}
impl PreparedCustody {
    pub fn check(&self, store: &Store) -> Result<(), Error> {
        if store.get::<InventoryCustody>(StableId(self.id.0)) != self.before.as_ref() {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn stage(&self, batch: &mut Batch) {
        batch.insert(StableId(self.id.0), self.after.clone());
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        self.check(store)?;
        let mut batch = Batch::new();
        self.stage(&mut batch);
        store.apply_batch(batch)
    }
}
pub struct CustodyContext<'a> {
    pub inventories: &'a std::collections::BTreeSet<InventoryId>,
    pub people: &'a std::collections::BTreeSet<PersonId>,
}
pub fn validate_custody(store: &Store, context: CustodyContext<'_>) -> Result<(), Error> {
    if custody_rows(store).any(|(id, row)| {
        !context.inventories.contains(&id) || !context.people.contains(&row.custodian)
    }) {
        return Err(Error::InvalidSave(
            "invalid inventory custody context".into(),
        ));
    }
    Ok(())
}
