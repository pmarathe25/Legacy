use super::*;
use std::{cell::RefCell, collections::BTreeMap};
#[derive(Default)]
struct Index {
    previous: Records<InventoryId, Inventory>,
    goods: BTreeMap<GoodId, BTreeMap<InventoryId, u64>>,
}
thread_local! {static INDEX:RefCell<Index>=RefCell::new(Index::default());}
impl Cargo {
    /// Current physical amounts. This index grants no character knowledge.
    pub fn stores_with(&self, good: GoodId, minimum: u64) -> Vec<InventoryId> {
        INDEX.with(|slot| {
            let mut index = slot.borrow_mut();
            let previous = index.previous.clone();
            for (id, stock) in previous.changed(&self.0) {
                for g in stock.goods.keys() {
                    if let Some(rows) = index.goods.get_mut(g) {
                        rows.remove(id);
                        if rows.is_empty() {
                            index.goods.remove(g);
                        }
                    }
                }
            }
            for (id, stock) in self.0.changed(&previous) {
                for (g, q) in &stock.goods {
                    if *q > 0 {
                        index.goods.entry(*g).or_default().insert(*id, *q);
                    }
                }
            }
            index.previous = self.0.clone();
            if minimum == 0 {
                return self.0.keys().copied().collect();
            }
            index
                .goods
                .get(&good)
                .into_iter()
                .flat_map(|rows| rows.iter())
                .filter(|(_, q)| **q >= minimum)
                .map(|(id, _)| *id)
                .collect()
        })
    }
}
