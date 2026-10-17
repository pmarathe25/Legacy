use super::*;
#[derive(Clone, Serialize, Deserialize)]
struct Weather {
    after: Option<InventoryId>,
}
impl Request for Weather {
    const KEY: &'static str = "food-storage.weather";
    type Response = ();
}
pub(super) fn start(tx: &mut Transaction<'_>) -> Result<()> {
    tx.schedule(tx.now().after(DAY)?, 4, Weather { after: None })?;
    Ok(())
}
pub fn temperature(tx: &Transaction<'_>, inventory: InventoryId) -> Result<i32> {
    let cargo = tx.get::<game_cargo::Cargo>()?;
    let inv = cargo
        .0
        .get(&inventory)
        .ok_or_else(|| Error::Missing("inventory".into()))?;
    let area = match inv.custody {
        game_cargo::Custody::Carried(p) => game_transport::surrounding(tx.get()?, p, tx.now()),
        _ => inv.area(tx.get()?).or(match inv.custody {
            game_cargo::Custody::Road { from, .. } => Some(from),
            _ => None,
        }),
    };
    let Some(area) = area else {
        return Ok(20);
    };
    if tx
        .get::<Stores>()?
        .sites
        .get(&inventory)
        .is_some_and(|s| s.kind == Kind::Cellar)
    {
        let area = game_shelter::outside(tx, area);
        let climate = tx
            .get::<game_environment::Regions>()?
            .0
            .get(&area)
            .unwrap_or(tx.get::<game_environment::Climate>()?);
        let length = climate.year()?;
        return Ok((climate
            .seasons
            .iter()
            .map(|s| i128::from(s.warmth) * i128::from(s.duration.0))
            .sum::<i128>()
            / i128::from(length.max(1))) as i32);
    }
    Ok(game_environment::at(tx, area)?.warmth)
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<game_preservation::Conditions>(|tx, r| {
        if !tx.get::<Stores>()?.enabled {
            return Ok(1000);
        }
        let degrees = temperature(tx, r.inventory)?;
        let base = game_preservation::temperature_factor(degrees);
        Ok(if tx.get::<Stores>()?.sites.contains_key(&r.inventory) {
            base * 3 / 2
        } else {
            base
        })
    })?;
    b.request::<Weather>(|tx, r| {
        let ids: Vec<_> = tx
            .get::<game_preservation::Preservation>()?
            .batches
            .scan_after(r.after.as_ref())
            .take(129)
            .map(|(id, _)| *id)
            .collect();
        for id in ids.iter().take(128) {
            tx.call(game_preservation::Refresh { inventory: *id })?;
        }
        if ids.len() > 128 {
            tx.schedule(
                tx.now(),
                4,
                Weather {
                    after: ids.get(127).copied(),
                },
            )?;
        } else {
            start(tx)?;
        }
        Ok(())
    })?;
    b.subscribe::<game_transport::Arrived>("food-storage.climate", |tx, e| {
        if tx.get::<Stores>()?.enabled {
            let carried: Vec<_> = tx.get::<game_cargo::Cargo>()?.carried(e.person).collect();
            for inventory in carried {
                tx.call(game_preservation::Refresh { inventory })?;
            }
        }
        Ok(())
    })?;
    Ok(())
}
