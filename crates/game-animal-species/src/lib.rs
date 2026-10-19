//! Resolved physical species profiles, independent of encounters and character decisions.
use game_core::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Species {
    pub name: String,
    pub body_mass: u64,
    pub speed: u64,
    pub maturity_days: u64,
    pub lifespan_days: u64,
    pub gestation_days: u64,
    pub intake: u64,
    pub minimum_warmth: i32,
    pub maximum_warmth: i32,
    pub meat: GoodId,
    pub yield_units: u64,
}
impl Species {
    pub fn valid(&self) -> bool {
        !self.name.is_empty()
            && self.body_mass > 0
            && self.speed > 0
            && self.maturity_days > 0
            && self.lifespan_days > self.maturity_days
            && self.lifespan_days <= 1_000_000
            && (1..=1_000_000).contains(&self.gestation_days)
            && self.intake > 0
            && self.minimum_warmth <= self.maximum_warmth
            && self.yield_units > 0
    }
}
#[derive(Serialize, Deserialize)]
struct Spec {
    name: String,
    body_mass: u64,
    speed: u64,
    maturity_days: u64,
    lifespan_days: u64,
    gestation_days: u64,
    intake: u64,
    minimum_warmth: i32,
    maximum_warmth: i32,
    meat: String,
    yield_units: u64,
}
#[derive(Serialize, Deserialize)]
struct Definitions {
    species: BTreeMap<String, Spec>,
}
pub fn load(
    store: &mut game_content::Store,
    objects: &game_objects::Objects,
) -> Result<BTreeMap<String, Species>> {
    let mut species = BTreeMap::new();
    for path in store.scan("fauna")? {
        let doc: game_content::Document<Definitions> = store.load(&path)?;
        for (key, s) in doc.data.species {
            species.insert(
                key,
                Species {
                    name: s.name,
                    body_mass: s.body_mass,
                    speed: s.speed,
                    maturity_days: s.maturity_days,
                    lifespan_days: s.lifespan_days,
                    gestation_days: s.gestation_days,
                    intake: s.intake,
                    minimum_warmth: s.minimum_warmth,
                    maximum_warmth: s.maximum_warmth,
                    meat: objects.resolve(&s.meat)?,
                    yield_units: s.yield_units,
                },
            );
        }
    }
    require(
        species.values().all(Species::valid),
        "invalid animal biology",
    )?;
    Ok(species)
}
