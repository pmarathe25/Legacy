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
    pub fn stage_into(
        self,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<game_ecs::ReadGuard, Error> {
        self.check(store)?;
        let mut reads = game_ecs::ReadSet::new();
        let mut admitted = game_ecs::ReadFootprint::new();
        for (id, row) in &self.before {
            let id = StableId(id.0);
            reads.expect_row(id, Some(row.clone()));
            if batch.touches::<Inventory>(id) {
                return Err(Error::Unavailable);
            }
            admitted.protect::<Inventory>(id);
        }
        self.stage(batch);
        reads.bind(store, batch, &admitted)
    }
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

/// Independent personal and institutional grants both authorize stock access.
pub fn effective_access(personal: bool, institutional: bool) -> bool {
    personal || institutional
}
/// Receipt authority belongs to the accepting obligation owners. Cargo joins
/// their factual acceptance without manufacturing a general access grant.
pub fn receiver_authorization(agreement_accepted: bool, guardian_accepted: bool) -> bool {
    agreement_accepted || guardian_accepted
}

#[cfg(test)]
mod authorization_tests {
    use super::*;
    #[test]
    fn access_and_accepted_receipt_have_independent_owner_grants() {
        for (first, second, expected) in [
            (false, false, false),
            (true, false, true),
            (false, true, true),
            (true, true, true),
        ] {
            assert_eq!(effective_access(first, second), expected);
            assert_eq!(receiver_authorization(first, second), expected);
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MealSourceFacts {
    pub inventory: InventoryId,
    pub good: Good,
    pub quantity: Quantity,
    pub accessible: bool,
}
/// Public meal candidates use current local stock facts in supplied scene order.
/// Closing access and the shared consumption budget are admitted separately.
pub fn meal_sources(
    stocks: impl IntoIterator<Item = MealSourceFacts>,
    meal_size: Quantity,
    hunger_exempt: bool,
) -> Vec<InventoryId> {
    if hunger_exempt {
        return Vec::new();
    }
    stocks
        .into_iter()
        .filter(|stock| stock.accessible && stock.good == Good::Food && stock.quantity >= meal_size)
        .map(|stock| stock.inventory)
        .collect()
}

#[cfg(test)]
mod meal_tests {
    use super::*;
    #[test]
    fn meal_menu_uses_access_material_portion_threshold_and_scene_order() {
        let stock = |id, good, amount, accessible| MealSourceFacts {
            inventory: InventoryId(id),
            good,
            quantity: Quantity(amount),
            accessible,
        };
        let facts = [
            stock(8, Good::Food, 1000, true),
            stock(2, Good::Water, 2000, true),
            stock(5, Good::Food, 1000, false),
            stock(3, Good::Food, 499, true),
            stock(9, Good::Food, 500, true),
        ];
        assert_eq!(
            meal_sources(facts, Quantity(500), false),
            vec![InventoryId(8), InventoryId(9)]
        );
        assert!(meal_sources(facts, Quantity(500), true).is_empty());
        assert_eq!(
            meal_sources([stock(7, Good::Food, 0, true)], Quantity(0), false),
            vec![InventoryId(7)]
        );
    }
}
