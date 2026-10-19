//! FIFO food batches keep their expiry during transport; spoilage consumes real stock.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};
mod calendar;
mod conditions;
pub use conditions::{Conditions, Refresh};
pub fn temperature_factor(degrees: i32) -> u32 {
    match degrees {
        i32::MIN..=0 => 8000,
        1..=8 => 3000,
        9..=16 => 1500,
        17..=29 => 1000,
        _ => 500,
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Batch {
    pub quantity: u64,
    pub expires: SimTime,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preservation {
    pub enabled: bool,
    pub batches: Table<InventoryId, Table<GoodId, VecDeque<Batch>>>,
    pub due: Table<SimTime, BTreeSet<(InventoryId, GoodId)>>,
    pub spoiled: u64,
    #[serde(default)]
    pub rates: Table<InventoryId, u32>,
    #[serde(default)]
    timers: Table<SimTime, ScheduleId>,
    #[serde(default)]
    indexed: bool,
}
impl Resource for Preservation {
    const KEY: &'static str = "preservation.batches";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "preservation.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Expire;
impl Request for Expire {
    const KEY: &'static str = "preservation.expire";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Spoiled {
    pub inventory: InventoryId,
    pub good: GoodId,
    pub quantity: u64,
}
impl Event for Spoiled {
    const KEY: &'static str = "preservation.spoiled";
}
fn shelf(tx: &Transaction<'_>, good: GoodId) -> Result<Option<u64>> {
    Ok(tx.get::<game_objects::Objects>()?.by_id(good)?.shelf_days())
}
fn put(tx: &mut Transaction<'_>, inventory: InventoryId, good: GoodId, batch: Batch) -> Result<()> {
    calendar::ensure(tx)?;
    let at = batch.expires;
    calendar::wake(tx, at)?;
    let s = tx.edit::<Preservation>()?;
    let lots = s
        .batches
        .entry(inventory)
        .or_default()
        .entry(good)
        .or_default();
    lots.push_back(batch);
    lots.make_contiguous().sort_by_key(|b| b.expires);
    s.due.entry(at).or_default().insert((inventory, good));
    Ok(())
}
fn take(
    tx: &mut Transaction<'_>,
    inventory: InventoryId,
    good: GoodId,
    quantity: u64,
) -> Result<Vec<Batch>> {
    calendar::ensure(tx)?;
    let mut remaining = quantity;
    let mut result = vec![];
    let s = tx.edit::<Preservation>()?;
    if let Some(lots) = s.batches.get_mut(&inventory).and_then(|r| r.get_mut(&good)) {
        while remaining > 0 {
            let mut lot = lots
                .pop_front()
                .ok_or_else(|| Error::Rejected("untracked food".into()))?;
            let q = remaining.min(lot.quantity);
            remaining -= q;
            lot.quantity -= q;
            result.push(Batch {
                quantity: q,
                expires: lot.expires,
            });
            if lot.quantity > 0 {
                lots.push_front(lot);
            }
        }
    }
    require(remaining == 0, "untracked food")?;
    let retired: BTreeSet<_> = result
        .iter()
        .map(|b| b.expires)
        .filter(|at| {
            s.batches
                .get(&inventory)
                .and_then(|rows| rows.get(&good))
                .is_none_or(|lots| lots.iter().all(|lot| lot.expires != *at))
        })
        .collect();
    if s.batches
        .get(&inventory)
        .and_then(|rows| rows.get(&good))
        .is_some_and(|lots| lots.is_empty())
    {
        s.batches.get_mut(&inventory).unwrap().remove(&good);
        if s.batches[&inventory].is_empty() {
            s.batches.remove(&inventory);
        }
    }
    for at in retired {
        calendar::remove(tx, inventory, good, at)?;
    }
    Ok(result)
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("preservation", 1)?;
    b.resource::<Preservation>()?;
    conditions::register(b)?;
    b.request::<Enable>(|tx, _| {
        require(!tx.get::<Preservation>()?.enabled, "already tracking food")?;
        tx.edit::<Preservation>()?.enabled = true;
        let stock: Vec<_> = tx
            .get::<game_cargo::Cargo>()?
            .0
            .iter()
            .flat_map(|(i, s)| s.goods.iter().map(move |(g, q)| (*i, *g, *q)))
            .collect();
        for (i, g, q) in stock {
            if q > 0
                && let Some(days) = shelf(tx, g)?
            {
                let expires = conditions::fresh(tx, i, days)?;
                put(
                    tx,
                    i,
                    g,
                    Batch {
                        quantity: q,
                        expires,
                    },
                )?;
            }
        }
        Ok(())
    })?;
    b.subscribe::<game_cargo::Changed>("preservation.stock", |tx, e| {
        if !tx.get::<Preservation>()?.enabled || shelf(tx, e.good)?.is_none() {
            return Ok(());
        }
        if e.added {
            let days = shelf(tx, e.good)?.unwrap();
            tx.call(Refresh {
                inventory: e.inventory,
            })?;
            let expires = conditions::fresh(tx, e.inventory, days)?;
            put(
                tx,
                e.inventory,
                e.good,
                Batch {
                    quantity: e.quantity,
                    expires,
                },
            )?;
        } else {
            let lots = take(tx, e.inventory, e.good, e.quantity)?;
            if let Some(destination) = e.destination {
                tx.call(Refresh {
                    inventory: destination,
                })?;
                let from = tx
                    .get::<Preservation>()?
                    .rates
                    .get(&e.inventory)
                    .copied()
                    .unwrap_or(1000);
                let to = tx
                    .get::<Preservation>()?
                    .rates
                    .get(&destination)
                    .copied()
                    .unwrap_or(1000);
                for batch in lots {
                    let batch = Batch {
                        quantity: batch.quantity,
                        expires: conditions::retime(tx.now(), batch.expires, from, to)?,
                    };
                    put(tx, destination, e.good, batch)?;
                }
            }
        }
        Ok(())
    })?;
    b.request::<Expire>(|tx, _| {
        let _measurement = game_diagnostics::span("food expiry");
        calendar::ensure(tx)?;
        let at = tx.now();
        if let Some(id) = tx.edit::<Preservation>()?.timers.remove(&at) {
            tx.unschedule(id);
        }
        let mut targets = tx
            .edit::<Preservation>()?
            .due
            .remove(&at)
            .unwrap_or_default();
        let batch: Vec<_> = (0..128).filter_map(|_| targets.pop_first()).collect();
        if !targets.is_empty() {
            tx.edit::<Preservation>()?.due.insert(at, targets);
            calendar::wake(tx, at)?;
        }
        for (inventory, good) in batch {
            let amount = tx
                .get::<Preservation>()?
                .batches
                .get(&inventory)
                .and_then(|s| s.get(&good))
                .map_or(0, |lots| {
                    lots.iter()
                        .take_while(|l| l.expires <= at)
                        .map(|l| l.quantity)
                        .sum()
                });
            if amount > 0 {
                tx.call(game_cargo::Consume {
                    inventory,
                    good,
                    quantity: amount,
                })?;
                tx.edit::<Preservation>()?.spoiled =
                    tx.get::<Preservation>()?.spoiled.saturating_add(amount);
                tx.emit(
                    Spoiled {
                        inventory,
                        good,
                        quantity: amount,
                    },
                    vec![],
                )?;
            }
        }
        Ok(())
    })?;
    b.validate_save(|h| {
        let s = h.get::<Preservation>()?;
        if !s.enabled {
            return Ok(());
        }
        for (id, inv) in &h.get::<game_cargo::Cargo>()?.0 {
            for (g, q) in &inv.goods {
                if h.get::<game_objects::Objects>()?.by_id(*g)?.food.is_none() {
                    continue;
                }
                let tracked: u64 = s
                    .batches
                    .get(id)
                    .and_then(|r| r.get(g))
                    .map_or(0, |l| l.iter().map(|b| b.quantity).sum());
                require(*q == tracked, "food batch quantity mismatch")?;
            }
        }
        Ok(())
    });
    Ok(())
}
