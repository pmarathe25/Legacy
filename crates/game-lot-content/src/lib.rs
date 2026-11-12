//! Translate authored physical goods into expiry-aware custody.
use game_lot_model::{Container, Creation, Lot, Place, State, Transaction};
use game_production_model::{Good, Inventory};
use rayon::prelude::*;
use std::collections::HashMap;

pub struct Catalog {
    shelf_ticks: HashMap<u32, Option<u64>>,
}
/// Trusted environmental harvest and consumption whose lot details are unnecessary.
#[derive(Default)]
pub struct Growth<'a> {
    pub harvest: &'a [Transaction],
    /// Domain-validated animal yield; every accepted creation is tied to a staged animal death.
    pub animal_yield: &'a [AnimalYield],
    /// Renewable products, distinct from death-linked hunting yields.
    pub animal_products: &'a [AnimalProduct],
    /// Domain-validated writing consumes materials to create an identified page.
    pub writing: &'a [WritingPage],
    pub compact_consumption: &'a [u64],
    /// Domain-validated gathering into personal custody.
    pub gathered: &'a [Gathered],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gathered {
    pub action: u64,
    pub person: u64,
    pub good: u32,
    pub quantity: u64,
}

/// Generate one fresh personal output per independently admitted gatherer.
pub fn prepare_gathered(
    catalog: &Catalog,
    outputs: &[Gathered],
    tick: u64,
) -> Result<Vec<Transaction>, String> {
    let mut people: Vec<_> = outputs.par_iter().map(|row| row.person).collect();
    let mut actions: Vec<_> = outputs.par_iter().map(|row| row.action).collect();
    people.par_sort_unstable();
    actions.par_sort_unstable();
    if people.first() == Some(&0)
        || actions.first() == Some(&0)
        || people.par_windows(2).any(|rows| rows[0] == rows[1])
        || actions.par_windows(2).any(|rows| rows[0] == rows[1])
    {
        return Err("invalid gathering output identity".into());
    }
    outputs
        .par_iter()
        .map(|row| {
            Ok(Transaction {
                id: row.action,
                movements: vec![],
                creations: vec![catalog.creation(
                    row.action,
                    Place::Person(row.person),
                    row.good,
                    row.quantity,
                    tick,
                )?],
            })
        })
        .collect()
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WritingPage {
    pub action: u64,
    pub writer: u64,
    pub paper: u32,
    pub paper_quantity: u64,
    pub ink: u32,
    pub ink_quantity: u64,
    pub page: u32,
    pub item: u64,
}

pub fn prepare_writing(
    catalog: &Catalog,
    pages: &[WritingPage],
    tick: u64,
) -> Result<Vec<Transaction>, String> {
    pages
        .par_iter()
        .map(|p| {
            if p.action == 0
                || p.writer == 0
                || p.item == 0
                || p.paper == p.ink
                || p.paper == p.page
                || p.ink == p.page
                || p.paper_quantity == 0
                || p.ink_quantity == 0
                || !catalog.contains(p.paper)
                || !catalog.contains(p.ink)
            {
                return Err("invalid writing action".into());
            }
            let mut lot = catalog.fresh(p.page, 1, tick)?;
            if lot.expires_at.is_some() {
                return Err("written pages must not expire".into());
            }
            lot.item = Some(p.item);
            let place = Place::Person(p.writer);
            Ok(Transaction {
                id: p.action,
                movements: vec![
                    game_lot_model::Movement {
                        id: 1,
                        from: place,
                        to: None,
                        good: p.paper,
                        quantity: p.paper_quantity,
                        item: None,
                    },
                    game_lot_model::Movement {
                        id: 2,
                        from: place,
                        to: None,
                        good: p.ink,
                        quantity: p.ink_quantity,
                        item: None,
                    },
                ],
                creations: vec![Creation {
                    id: 3,
                    to: place,
                    lot,
                }],
            })
        })
        .collect()
}
/// A domain-validated yield tied to one prey and one hunter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimalYield {
    pub action: u64,
    pub animal: u64,
    pub hunter: u64,
    pub good: u32,
    pub quantity: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimalProduct {
    pub action: u64,
    pub animal: u64,
    pub collector: u64,
    pub good: u32,
    pub quantity: u64,
}
/// Produce freshly dated goods only for distinct animal and collector claims.
pub fn prepare_animal_products(
    catalog: &Catalog,
    products: &[AnimalProduct],
    tick: u64,
) -> Result<Vec<Transaction>, String> {
    let mapped: Vec<_> = products
        .par_iter()
        .map(|p| AnimalYield {
            action: p.action,
            animal: p.animal,
            hunter: p.collector,
            good: p.good,
            quantity: p.quantity,
        })
        .collect();
    prepare_animal_yield(catalog, &mapped, tick)
}
pub fn combine_growth(
    external: &[Transaction],
    growth: &[Transaction],
    animal_yield: &[Transaction],
) -> Option<Vec<Transaction>> {
    (!growth.is_empty() || !animal_yield.is_empty()).then(|| {
        external
            .par_iter()
            .chain(growth.par_iter())
            .chain(animal_yield.par_iter())
            .cloned()
            .collect()
    })
}
/// Generate canonical dated output only for unique prey and hunters selected by the animal owner.
pub fn prepare_animal_yield(
    catalog: &Catalog,
    yields: &[AnimalYield],
    tick: u64,
) -> Result<Vec<Transaction>, String> {
    let mut animals: Vec<_> = yields.par_iter().map(|y| y.animal).collect();
    let mut hunters: Vec<_> = yields.par_iter().map(|y| y.hunter).collect();
    let mut actions: Vec<_> = yields.par_iter().map(|y| y.action).collect();
    animals.par_sort_unstable();
    hunters.par_sort_unstable();
    actions.par_sort_unstable();
    if animals.first() == Some(&0)
        || hunters.first() == Some(&0)
        || actions.first() == Some(&0)
        || animals.par_windows(2).any(|p| p[0] == p[1])
        || hunters.par_windows(2).any(|p| p[0] == p[1])
        || actions.par_windows(2).any(|p| p[0] == p[1])
    {
        return Err("invalid animal yield identity".into());
    }
    yields
        .par_iter()
        .map(|y| {
            Ok(Transaction {
                id: y.action,
                movements: vec![],
                creations: vec![catalog.creation(
                    y.action,
                    Place::Person(y.hunter),
                    y.good,
                    y.quantity,
                    tick,
                )?],
            })
        })
        .collect()
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
            batch: None,
            item: None,
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
