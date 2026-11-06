//! Data-file conversion for species with named food goods.
use crate::{Animal, Climate, Fauna, HuntYield, SocialResponse, Species, handling, restraint};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    species: Vec<SpeciesConfig>,
    animals: Vec<Animal>,
    next_id: u64,
    #[serde(default)]
    last_tick: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpeciesConfig {
    id: u32,
    name: String,
    food_good: String,
    intake: u64,
    hunger_per_tick: u32,
    relief_per_unit: u32,
    starvation_threshold: u32,
    starvation_damage: u32,
    max_health: u32,
    adult_age: u64,
    fertile_until: u64,
    lifespan: u64,
    gestation: u64,
    conception_one_in: u64,
    breeding_max_hunger: u32,
    newborn_health: u32,
    #[serde(default)]
    mutation_per_million: u32,
    #[serde(default)]
    mutation_step: u16,
    #[serde(default)]
    climate: Option<Climate>,
    #[serde(default)]
    hunting: Option<HuntYieldConfig>,
    #[serde(default)]
    newborn_social_response: Option<SocialResponse>,
    #[serde(default)]
    newborn_handling_response: Option<handling::Response>,
    #[serde(default)]
    newborn_restraint_response: Option<restraint::Response>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HuntYieldConfig {
    good: String,
    quantity: u64,
    success_per_thousand: u32,
}
impl Config {
    pub fn build(self, lookup: impl Fn(&str) -> Option<u32>) -> Result<Fauna, String> {
        let species = self
            .species
            .into_iter()
            .map(|s| {
                Ok(Species {
                    id: s.id,
                    name: s.name,
                    food_good: lookup(&s.food_good)
                        .ok_or_else(|| format!("unknown animal food {}", s.food_good))?,
                    intake: s.intake,
                    hunger_per_tick: s.hunger_per_tick,
                    relief_per_unit: s.relief_per_unit,
                    starvation_threshold: s.starvation_threshold,
                    starvation_damage: s.starvation_damage,
                    max_health: s.max_health,
                    adult_age: s.adult_age,
                    fertile_until: s.fertile_until,
                    lifespan: s.lifespan,
                    gestation: s.gestation,
                    conception_one_in: s.conception_one_in,
                    breeding_max_hunger: s.breeding_max_hunger,
                    newborn_health: s.newborn_health,
                    mutation_per_million: s.mutation_per_million,
                    mutation_step: s.mutation_step,
                    climate: s.climate,
                    hunting: s
                        .hunting
                        .map(|yield_| -> Result<_, String> {
                            Ok(HuntYield {
                                good: lookup(&yield_.good)
                                    .ok_or_else(|| format!("unknown hunt yield {}", yield_.good))?,
                                quantity: yield_.quantity,
                                success_per_thousand: yield_.success_per_thousand,
                            })
                        })
                        .transpose()?,
                    newborn_social_response: s.newborn_social_response,
                    newborn_handling_response: s.newborn_handling_response,
                    newborn_restraint_response: s.newborn_restraint_response,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Fauna {
            species,
            animals: self.animals,
            next_id: self.next_id,
            last_tick: self.last_tick,
        })
    }
}
