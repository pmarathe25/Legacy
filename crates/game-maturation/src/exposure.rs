use super::*;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conditions {
    climate: Option<game_environment::Climate>,
    fixed: i32,
}
fn conditions(tx: &Transaction<'_>, inventory: InventoryId) -> Result<Conditions> {
    let cargo = tx.get::<game_cargo::Cargo>()?;
    let inv = &cargo.0[&inventory];
    let area = match inv.custody {
        game_cargo::Custody::Carried(p) => game_transport::surrounding(tx.get()?, p, tx.now()),
        _ => inv.area(tx.get()?),
    };
    let cellar = tx
        .get::<game_food_storage::Stores>()?
        .sites
        .get(&inventory)
        .is_some_and(|s| s.kind == game_food_storage::Kind::Cellar);
    let climate = if cellar {
        None
    } else {
        area.and_then(|a| game_environment::climate(tx, a).ok())
            .filter(|c| !c.seasons.is_empty())
            .cloned()
    };
    Ok(Conditions {
        climate,
        fixed: game_food_storage::temperature(tx, inventory)?,
    })
}
pub(super) fn refresh(tx: &mut Transaction<'_>, inventory: InventoryId) -> Result<()> {
    if !tx.get::<Maturation>()?.lots.contains_key(&inventory) {
        return Ok(());
    }
    let current = conditions(tx, inventory)?;
    let now = tx.now();
    let rows = tx.edit::<Maturation>()?.lots.get_mut(&inventory).unwrap();
    let goods: Vec<_> = rows.keys().copied().collect();
    for good in goods {
        let lots = rows.get_mut(&good).unwrap();
        for lot in lots {
            if let Some(previous) = &lot.conditions {
                let elapsed = now.0.saturating_sub(lot.checked.0);
                let valid = if let Some(climate) = &previous.climate {
                    climate
                        .matching_duration(lot.checked, now, |s| {
                            (lot.minimum..=lot.maximum).contains(&s.warmth)
                        })?
                        .0
                } else if (lot.minimum..=lot.maximum).contains(&previous.fixed) {
                    elapsed
                } else {
                    0
                };
                lot.elapsed = lot.elapsed.saturating_add(elapsed);
                lot.unsuitable = lot.unsuitable.saturating_add(elapsed.saturating_sub(valid));
            }
            lot.conditions = Some(current.clone());
            lot.checked = now;
        }
    }
    Ok(())
}
pub(super) fn begin(tx: &Transaction<'_>, inventory: InventoryId, lot: &mut Lot) -> Result<()> {
    lot.conditions = Some(conditions(tx, inventory)?);
    lot.checked = tx.now();
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
struct Crossing {
    person: PersonId,
}
impl Request for Crossing {
    const KEY: &'static str = "maturation.crossing";
    type Response = ();
}
fn carried(tx: &mut Transaction<'_>, person: PersonId) -> Result<()> {
    let inventories: Vec<_> = tx.get::<game_cargo::Cargo>()?.carried(person).collect();
    for inventory in inventories {
        refresh(tx, inventory)?;
    }
    Ok(())
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Crossing>(|tx, r| carried(tx, r.person))?;
    b.subscribe::<game_environment::Changed>("maturation.conditions", |tx, _| {
        let inventories: Vec<_> = tx.get::<Maturation>()?.lots.keys().copied().collect();
        for inventory in inventories {
            refresh(tx, inventory)?;
        }
        Ok(())
    })?;
    b.subscribe::<game_transport::Departed>("maturation.travel", |tx, e| {
        carried(tx, e.person)?;
        if let Some(game_transport::Location::Transit {
            departed, arrives, ..
        }) = tx.get::<game_transport::Positions>()?.0.get(&e.person)
        {
            let midpoint = SimTime(departed.0 + (arrives.0 - departed.0).div_ceil(2));
            tx.schedule(midpoint, 5, Crossing { person: e.person })?;
        }
        Ok(())
    })?;
    b.subscribe::<game_transport::Arrived>("maturation.arrival", |tx, e| carried(tx, e.person))?;
    b.subscribe::<ActionFinished>("maturation.interruption", |tx, e| carried(tx, e.person))
}
