//! Prepared batches continue curing or fermenting after the worker leaves.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lot {
    pub quantity: u64,
    pub ready: SimTime,
    pub output: GoodId,
    pub minimum: i32,
    pub maximum: i32,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Maturation {
    pub enabled: bool,
    pub lots: Table<InventoryId, Table<GoodId, VecDeque<Lot>>>,
    pub due: Table<SimTime, BTreeSet<(InventoryId, GoodId)>>,
    pub finished: u64,
    pub failed: u64,
}
impl Resource for Maturation {
    const KEY: &'static str = "maturation.lots";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "maturation.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Ripen;
impl Request for Ripen {
    const KEY: &'static str = "maturation.ripen";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Matured {
    pub inventory: InventoryId,
    pub input: GoodId,
    pub output: GoodId,
    pub quantity: u64,
    pub lost: u64,
}
impl Event for Matured {
    const KEY: &'static str = "maturation.matured";
}
fn put(tx: &mut Transaction<'_>, inventory: InventoryId, good: GoodId, lot: Lot) -> Result<()> {
    if !tx.get::<Maturation>()?.due.contains_key(&lot.ready) {
        tx.schedule(lot.ready, 6, Ripen)?;
    }
    let s = tx.edit::<Maturation>()?;
    s.due
        .entry(lot.ready)
        .or_default()
        .insert((inventory, good));
    let lots = s
        .lots
        .entry(inventory)
        .or_default()
        .entry(good)
        .or_default();
    lots.push_back(lot);
    lots.make_contiguous().sort_by_key(|l| l.ready);
    Ok(())
}
fn fresh(
    tx: &mut Transaction<'_>,
    inventory: InventoryId,
    good: GoodId,
    quantity: u64,
) -> Result<()> {
    let objects = tx.get::<game_objects::Objects>()?;
    let object = objects.by_id(good)?;
    let Some(key) = object.tags.iter().find_map(|t| t.strip_prefix("matures:")) else {
        return Ok(());
    };
    let output = objects.resolve(key)?;
    require(
        objects.mass(output, 1)? <= object.mass,
        "mature output exceeds the prepared mass",
    )?;
    let days = object
        .tags
        .iter()
        .find_map(|t| t.strip_prefix("maturation-days:")?.parse::<u64>().ok())
        .filter(|d| *d > 0)
        .ok_or_else(|| Error::Rejected("maturation needs positive duration".into()))?;
    let minimum = object
        .tags
        .iter()
        .find_map(|t| t.strip_prefix("maturation-min:")?.parse().ok())
        .unwrap_or(-100);
    let maximum = object
        .tags
        .iter()
        .find_map(|t| t.strip_prefix("maturation-max:")?.parse().ok())
        .unwrap_or(100);
    let ready = tx
        .now()
        .after(SimDuration(days.checked_mul(DAY.0).ok_or(Error::Overflow)?))?;
    put(
        tx,
        inventory,
        good,
        Lot {
            quantity,
            ready,
            output,
            minimum,
            maximum,
        },
    )
}
fn take(
    tx: &mut Transaction<'_>,
    inventory: InventoryId,
    good: GoodId,
    quantity: u64,
) -> Result<Vec<Lot>> {
    let s = tx.edit::<Maturation>()?;
    let mut left = quantity;
    let mut taken = vec![];
    let lots = s
        .lots
        .get_mut(&inventory)
        .and_then(|r| r.get_mut(&good))
        .ok_or_else(|| Error::Missing("prepared batch".into()))?;
    while left > 0 {
        let mut lot = lots
            .pop_front()
            .ok_or_else(|| Error::Rejected("prepared quantity mismatch".into()))?;
        let q = left.min(lot.quantity);
        left -= q;
        lot.quantity -= q;
        taken.push(Lot {
            quantity: q,
            ..lot.clone()
        });
        if lot.quantity > 0 {
            lots.push_front(lot);
        }
    }
    if lots.is_empty() {
        s.lots.get_mut(&inventory).unwrap().remove(&good);
        if s.lots[&inventory].is_empty() {
            s.lots.remove(&inventory);
        }
    }
    Ok(taken)
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("maturation", 1)?;
    b.resource::<Maturation>()?;
    b.request::<Enable>(|tx, _| {
        if tx.get::<Maturation>()?.enabled {
            return Ok(());
        }
        tx.edit::<Maturation>()?.enabled = true;
        let goods: Vec<_> = tx
            .get::<game_cargo::Cargo>()?
            .0
            .iter()
            .flat_map(|(id, i)| i.goods.iter().map(move |(g, q)| (*id, *g, *q)))
            .collect();
        for (i, g, q) in goods {
            if q > 0 {
                fresh(tx, i, g, q)?;
            }
        }
        Ok(())
    })?;
    b.subscribe::<game_cargo::Changed>("maturation.stock", |tx, e| {
        if !tx.get::<Maturation>()?.enabled
            || !tx
                .get::<game_objects::Objects>()?
                .by_id(e.good)?
                .tags
                .iter()
                .any(|t| t.starts_with("matures:"))
        {
            return Ok(());
        }
        if e.added {
            fresh(tx, e.inventory, e.good, e.quantity)?;
        } else {
            let lots = take(tx, e.inventory, e.good, e.quantity)?;
            if let Some(to) = e.destination {
                for lot in lots {
                    put(tx, to, e.good, lot)?;
                }
            }
        }
        Ok(())
    })?;
    b.request::<Ripen>(|tx, _| {
        let now = tx.now();
        let mut due = tx
            .edit::<Maturation>()?
            .due
            .remove(&now)
            .unwrap_or_default();
        let selected: Vec<_> = (0..128).filter_map(|_| due.pop_first()).collect();
        if !due.is_empty() {
            tx.edit::<Maturation>()?.due.insert(now, due);
            tx.schedule(now, 6, Ripen)?;
        }
        for (inventory, input) in selected {
            let lots: Vec<_> = tx
                .get::<Maturation>()?
                .lots
                .get(&inventory)
                .and_then(|r| r.get(&input))
                .into_iter()
                .flatten()
                .take_while(|l| l.ready <= now)
                .cloned()
                .collect();
            for lot in lots {
                let temperature = game_food_storage::temperature(tx, inventory)?;
                let usable = (lot.minimum..=lot.maximum).contains(&temperature);
                tx.call(game_cargo::Consume {
                    inventory,
                    good: input,
                    quantity: lot.quantity,
                })?;
                if usable {
                    tx.call(game_cargo::Deposit {
                        inventory,
                        good: lot.output,
                        quantity: lot.quantity,
                    })?;
                }
                let s = tx.edit::<Maturation>()?;
                if usable {
                    s.finished = s.finished.saturating_add(lot.quantity);
                } else {
                    s.failed = s.failed.saturating_add(lot.quantity);
                }
                tx.emit(
                    Matured {
                        inventory,
                        input,
                        output: lot.output,
                        quantity: if usable { lot.quantity } else { 0 },
                        lost: if usable { 0 } else { lot.quantity },
                    },
                    vec![],
                )?;
            }
        }
        Ok(())
    })?;
    b.validate_save(|h| {
        let s = h.get::<Maturation>()?;
        if !s.enabled {
            return Ok(());
        }
        for (inventory, rows) in &s.lots {
            for (good, lots) in rows {
                require(
                    h.get::<game_cargo::Cargo>()?
                        .0
                        .get(inventory)
                        .is_some_and(|i| {
                            i.amount(*good) == lots.iter().map(|l| l.quantity).sum::<u64>()
                        }),
                    "prepared quantity mismatch",
                )?;
            }
        }
        Ok(())
    });
    Ok(())
}
