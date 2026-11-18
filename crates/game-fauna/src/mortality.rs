use super::{Animal, Exposure, Species};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeathCause {
    Hunted,
    OldAge,
    Starvation,
    Exposure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimalDeath {
    pub animal: u64,
    pub tick: u64,
    pub cause: DeathCause,
}

/// Physical consequences are ordered by the lifecycle. The first effect to
/// reach zero health supplies the death receipt.
pub(super) fn stage_health(
    animal: &Animal,
    species: &Species,
    age: u64,
    hunger: u32,
    exposure: &[Exposure],
) -> Result<(u32, Option<DeathCause>), &'static str> {
    let (mut health, mut cause) = if age >= species.lifespan {
        (0, Some(DeathCause::OldAge))
    } else if hunger >= species.starvation_threshold {
        let health = animal.health.saturating_sub(species.starvation_damage);
        (health, (health == 0).then_some(DeathCause::Starvation))
    } else {
        (animal.health, None)
    };
    if let Some(climate) = &species.climate {
        let severity = exposure
            .binary_search_by_key(&animal.region, |row| row.region)
            .ok()
            .map_or(0, |i| exposure[i].severity);
        let alleles = animal
            .genome
            .alleles
            .get(climate.hardiness_locus)
            .ok_or("invalid climate hardiness locus")?;
        let mean = (u32::from(alleles[0]) + u32::from(alleles[1])) / 2;
        let inherited = (u64::from(mean) * u64::from(climate.inherited_tolerance) / 1000)
            .min(u64::from(u32::MAX)) as u32;
        let excess = severity.saturating_sub(climate.tolerance.saturating_add(inherited));
        let after = health.saturating_sub(excess.saturating_mul(climate.damage_per_excess));
        if health > 0 && after == 0 {
            cause = Some(DeathCause::Exposure);
        }
        health = after;
    }
    Ok((health, cause))
}
