use super::*;
use game_agriculture::{Crops, Farms, Phase};
fn stores(
    tx: &Transaction<'_>,
    p: PersonId,
    area: AreaId,
) -> Result<Vec<(InventoryId, game_knowledge::Stock)>> {
    let cargo = tx.get::<game_cargo::Cargo>()?;
    let mut stocks: std::collections::BTreeMap<_, _> = cargo
        .carried(p)
        .map(|id| (id, cargo.0[&id].goods.clone()))
        .collect();
    if let Some(facts) = tx.get::<game_knowledge::Knowledge>()?.0.get(&p) {
        for (key, fact) in game_knowledge::with_prefix(facts, "store:").take(64) {
            let Some(id) = key
                .strip_prefix("store:")
                .and_then(|s| s.parse().ok())
                .map(InventoryId)
            else {
                continue;
            };
            let location = game_knowledge::known::<game_knowledge::StoreLocation>(
                facts,
                &game_knowledge::location_key(id),
            );
            if location.is_some_and(|l| l.area == area)
                && let Some(stock) = fact.value.decode::<game_knowledge::Stock>()
            {
                stocks.entry(id).or_insert(stock);
            }
        }
    }
    Ok(stocks.into_iter().collect())
}
pub(super) fn offer(tx: &mut Transaction<'_>, p: PersonId) -> Result<()> {
    if !tx.get::<Gardens>()?.enabled {
        return Ok(());
    }
    let Ok(area) = game_transport::area(tx.get()?, p) else {
        return Ok(());
    };
    let climate = game_environment::at(tx, area)?.clone();
    let stocks = stores(tx, p, area)?;
    let packs: Vec<_> = tx.get::<game_cargo::Cargo>()?.carried(p).collect();
    let garden = tx.get::<Gardens>()?;
    let mut plots: std::collections::BTreeSet<_> = garden
        .all
        .get(&p)
        .into_iter()
        .flat_map(|r| r.keys())
        .copied()
        .collect();
    plots.extend(garden.plots.get(&p).copied());
    let load = plots.len() as i64;
    let patience = game_vitality::disposition(tx, p, "patience") / 20;
    let foresight = game_vitality::ability(tx, p, "foresight").min(16000) as i64;
    let risk = game_vitality::disposition(tx, p, "risk-tolerance");
    for plot in plots.iter().take(32) {
        let Some(land) = tx.get::<Farms>()?.plots.get(plot) else {
            continue;
        };
        if land.area != area {
            continue;
        }
        let crop = tx.get::<Crops>()?.items[&land.crop].clone();
        match land.phase {
            Phase::Ripe { yield_units } => {
                for inventory in &packs {
                    let inv = &tx.get::<game_cargo::Cargo>()?.0[inventory];
                    let mass = tx.get::<game_objects::Objects>()?.mass(crop.output, 1)?;
                    let quantity = yield_units.min(inv.capacity.saturating_sub(inv.used()?) / mass);
                    if quantity > 0 {
                        game_choice::offer(
                            tx,
                            p,
                            format!("garden:harvest:{}:{}", plot.0, inventory.0),
                            game_choice::motive(
                                0,
                                patience + quantity.min(100) as i64,
                                0,
                                (crop.harvest_labor.0 / MINUTE.0 / 10).min(10000) as i64,
                                0,
                            ),
                            game_agriculture::HarvestSome {
                                person: p,
                                plot: *plot,
                                destination: *inventory,
                                quantity,
                            },
                        )?;
                    }
                }
            }
            Phase::Fallow => {
                for (seed, stock) in &stocks {
                    if stock.get(&crop.seed).copied().unwrap_or(0) < crop.seed_input {
                        continue;
                    }
                    let reluctance = game_property::reluctance(tx, p, *seed);
                    game_choice::offer(
                        tx,
                        p,
                        format!("garden:sow:{}:{}", plot.0, seed.0),
                        game_choice::motive(
                            0,
                            patience + foresight / 100,
                            0,
                            (crop.sow_labor.0 / MINUTE.0 / 10).min(10000) as i64,
                            reluctance,
                        ),
                        game_agriculture::Sow {
                            person: p,
                            plot: *plot,
                            seed: *seed,
                        },
                    )?;
                }
            }
            _ => {}
        }
    }
    let crops = tx.get::<Crops>()?.items.clone();
    for (key, crop) in &crops {
        if !stocks
            .iter()
            .any(|(_, stock)| stock.get(&crop.seed).copied().unwrap_or(0) >= crop.seed_input)
        {
            continue;
        }
        let tradition = tx
            .get::<game_relationships::Social>()?
            .values
            .get(&p)
            .and_then(|v| v.get(format!("cultivation:{key}").as_str()))
            .copied()
            .unwrap_or(0);
        let climate_risk = i64::from(crop.minimum_warmth.saturating_sub(climate.warmth).max(0)) * 2
            + i64::from(crop.minimum_rain.saturating_sub(climate.rain)) * 5;
        let taste = tx
            .get::<game_objects::Objects>()?
            .by_id(crop.output)?
            .food
            .as_ref()
            .map_or(0, |f| {
                game_nutrition::preference(tx, p, crop.output, f) / 10
            });
        for (inventory, stock) in &stocks {
            if stock.get(&crop.seed).copied().unwrap_or(0) < crop.seed_input {
                continue;
            }
            let held = packs
                .iter()
                .map(|id| tx.get::<game_cargo::Cargo>().unwrap().0[id].amount(crop.output))
                .sum::<u64>();
            let desire = patience + foresight / 100 + i64::from(tradition) + taste;
            let effort = (crop.sow_labor.0 / MINUTE.0 / 10).min(10000) as i64
                + load * 4
                + held.min(200) as i64 / 5;
            let danger =
                climate_risk * (1000 - risk) / 1000 + game_property::reluctance(tx, p, *inventory);
            game_choice::offer(
                tx,
                p,
                format!("garden:plant:{key}:{}", inventory.0),
                game_choice::motive(0, desire, 0, effort, danger),
                Plant {
                    person: p,
                    crop: key.clone(),
                    inventory: *inventory,
                },
            )?;
        }
    }
    Ok(())
}
