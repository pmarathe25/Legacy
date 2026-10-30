//! Translate authored physical goods into expiry-aware custody.
use game_parallel_lot_model::{Container, Creation, Lot, Place, State, Transaction};
use game_parallel_production_model::{Good, Inventory};
use rayon::prelude::*;
use std::collections::HashMap;

pub struct Catalog {
    shelf_ticks: HashMap<u32, Option<u64>>,
}
/// Trusted environmental harvest and consumption whose lot details are unnecessary.
#[derive(Default)]
pub struct Growth<'a> {
    pub harvest: &'a [Transaction],
    pub compact_consumption: &'a [u64],
}
pub fn combine_growth(
    external: &[Transaction],
    growth: &[Transaction],
) -> Option<Vec<Transaction>> {
    (!growth.is_empty()).then(|| {
        external
            .par_iter()
            .chain(growth.par_iter())
            .cloned()
            .collect()
    })
}
/// The environment may produce only freshly dated output into regional stock.
pub fn validate_growth(
    catalog: &Catalog,
    actions: &[Transaction],
    tick: u64,
) -> Result<(), String> {
    if actions.par_iter().any(|action| {
        !action.movements.is_empty()
            || action.creations.len() != 1
            || action.creations.iter().any(|row| {
                !matches!(row.to, Place::Stock { good, .. } if good == row.lot.good)
                    || catalog
                        .fresh(row.lot.good, row.lot.quantity, tick)
                        .ok()
                        .is_none_or(|lot| lot != row.lot)
            })
    }) {
        return Err("invalid environmental output".into());
    }
    Ok(())
}
impl Catalog {
    pub fn compile(goods: &[Good], ticks_per_day: u64) -> Result<Self, String> {
        if ticks_per_day == 0 {
            return Err("invalid ticks per day".into());
        }
        Self::convert(goods, |days| {
            u64::from(days)
                .checked_mul(ticks_per_day)
                .ok_or("shelf life overflow".into())
        })
    }
    /// Round shelf life up to the first boundary at or after the authored duration.
    pub fn from_tick_millis(goods: &[Good], tick_millis: u64) -> Result<Self, String> {
        if tick_millis == 0 {
            return Err("invalid tick duration".into());
        }
        Self::convert(goods, |days| {
            let millis = u128::from(days) * 86_400_000;
            u64::try_from(millis.div_ceil(u128::from(tick_millis)))
                .map_err(|_| "shelf life overflow".into())
        })
    }
    fn convert(
        goods: &[Good],
        duration: impl Fn(u32) -> Result<u64, String>,
    ) -> Result<Self, String> {
        let mut shelf_ticks = HashMap::new();
        for good in goods {
            let shelf = good.food.as_ref().and_then(|food| food.shelf_days);
            if good.id == 0 || shelf == Some(0) {
                return Err("invalid physical good".into());
            }
            let shelf = shelf.map(&duration).transpose()?;
            if shelf_ticks.insert(good.id, shelf).is_some() {
                return Err("duplicate physical good".into());
            }
        }
        Ok(Self { shelf_ticks })
    }
    pub fn contains(&self, good: u32) -> bool {
        self.shelf_ticks.contains_key(&good)
    }
    /// Date genuinely new production at the boundary when it was made.
    pub fn fresh(&self, good: u32, quantity: u64, created_at: u64) -> Result<Lot, String> {
        if quantity == 0 {
            return Err("zero created quantity".into());
        }
        let shelf = self.shelf_ticks.get(&good).ok_or("unknown physical good")?;
        let expires_at = shelf
            .map(|ticks| {
                created_at
                    .checked_add(ticks)
                    .ok_or("shelf life overflow".to_string())
            })
            .transpose()?;
        Ok(Lot {
            good,
            quantity,
            expires_at,
        })
    }
    pub fn creation(
        &self,
        id: u64,
        to: Place,
        good: u32,
        quantity: u64,
        created_at: u64,
    ) -> Result<Creation, String> {
        if id == 0 || !to.accepts(good) {
            return Err("invalid output custody".into());
        }
        Ok(Creation {
            id,
            to,
            lot: self.fresh(good, quantity, created_at)?,
        })
    }
    /// Starting inventories are newly created at `tick`, not imported historical stock.
    pub fn fresh_world(&self, inventories: &[Inventory], tick: u64) -> Result<State, String> {
        let mut containers: Vec<_> = inventories
            .par_iter()
            .flat_map_iter(|inventory| {
                inventory.stocks.iter().map(move |stock| {
                    let place = Place::Stock {
                        region: inventory.region,
                        good: stock.good,
                    };
                    if !self.shelf_ticks.contains_key(&stock.good) {
                        return Err("unknown stock good".to_string());
                    }
                    let lots = if stock.quantity == 0 {
                        Vec::new()
                    } else {
                        vec![self.fresh(stock.good, stock.quantity, tick)?]
                    };
                    Container::new(place, stock.capacity, lots)
                })
            })
            .collect::<Result<_, _>>()?;
        containers.par_sort_unstable_by_key(|row| row.place);
        if containers
            .par_windows(2)
            .any(|rows| rows[0].place == rows[1].place)
        {
            return Err("duplicate stock custody".into());
        }
        Ok(State {
            tick,
            revision: 0,
            containers,
        })
    }
}

#[cfg(test)]
#[path = "../tests/support/behavior.rs"]
mod tests;
