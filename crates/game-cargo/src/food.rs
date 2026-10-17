use super::*;
use std::{
    cell::RefCell,
    collections::BTreeSet,
    sync::{Arc, Weak},
};
#[derive(Default)]
struct Index {
    previous: Records<InventoryId, Inventory>,
    rules: Weak<std::collections::BTreeMap<GoodId, game_objects::Food>>,
    edible: BTreeSet<InventoryId>,
}
thread_local! {static INDEX:RefCell<Index>=RefCell::new(Index::default());}
impl Cargo {
    /// Physical current stocks, not character knowledge. Decision owners must
    /// intersect these with perceived opportunities before proposing actions.
    pub fn food_stores(&self, objects: &game_objects::Objects) -> Vec<InventoryId> {
        INDEX.with(|slot| {
            let mut index = slot.borrow_mut();
            if index.rules.as_ptr() != Arc::as_ptr(objects.food_rules()) {
                *index = Index {
                    rules: Arc::downgrade(objects.food_rules()),
                    ..Default::default()
                };
            }
            let removed: Vec<_> = index
                .previous
                .changed(&self.0)
                .filter(|(id, _)| !self.0.contains_key(id))
                .map(|(id, _)| *id)
                .collect();
            for id in removed {
                index.edible.remove(&id);
            }
            let previous = index.previous.clone();
            for (id, stock) in self.0.changed(&previous) {
                if objects.has_meal(&stock.goods) {
                    index.edible.insert(*id);
                } else {
                    index.edible.remove(id);
                }
            }
            index.previous = self.0.clone();
            index.edible.iter().copied().collect()
        })
    }
}
