//! Voluntary plant tending owns decisions; agriculture owns paid labor and crop phases.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gardens {
    pub enabled: bool,
    pub plots: Table<PersonId, PlotId>,
    pub retry_at: Table<PersonId, SimTime>,
}
impl Resource for Gardens {
    const KEY: &'static str = "horticulture.gardens";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "horticulture.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Consider {
    pub person: PersonId,
}
impl Request for Consider {
    const KEY: &'static str = "horticulture.consider";
    type Response = bool;
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Plant {
    pub person: PersonId,
    pub crop: String,
    pub inventory: InventoryId,
}
impl Request for Plant {
    const KEY: &'static str = "horticulture.plant";
    type Response = PlotId;
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("horticulture", 1)?;
    b.resource::<Gardens>()?;
    b.request::<Enable>(|tx, _| {
        tx.edit::<Gardens>()?.enabled = true;
        Ok(())
    })?;
    b.request::<Consider>(|tx, r| {
        let _measurement = game_diagnostics::span("garden planning");
        let person = r.person;
        if !tx.get::<Gardens>()?.enabled
            || tx.get::<Actions>()?.0.contains_key(&person)
            || tx.get::<game_lineage::Lineage>()?.current == Some(person)
            || game_physiology::capable(tx, person).is_err()
            || game_transport::area(tx.get()?, person).is_err()
            || tx
                .get::<Gardens>()?
                .retry_at
                .get(&person)
                .is_some_and(|at| *at > tx.now())
        {
            return Ok(false);
        }
        let next = tx.now().after(HOUR)?;
        tx.edit::<Gardens>()?.retry_at.insert(person, next);
        let stocks: Vec<_> = tx
            .get::<game_cargo::Cargo>()?
            .carried(person)
            .filter(|id| {
                tx.get::<game_property::Property>()
                    .unwrap()
                    .allowed(person, *id)
            })
            .collect();
        consider(tx, person, &stocks)
    })?;
    b.subscribe::<game_characters::IdleReview>("horticulture.tend", |tx, e| {
        tx.call(Consider { person: e.person })?;
        Ok(())
    })?;
    b.subscribe::<game_physiology::Died>("horticulture.death", |tx, e| {
        tx.edit::<Gardens>()?.retry_at.remove(&e.person);
        Ok(())
    })?;
    b.validate_save(|h| {
        for (person, plot) in &h.get::<Gardens>()?.plots {
            require(
                h.get::<game_agriculture::Farms>()?
                    .plots
                    .get(plot)
                    .is_some_and(|p| p.steward == *person),
                "invalid tended plot",
            )?;
        }
        Ok(())
    });
    b.request::<Plant>(|tx, r| {
        require(
            !tx.get::<Gardens>()?.plots.contains_key(&r.person),
            "already tending land",
        )?;
        let area = game_transport::area(tx.get()?, r.person)?;
        let plot = tx.call(game_agriculture::Create(game_agriculture::Plot {
            area,
            steward: r.person,
            crop: r.crop,
            ..Default::default()
        }))?;
        tx.call(game_agriculture::Sow {
            person: r.person,
            plot,
            seed: r.inventory,
        })?;
        tx.edit::<Gardens>()?.plots.insert(r.person, plot);
        Ok(plot)
    })
}
fn consider(tx: &mut Transaction<'_>, person: PersonId, stocks: &[InventoryId]) -> Result<bool> {
    let area = game_transport::area(tx.get()?, person)?;
    let climate = game_environment::at(tx, area)?.clone();
    if let Some(plot) = tx.get::<Gardens>()?.plots.get(&person).copied() {
        let p = &tx.get::<game_agriculture::Farms>()?.plots[&plot];
        if p.area != area {
            return Ok(false);
        }
        let phase = p.phase.clone();
        let crop = tx.get::<game_agriculture::Crops>()?.items[&p.crop].clone();
        if phase == game_agriculture::Phase::Fallow {
            let held = tx
                .get::<game_property::Property>()?
                .permitted_set(person)
                .into_iter()
                .flat_map(|s| s.keys())
                .filter(|id| game_cargo::locality(tx, **id).is_ok_and(|a| a == area))
                .fold(0u64, |n, id| {
                    n.saturating_add(
                        tx.get::<game_cargo::Cargo>().unwrap().0[id].amount(crop.output),
                    )
                });
            let foresight = game_vitality::ability(tx, person, "foresight");
            let harvests = if foresight < 250 {
                64
            } else {
                (4000 / foresight).clamp(1, 16)
            };
            if held >= crop.yield_units.saturating_mul(harvests) {
                return Ok(false);
            }
        }
        for inventory in stocks {
            if !tx
                .get::<game_property::Property>()?
                .allowed(person, *inventory)
            {
                continue;
            }
            let result = match phase {
                game_agriculture::Phase::Ripe { yield_units: 0 } => tx
                    .attempt(game_agriculture::Harvest {
                        person,
                        plot,
                        destination: *inventory,
                    })
                    .map(|_| ()),
                game_agriculture::Phase::Ripe { yield_units } => {
                    let inv = &tx.get::<game_cargo::Cargo>()?.0[inventory];
                    let mass = tx.get::<game_objects::Objects>()?.mass(crop.output, 1)?;
                    let quantity = yield_units.min(inv.capacity.saturating_sub(inv.used()?) / mass);
                    if quantity == 0 {
                        continue;
                    }
                    tx.attempt(game_agriculture::HarvestSome {
                        person,
                        plot,
                        destination: *inventory,
                        quantity,
                    })
                    .map(|_| ())
                }
                game_agriculture::Phase::Fallow
                    if climate.warmth >= crop.minimum_warmth.saturating_sub(10)
                        && climate.rain.saturating_add(3) >= crop.minimum_rain =>
                {
                    tx.attempt(game_agriculture::Sow {
                        person,
                        plot,
                        seed: *inventory,
                    })
                    .map(|_| ())
                }
                _ => return Ok(false),
            };
            if result.is_ok() {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    // Not everyone wishes to tend plants. This is a disposition, not an assigned profession.
    if game_vitality::disposition(tx, person, "patience") < 450 {
        return Ok(false);
    }
    let mut options: Vec<_> = tx
        .get::<game_agriculture::Crops>()?
        .items
        .iter()
        .filter(|(_, c)| {
            climate.warmth >= c.minimum_warmth.saturating_sub(10)
                && climate.rain.saturating_add(3) >= c.minimum_rain
        })
        .enumerate()
        .map(|(i, (key, c))| {
            (
                random(tx.seed(), person.0 + 711, i as u64),
                key.clone(),
                c.clone(),
            )
        })
        .collect();
    options.sort_by_key(|(rank, _, _)| *rank);
    for (_, crop, c) in options {
        for inventory in stocks {
            if tx.get::<game_cargo::Cargo>()?.0[inventory].amount(c.seed) < c.seed_input {
                continue;
            }
            if tx
                .attempt(Plant {
                    person,
                    crop: crop.clone(),
                    inventory: *inventory,
                })
                .is_ok()
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
