//! Read-only physical quantities for compatibility with decision owners.
use game_parallel_lot_model::{Place, State};
use rayon::prelude::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Balance {
    pub good: u32,
    pub quantity: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContainerView {
    pub place: Place,
    pub capacity: u64,
    pub used: u64,
    pub goods: Vec<Balance>,
}
impl ContainerView {
    pub fn quantity(&self, good: u32) -> u64 {
        self.goods
            .binary_search_by_key(&good, |row| row.good)
            .map_or(0, |index| self.goods[index].quantity)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub boundary: u64,
    pub revision: u64,
    pub containers: Vec<ContainerView>,
}
impl View {
    pub fn container(&self, place: Place) -> Option<&ContainerView> {
        self.containers
            .binary_search_by_key(&place, |row| row.place)
            .ok()
            .map(|index| &self.containers[index])
    }
}
pub fn prepare(state: &State, boundary: u64) -> Result<View, String> {
    if boundary != state.tick && state.tick.checked_add(1) != Some(boundary) {
        return Err("invalid physical view boundary".into());
    }
    let containers = state
        .containers
        .par_iter()
        .map(|container| {
            let goods: Vec<_> = container
                .lots
                .par_chunk_by(|a, b| a.good == b.good)
                .filter_map(|group| {
                    let quantity = group
                        .par_iter()
                        .filter(|lot| lot.expires_at.is_none_or(|at| at > boundary))
                        .map(|lot| lot.quantity)
                        .sum();
                    (quantity > 0).then_some(Balance {
                        good: group[0].good,
                        quantity,
                    })
                })
                .collect();
            let used = goods.iter().map(|row| row.quantity).sum();
            ContainerView {
                place: container.place,
                capacity: container.capacity,
                used,
                goods,
            }
        })
        .collect();
    Ok(View {
        boundary,
        revision: state.revision,
        containers,
    })
}
