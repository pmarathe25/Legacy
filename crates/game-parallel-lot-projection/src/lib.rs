//! Read-only projections for existing individual decision planners.
pub mod agreement;
pub mod market;
mod receipts;
use game_parallel_goods_cargo::{GoodsCargo, GoodsHolding};
use game_parallel_lots::{Place, View};
use game_parallel_production_model::{Amount, Inventory, Stock};
use rayon::prelude::*;

pub fn inventories(view: &View) -> Vec<Inventory> {
    let stocks: Vec<_> = view
        .containers
        .par_iter()
        .filter_map(|container| match container.place {
            Place::Stock { region, good } => Some((
                region,
                Stock {
                    good,
                    quantity: container.quantity(good),
                    capacity: container.capacity,
                },
            )),
            _ => None,
        })
        .collect();
    stocks
        .par_chunk_by(|a, b| a.0 == b.0)
        .map(|group| Inventory {
            region: group[0].0,
            stocks: group.iter().map(|row| row.1.clone()).collect(),
        })
        .collect()
}
pub fn production(
    runtime: &game_parallel_lot_runtime::Runtime,
    view: &View,
    hunger: u32,
) -> game_parallel_production::Production {
    game_parallel_production::Production {
        max_work_hunger: hunger,
        goods: runtime.goods().to_vec(),
        recipes: runtime.recipes().to_vec(),
        recipe_categories: vec![],
        inventories: inventories(view),
        jobs: vec![],
        events: vec![],
    }
}
/// The old cargo type has one shared capacity; heterogeneous holders are rejected.
pub fn cargo(view: &View, capacity: u64) -> Result<GoodsCargo, String> {
    if capacity == 0
        || view
            .containers
            .par_iter()
            .any(|row| matches!(row.place, Place::Person(_)) && row.capacity != capacity)
    {
        return Err("incompatible personal carrying capacities".into());
    }
    let holdings = view
        .containers
        .par_iter()
        .filter_map(|container| match container.place {
            Place::Person(person) if !container.goods.is_empty() => Some(GoodsHolding {
                person,
                items: container
                    .goods
                    .iter()
                    .map(|item| Amount {
                        good: item.good,
                        quantity: item.quantity,
                    })
                    .collect(),
            }),
            _ => None,
        })
        .collect();
    Ok(GoodsCargo { capacity, holdings })
}

#[cfg(test)]
#[path = "../tests/support/behavior.rs"]
mod tests;
