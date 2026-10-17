//! Carried clothing provides warmth, wears out, and can be repaired with real cloth.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Garment {
    pub inventory: InventoryId,
    pub good: GoodId,
    pub condition: u32,
    #[serde(default = "worn")]
    pub equipped: bool,
}
fn worn() -> bool {
    true
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Equipment {
    pub enabled: bool,
    pub worn: Table<PersonId, Garment>,
    pub next: Table<PersonId, SimTime>,
    #[serde(default)]
    pub conditions: Table<PersonId, Table<GoodId, u32>>,
}
impl Resource for Equipment {
    const KEY: &'static str = "equipment.worn";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "equipment.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Equip {
    pub person: PersonId,
    pub inventory: InventoryId,
    pub good: GoodId,
}
impl Request for Equip {
    const KEY: &'static str = "equipment.equip";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Remove {
    pub person: PersonId,
}
impl Request for Remove {
    const KEY: &'static str = "equipment.remove";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Repair {
    pub person: PersonId,
    pub inventory: InventoryId,
    pub material: GoodId,
}
impl Request for Repair {
    const KEY: &'static str = "equipment.repair";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Exposed {
    pub person: PersonId,
    pub warmth: i32,
    pub protection: i32,
    pub damage: u64,
}
impl Event for Exposed {
    const KEY: &'static str = "equipment.exposed";
}
fn insulation(o: &game_objects::Object) -> i32 {
    o.tags
        .iter()
        .find_map(|t| t.strip_prefix("warmth:")?.parse().ok())
        .unwrap_or(0)
}
pub fn protection(tx: &Transaction<'_>, person: PersonId) -> i32 {
    let Some(w) = tx
        .get::<Equipment>()
        .ok()
        .and_then(|s| s.worn.get(&person))
        .filter(|w| w.equipped)
    else {
        return 0;
    };
    let Some(inv) = tx
        .get::<game_cargo::Cargo>()
        .ok()
        .and_then(|c| c.0.get(&w.inventory))
    else {
        return 0;
    };
    if inv.amount(w.good) == 0 || inv.custody != game_cargo::Custody::Carried(person) {
        return 0;
    }
    let Ok(object) = tx
        .get::<game_objects::Objects>()
        .and_then(|o| o.by_id(w.good))
    else {
        return 0;
    };
    let rain = game_transport::surrounding(tx.get().unwrap(), person, tx.now())
        .and_then(|a| game_environment::at(tx, a).ok())
        .map_or(0, |s| s.rain);
    let dry = insulation(object) * w.condition as i32 / 100;
    if rain >= 6 && !object.tags.contains("weatherproof") {
        dry * 2 / 3
    } else {
        dry
    }
}
fn consider(tx: &mut Transaction<'_>, p: PersonId) -> Result<()> {
    if !tx.get::<Equipment>()?.enabled
        || Some(p) == tx.get::<game_lineage::Lineage>()?.current
        || tx
            .get::<Equipment>()?
            .next
            .get(&p)
            .is_some_and(|at| *at > tx.now())
    {
        return Ok(());
    }
    let Ok(area) = game_transport::area(tx.get()?, p) else {
        return Ok(());
    };
    let next = tx.now().after(SimDuration(7 * DAY.0))?;
    tx.edit::<Equipment>()?.next.insert(p, next);
    let mut choices = vec![];
    for inv in tx.get::<game_cargo::Cargo>()?.carried(p) {
        for (good, q) in &tx.get::<game_cargo::Cargo>()?.0[&inv].goods {
            let warmth = insulation(tx.get::<game_objects::Objects>()?.by_id(*good)?);
            if *q > 0 && warmth > 0 {
                choices.push((warmth, inv, *good));
            }
        }
    }
    let needed = (10
        - game_environment::at(tx, area)?.warmth
        - game_vitality::ability(tx, p, "hardiness") as i32 / 200)
        .max(0);
    let best = choices
        .into_iter()
        .min_by_key(|(warmth, _, good)| ((warmth - needed).abs(), *good));
    if let Some((_warmth, inventory, good)) =
        best.filter(|(warmth, _, _)| (*warmth - needed).abs() <= needed)
        && tx
            .get::<Equipment>()?
            .worn
            .get(&p)
            .is_none_or(|w| w.good != good || w.inventory != inventory || !w.equipped)
    {
        tx.call(Equip {
            person: p,
            inventory,
            good,
        })?;
    } else if best.is_none_or(|(warmth, _, _)| (warmth - needed).abs() > needed) {
        tx.call(Remove { person: p })?;
    }
    if let Some(w) = tx.get::<Equipment>()?.worn.get(&p).cloned()
        && w.equipped
        && tx
            .get::<game_cargo::Cargo>()?
            .0
            .get(&w.inventory)
            .is_some_and(|i| i.amount(w.good) > 0 && i.custody == game_cargo::Custody::Carried(p))
    {
        tx.edit::<Equipment>()?.worn.get_mut(&p).unwrap().condition = w.condition.saturating_sub(1);
        if w.condition < 40 {
            let material = tx
                .get::<game_objects::Objects>()?
                .items
                .values()
                .find(|o| {
                    o.tags.contains("repair-cloth")
                        && tx.get::<game_cargo::Cargo>().unwrap().0[&w.inventory].amount(o.id) > 0
                })
                .map(|o| o.id);
            if let Some(material) = material {
                let _ = tx.attempt(Repair {
                    person: p,
                    inventory: w.inventory,
                    material,
                });
            }
        }
    }
    Ok(())
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("equipment", 1)?;
    b.resource::<Equipment>()?;
    b.request::<Enable>(|tx, _| {
        tx.edit::<Equipment>()?.enabled = true;
        Ok(())
    })?;
    b.request::<Remove>(|tx, r| {
        game_physiology::capable(tx, r.person)?;
        if let Some(w) = tx.edit::<Equipment>()?.worn.get_mut(&r.person) {
            w.equipped = false;
        }
        Ok(())
    })?;
    b.request::<Equip>(|tx, r| {
        game_physiology::capable(tx, r.person)?;
        let inv = tx
            .get::<game_cargo::Cargo>()?
            .0
            .get(&r.inventory)
            .ok_or_else(|| Error::Missing("inventory".into()))?;
        require(
            inv.custody == game_cargo::Custody::Carried(r.person) && inv.amount(r.good) > 0,
            "garment not carried",
        )?;
        require(
            insulation(tx.get::<game_objects::Objects>()?.by_id(r.good)?) > 0,
            "not insulating clothing",
        )?;
        tx.call(game_property::Use {
            actor: r.person,
            inventory: r.inventory,
            purpose: "wearing".into(),
        })?;
        if let Some(w) = tx.get::<Equipment>()?.worn.get(&r.person).cloned() {
            tx.edit::<Equipment>()?
                .conditions
                .entry(r.person)
                .or_default()
                .insert(w.good, w.condition);
        }
        let condition = tx
            .get::<Equipment>()?
            .conditions
            .get(&r.person)
            .and_then(|rows| rows.get(&r.good))
            .copied()
            .unwrap_or(100);
        tx.edit::<Equipment>()?.worn.insert(
            r.person,
            Garment {
                inventory: r.inventory,
                good: r.good,
                condition,
                equipped: true,
            },
        );
        Ok(())
    })?;
    b.request::<Repair>(|tx, r| {
        game_physiology::capable(tx, r.person)?;
        let worn = tx
            .get::<Equipment>()?
            .worn
            .get(&r.person)
            .ok_or_else(|| Error::Missing("garment".into()))?;
        require(
            worn.inventory == r.inventory
                && game_cargo::locality(tx, r.inventory)?
                    == game_transport::area(tx.get()?, r.person)?,
            "repair requires access",
        )?;
        require(
            tx.get::<game_objects::Objects>()?
                .by_id(r.material)?
                .tags
                .contains("repair-cloth"),
            "repair needs cloth",
        )?;
        tx.call(game_property::Use {
            actor: r.person,
            inventory: r.inventory,
            purpose: "repairing".into(),
        })?;
        tx.call(game_cargo::Consume {
            inventory: r.inventory,
            good: r.material,
            quantity: 1,
        })?;
        tx.edit::<Equipment>()?
            .worn
            .get_mut(&r.person)
            .unwrap()
            .condition = 100;
        Ok(())
    })?;
    b.subscribe::<game_characters::IdleReview>("b.equipment.weather", |tx, e| {
        consider(tx, e.person)
    })?;
    b.validate_save(|h| {
        require(
            h.get::<Equipment>()?
                .worn
                .values()
                .all(|g| g.condition <= 100),
            "invalid garment condition",
        )
    });
    Ok(())
}
