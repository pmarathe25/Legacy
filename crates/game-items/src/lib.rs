//! Sparse identities for particular physical units, backed by the cargo ledger.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub good: GoodId,
    pub inventory: Option<InventoryId>,
    pub label: String,
    pub recognized_by: PersonId,
    pub at: SimTime,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Items {
    pub objects: Table<u64, Item>,
    pub held: Table<InventoryId, Table<GoodId, BTreeSet<u64>>>,
    #[serde(skip)]
    selected: Option<u64>,
}
impl Resource for Items {
    const KEY: &'static str = "items.identities";
    fn allocated_ids(&self) -> Vec<u64> {
        self.objects.keys().copied().collect()
    }
    fn validate(&self) -> Result<()> {
        require(self.selected.is_none(), "unfinished item transfer")
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Mark {
    pub person: PersonId,
    pub inventory: InventoryId,
    pub good: GoodId,
    pub label: String,
}
impl Request for Mark {
    const KEY: &'static str = "items.mark";
    type Response = u64;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Move {
    pub person: PersonId,
    pub item: u64,
    pub destination: InventoryId,
}
impl Request for Move {
    const KEY: &'static str = "items.move";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Marked {
    pub item: u64,
    pub person: PersonId,
    pub good: GoodId,
    pub label: String,
}
impl Event for Marked {
    const KEY: &'static str = "items.marked";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Moved {
    pub item: u64,
    pub from: InventoryId,
    pub to: Option<InventoryId>,
}
impl Event for Moved {
    const KEY: &'static str = "items.moved";
}
fn container<'a>(tx: &'a Transaction<'_>, item: u64) -> Option<&'a game_cargo::Inventory> {
    let id = tx.get::<Items>().ok()?.objects.get(&item)?.inventory?;
    tx.get::<game_cargo::Cargo>().ok()?.0.get(&id)
}
/// Physical custody for validating an actual presentation, not personal knowledge of its location.
pub fn holder(tx: &Transaction<'_>, item: u64) -> Option<PersonId> {
    match container(tx, item)?.custody {
        game_cargo::Custody::Carried(p) => Some(p),
        _ => None,
    }
}
pub fn exposed(
    tx: &Transaction<'_>,
    observer: PersonId,
    item: u64,
    presenter: Option<PersonId>,
) -> bool {
    exposed_in(
        tx.get().unwrap(),
        tx.get().unwrap(),
        tx.get().unwrap(),
        observer,
        item,
        presenter,
    )
}
pub fn exposed_in(
    items: &Items,
    cargo: &game_cargo::Cargo,
    positions: &game_transport::Positions,
    observer: PersonId,
    item: u64,
    presenter: Option<PersonId>,
) -> bool {
    let Some(inv) = items
        .objects
        .get(&item)
        .and_then(|i| i.inventory)
        .and_then(|i| cargo.0.get(&i))
    else {
        return false;
    };
    let Ok(area) = game_transport::area(positions, observer) else {
        return false;
    };
    inv.area(positions) == Some(area)
        && match inv.custody {
            game_cargo::Custody::Area(_) => true,
            game_cargo::Custody::Carried(p) => p == observer || presenter == Some(p),
            _ => false,
        }
}
pub fn departing(
    s: &Items,
    inventory: InventoryId,
    good: GoodId,
    before: u64,
    quantity: u64,
) -> Vec<u64> {
    let Some(ids) = s.held.get(&inventory).and_then(|r| r.get(&good)) else {
        return vec![];
    };
    let plain = before.saturating_sub(ids.len() as u64);
    ids.iter()
        .take(quantity.saturating_sub(plain).min(ids.len() as u64) as usize)
        .copied()
        .collect()
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Inspect {
    pub person: PersonId,
    pub inventory: InventoryId,
    pub item: u64,
}
impl Request for Inspect {
    const KEY: &'static str = "items.inspect";
    type Response = bool;
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("items", 1)?;
    b.resource::<Items>()?;
    b.request::<Inspect>(|tx, r| {
        require(
            game_transport::area(tx.get()?, r.person)? == game_cargo::locality(tx, r.inventory)?,
            "inspection is not local",
        )?;
        tx.call(game_property::Use {
            actor: r.person,
            inventory: r.inventory,
            purpose: "inspecting named objects".into(),
        })?;
        Ok(tx
            .get::<Items>()?
            .held
            .get(&r.inventory)
            .is_some_and(|goods| goods.values().any(|ids| ids.contains(&r.item))))
    })?;
    b.request::<Mark>(|tx, r| {
        game_physiology::capable(tx, r.person)?;
        require(
            !r.label.trim().is_empty() && r.label.len() <= 200,
            "invalid item label",
        )?;
        require(
            game_transport::area(tx.get()?, r.person)? == game_cargo::locality(tx, r.inventory)?,
            "object is elsewhere",
        )?;
        let held = tx
            .get::<Items>()?
            .held
            .get(&r.inventory)
            .and_then(|rows| rows.get(&r.good))
            .map_or(0, |r| r.len() as u64);
        require(
            tx.get::<game_cargo::Cargo>()?.0[&r.inventory].amount(r.good) > held,
            "no unmarked physical unit",
        )?;
        let id = tx.allocate()?;
        let at = tx.now();
        let s = tx.edit::<Items>()?;
        s.objects.insert(
            id,
            Item {
                good: r.good,
                inventory: Some(r.inventory),
                label: r.label.clone(),
                recognized_by: r.person,
                at,
            },
        );
        s.held
            .entry(r.inventory)
            .or_default()
            .entry(r.good)
            .or_default()
            .insert(id);
        tx.emit(
            Marked {
                item: id,
                person: r.person,
                good: r.good,
                label: r.label,
            },
            vec![r.person],
        )?;
        Ok(id)
    })?;
    b.request::<Move>(|tx, r| {
        game_physiology::capable(tx, r.person)?;
        let item = tx
            .get::<Items>()?
            .objects
            .get(&r.item)
            .cloned()
            .ok_or_else(|| Error::Missing("item".into()))?;
        let from = item
            .inventory
            .ok_or_else(|| Error::Rejected("item was destroyed".into()))?;
        tx.call(game_property::Use {
            actor: r.person,
            inventory: from,
            purpose: "moving a particular object".into(),
        })?;
        tx.call(game_property::Use {
            actor: r.person,
            inventory: r.destination,
            purpose: "placing a particular object".into(),
        })?;
        require(
            tx.get::<Items>()?.selected.is_none(),
            "another item transfer is pending",
        )?;
        tx.edit::<Items>()?.selected = Some(r.item);
        tx.call(game_cargo::Transfer {
            from,
            to: r.destination,
            good: item.good,
            quantity: 1,
        })
    })?;
    b.subscribe::<game_cargo::Changed>("a.items.identity", |tx, e| {
        if e.added {
            return Ok(());
        }
        let moved = if let Some(item) = tx.edit::<Items>()?.selected.take() {
            vec![item]
        } else {
            let after = tx.get::<game_cargo::Cargo>()?.0[&e.inventory].amount(e.good);
            departing(
                tx.get()?,
                e.inventory,
                e.good,
                after.saturating_add(e.quantity),
                e.quantity,
            )
        };
        for item in moved {
            let s = tx.edit::<Items>()?;
            let ids = s
                .held
                .get_mut(&e.inventory)
                .unwrap()
                .get_mut(&e.good)
                .unwrap();
            require(ids.remove(&item), "selected object is not present")?;
            if ids.is_empty() {
                s.held.get_mut(&e.inventory).unwrap().remove(&e.good);
            }
            if s.held[&e.inventory].is_empty() {
                s.held.remove(&e.inventory);
            }
            if let Some(to) = e.destination {
                s.held
                    .entry(to)
                    .or_default()
                    .entry(e.good)
                    .or_default()
                    .insert(item);
            }
            s.objects.get_mut(&item).unwrap().inventory = e.destination;
            tx.emit(
                Moved {
                    item,
                    from: e.inventory,
                    to: e.destination,
                },
                vec![],
            )?;
        }
        Ok(())
    })?;
    b.validate_save(|h| {
        let s = h.get::<Items>()?;
        let mut rebuilt: Table<InventoryId, Table<GoodId, BTreeSet<u64>>> = Table::new();
        for (id, item) in &s.objects {
            if let Some(inv) = item.inventory {
                rebuilt
                    .entry(inv)
                    .or_default()
                    .entry(item.good)
                    .or_default()
                    .insert(*id);
            }
        }
        require(rebuilt == s.held, "invalid object location index")?;
        for (inv, goods) in &s.held {
            for (good, ids) in goods {
                require(
                    h.get::<game_cargo::Cargo>()?
                        .0
                        .get(inv)
                        .is_some_and(|i| i.amount(*good) >= ids.len() as u64),
                    "object identities exceed physical stock",
                )?;
            }
        }
        Ok(())
    });
    Ok(())
}
