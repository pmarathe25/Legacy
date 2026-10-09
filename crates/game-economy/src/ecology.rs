//! Natural deposits, personally observed opportunities and physical collection.
use game_core::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ResourceKind {
    Coppice,
    Berries,
    Herbs,
    Spring,
}
impl ResourceKind {
    pub fn good(self) -> Good {
        match self {
            Self::Coppice => Good::Timber,
            Self::Berries => Good::Food,
            Self::Herbs => Good::Herb,
            Self::Spring => Good::Water,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EcologyAction {
    Gather {
        resource: ResourceId,
        quantity: Quantity,
        unauthorized: bool,
    },
    RequestAccess {
        resource: ResourceId,
    },
    GrantAccess {
        resource: ResourceId,
        recipient: PersonId,
        allowed: bool,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceView {
    pub id: ResourceId,
    pub area: AreaId,
    pub kind: ResourceKind,
    pub available: Quantity,
    pub accessible: bool,
    pub custodian: Option<PersonId>,
    pub contamination: u16,
    pub observed_at: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlotView {
    pub id: SiteId,
    pub area: AreaId,
    pub soil_fertility: u16,
    pub soil_water: u64,
    pub accessible: bool,
    pub custodian: Option<PersonId>,
    pub observed_at: SimTime,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EcologyView {
    pub resources: Vec<ResourceView>,
    pub plots: Vec<PlotView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resource {
    pub id: ResourceId,
    pub area: AreaId,
    pub kind: ResourceKind,
    pub remaining: Quantity,
    pub capacity: Quantity,
    pub daily_growth: Quantity,
    pub growth_remainder: u64,
    pub decay_permille: u16,
    pub soil_water: u64,
    pub water_capacity: u64,
    pub optimal_water: u64,
    pub minimum_temperature: i32,
    pub optimum_temperature: i32,
    pub contamination: u16,
    pub runoff_contamination: u16,
    pub labor_per_unit: SimDuration,
    pub workers: usize,
    pub access: BTreeSet<PersonId>,
    pub custodian: Option<PersonId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plot {
    pub id: SiteId,
    pub area: AreaId,
    pub fertility: u16,
    pub soil_water: u64,
    pub water_capacity: u64,
    pub access: BTreeSet<PersonId>,
    pub custodian: Option<PersonId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gathering {
    pub resource: ResourceId,
    pub destination: InventoryId,
    pub quantity: Quantity,
    pub tool: Option<InventoryId>,
    pub completes: SimTime,
    pub unauthorized: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub resources: BTreeMap<ResourceId, Resource>,
    pub plots: BTreeMap<SiteId, Plot>,
    pub jobs: BTreeMap<PersonId, Gathering>,
    pub known: BTreeMap<PersonId, BTreeMap<ResourceId, ResourceView>>,
    pub known_plots: BTreeMap<PersonId, BTreeMap<SiteId, PlotView>>,
    pub day: u64,
    #[serde(default)]
    pub next_request: BTreeMap<PersonId, BTreeMap<ResourceId, SimTime>>,
}
/// A calendar boundary with weather for the elapsed day; no foreign world state.
pub struct RenewalInput<'a> {
    pub day: u64,
    pub weather: &'a crate::environment::Weather,
    pub base_temperature: i32,
    pub soil_recovery: u16,
}
impl World {
    pub fn advance(&self, input: RenewalInput<'_>) -> Result<Self, Error> {
        let mut world = self.clone();
        let day = input.day;
        if day == world.day {
            return Ok(world);
        }
        if day != world.day + 1 {
            return Err(Error::InvalidSave("ecology skipped a calendar day".into()));
        }
        for source in world.resources.values_mut() {
            source.soil_water = source
                .soil_water
                .saturating_add(input.weather.rain)
                .min(source.water_capacity)
                .saturating_sub(input.weather.evaporation);
            let warmth = i64::from(input.weather.temperature - source.minimum_temperature)
                .max(0)
                .min(i64::from(
                    source.optimum_temperature - source.minimum_temperature,
                )) as u64
                * 1_000
                / u64::try_from(source.optimum_temperature - source.minimum_temperature)
                    .map_err(|_| Error::Overflow)?;
            let moisture = (u128::from(source.soil_water.min(source.optimal_water)) * 1_000
                / u128::from(source.optimal_water)) as u64;
            let growth =
                u128::from(source.daily_growth.0) * u128::from(warmth) * u128::from(moisture)
                    + u128::from(source.growth_remainder);
            let available = source.capacity.0 - source.remaining.0;
            let added = u64::try_from((growth / 1_000_000).min(u128::from(available)))
                .map_err(|_| Error::Overflow)?;
            source.growth_remainder = if added == available {
                0
            } else {
                (growth % 1_000_000) as u64
            };
            let decay =
                (u128::from(source.remaining.0) * u128::from(source.decay_permille) / 1_000) as u64;
            let previous = source.remaining.0 - decay;
            source.remaining.0 = previous.saturating_add(added).min(source.capacity.0);
            if source.kind == ResourceKind::Spring && added > 0 {
                source.contamination = ((u128::from(previous) * u128::from(source.contamination)
                    + u128::from(added) * u128::from(source.runoff_contamination))
                    / u128::from(source.remaining.0.max(1)))
                    as u16;
            }
        }
        for plot in world.plots.values_mut() {
            plot.soil_water = plot
                .soil_water
                .saturating_add(if input.weather.temperature > 0 {
                    input.weather.rain
                } else {
                    0
                })
                .min(plot.water_capacity)
                .saturating_sub(input.weather.evaporation);
            if input.weather.temperature > input.base_temperature && plot.soil_water > 0 {
                plot.fertility = plot
                    .fertility
                    .saturating_add(input.soil_recovery)
                    .min(1_000);
            }
        }
        world.day = day;
        Ok(world)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn world() -> World {
        World {
            resources: BTreeMap::from([(
                ResourceId(1),
                Resource {
                    id: ResourceId(1),
                    area: AreaId(2),
                    kind: ResourceKind::Coppice,
                    remaining: Quantity(0),
                    capacity: Quantity(20_000),
                    daily_growth: Quantity(250),
                    growth_remainder: 0,
                    decay_permille: 0,
                    soil_water: 75,
                    water_capacity: 120,
                    optimal_water: 50,
                    minimum_temperature: 0,
                    optimum_temperature: 20,
                    contamination: 0,
                    runoff_contamination: 0,
                    labor_per_unit: SimDuration(3_600_000),
                    workers: 2,
                    access: BTreeSet::new(),
                    custodian: None,
                },
            )]),
            plots: BTreeMap::new(),
            jobs: BTreeMap::new(),
            known: BTreeMap::new(),
            known_plots: BTreeMap::new(),
            day: 0,
            next_request: BTreeMap::new(),
        }
    }
    #[test]
    fn natural_renewal_is_finite_and_changes_no_permissions_or_inventories() {
        let original = world();
        let weather = crate::environment::Weather {
            temperature: 20,
            rain: 30,
            evaporation: 0,
        };
        let next = original
            .advance(RenewalInput {
                day: 1,
                weather: &weather,
                base_temperature: 5,
                soil_recovery: 1,
            })
            .unwrap();
        assert_eq!(next.resources[&ResourceId(1)].remaining, Quantity(250));
        assert!(next.resources[&ResourceId(1)].access.is_empty());
        assert_eq!(original.resources[&ResourceId(1)].remaining, Quantity(0));
        let cold = crate::environment::Weather {
            temperature: -5,
            rain: 30,
            evaporation: 0,
        };
        let frozen = original
            .advance(RenewalInput {
                day: 1,
                weather: &cold,
                base_temperature: 5,
                soil_recovery: 1,
            })
            .unwrap();
        assert_eq!(frozen.resources[&ResourceId(1)].remaining, Quantity(0));
    }
}
