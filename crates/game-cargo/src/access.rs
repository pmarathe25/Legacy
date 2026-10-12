//! Owner-prepared access edits preserve physical stock and dated grant metadata.
use crate::*;
use game_ecs::{Batch, StableId, Store};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedAccess {
    before: Inventory,
    after: Inventory,
}
pub fn prepare_access(
    store: &Store,
    id: InventoryId,
    grantee: PersonId,
    allowed: bool,
) -> Result<PreparedAccess, Error> {
    if grantee.0 == 0 {
        return Err(Error::Unavailable);
    }
    let before = inventory(store, id).cloned().ok_or(Error::Unavailable)?;
    let mut after = before.clone();
    if allowed {
        after.access.insert(grantee);
    } else {
        after.access.remove(&grantee);
    }
    Ok(PreparedAccess { before, after })
}
impl PreparedAccess {
    pub fn check(&self, store: &Store) -> Result<(), Error> {
        if inventory(store, self.before.id) != Some(&self.before) {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn stage(&self, batch: &mut Batch) {
        batch.insert(StableId(self.after.id.0), self.after.clone());
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        self.check(store)?;
        let mut batch = Batch::new();
        self.stage(&mut batch);
        store.apply_batch(batch)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedAccessChanges {
    before: std::collections::BTreeMap<InventoryId, Inventory>,
    after: std::collections::BTreeMap<InventoryId, Inventory>,
}
pub fn prepare_access_changes(
    store: &Store,
    changes: &[(InventoryId, PersonId, bool)],
) -> Result<PreparedAccessChanges, Error> {
    let mut before = std::collections::BTreeMap::new();
    let mut after: std::collections::BTreeMap<InventoryId, Inventory> =
        std::collections::BTreeMap::new();
    for (id, grantee, allowed) in changes {
        if grantee.0 == 0 {
            return Err(Error::Unavailable);
        }
        let original = inventory(store, *id).ok_or(Error::Unavailable)?;
        before.entry(*id).or_insert_with(|| original.clone());
        let row = after.entry(*id).or_insert_with(|| original.clone());
        if *allowed {
            row.access.insert(*grantee);
        } else {
            row.access.remove(grantee);
        }
    }
    Ok(PreparedAccessChanges { before, after })
}
impl PreparedAccessChanges {
    pub fn check(&self, store: &Store) -> Result<(), Error> {
        if self
            .before
            .iter()
            .any(|(id, row)| inventory(store, *id) != Some(row))
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn stage(&self, batch: &mut Batch) {
        for (id, row) in &self.after {
            batch.insert(StableId(id.0), row.clone());
        }
    }
}
