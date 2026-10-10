//! Natural deposits, personally observed opportunities and physical collection.
use game_core::*;
use game_environment::DayFacts;
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
impl Resource {
    pub fn validate(&self) -> Result<(), Error> {
        if self.capacity.0 == 0
            || self.remaining > self.capacity
            || self.decay_permille > 1_000
            || self.contamination > 1_000
            || self.runoff_contamination > 1_000
            || self.growth_remainder >= 1_000_000
            || self.water_capacity == 0
            || self.soil_water > self.water_capacity
            || self.optimal_water == 0
            || self.optimal_water > self.water_capacity
            || self.minimum_temperature >= self.optimum_temperature
            || !(-100..=100).contains(&self.minimum_temperature)
            || !(-100..=100).contains(&self.optimum_temperature)
            || self.labor_per_unit.0 == 0
            || self.workers == 0
        {
            return Err(Error::InvalidConfiguration(
                "invalid natural deposit".into(),
            ));
        }
        Ok(())
    }
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
impl Plot {
    pub fn validate(&self) -> Result<(), Error> {
        if self.fertility > 1_000
            || self.water_capacity == 0
            || self.soil_water > self.water_capacity
        {
            return Err(Error::InvalidConfiguration(
                "invalid undeveloped plot".into(),
            ));
        }
        Ok(())
    }
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
    pub rules: EcologyRules,
    pub resources: BTreeMap<ResourceId, Resource>,
    pub plots: BTreeMap<SiteId, Plot>,
    pub jobs: BTreeMap<PersonId, Gathering>,
    pub known: BTreeMap<PersonId, BTreeMap<ResourceId, ResourceView>>,
    pub known_plots: BTreeMap<PersonId, BTreeMap<SiteId, PlotView>>,
    pub observed_areas: BTreeMap<PersonId, BTreeMap<AreaId, SimTime>>,
    pub day: u64,
    #[serde(default)]
    pub next_request: BTreeMap<PersonId, BTreeMap<ResourceId, SimTime>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EcologyRules {
    pub base_temperature: i32,
    pub soil_recovery: u16,
    pub search_retry: SimDuration,
}

#[derive(Clone, Copy, Debug)]
pub struct LocalAcquisition {
    pub resource: ResourceId,
    pub can_prepare: bool,
    pub can_request: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcquisitionProspect {
    pub resource: ResourceId,
    pub area: AreaId,
    pub observed_at: SimTime,
    pub quantity: Quantity,
    pub needs_tool: bool,
    pub action: EcologyAction,
}
#[derive(Clone, Copy, Debug)]
pub struct AcquisitionNeed {
    pub actor: PersonId,
    pub area: AreaId,
    pub now: SimTime,
    pub good: Good,
    pub quantity: Quantity,
    pub room: Quantity,
}
impl Default for EcologyRules {
    fn default() -> Self {
        Self {
            base_temperature: 5,
            soil_recovery: 1,
            search_retry: SimDuration(21_600_000),
        }
    }
}
impl EcologyRules {
    pub fn validate(&self) -> Result<(), Error> {
        if !(-100..=100).contains(&self.base_temperature)
            || self.soil_recovery > 1_000
            || self.search_retry.0 == 0
        {
            return Err(Error::InvalidConfiguration(
                "invalid ecological land recovery".into(),
            ));
        }
        Ok(())
    }
}
impl World {
    /// Remote prospects consume the person's dated resource views only. Local
    /// preparation facts come from the physical collection owner at this point.
    pub fn acquisition_prospects(
        &self,
        need: AcquisitionNeed,
        local: &[LocalAcquisition],
    ) -> Vec<AcquisitionProspect> {
        let mut prospects = Vec::new();
        for known in self
            .known
            .get(&need.actor)
            .into_iter()
            .flat_map(|known| known.values())
        {
            if known.kind.good() != need.good
                || known.observed_at > need.now
                || known.available.0 == 0
            {
                continue;
            }
            let local = local.iter().find(|facts| facts.resource == known.id);
            let local_here = known.area == need.area;
            if !known.accessible
                && known.custodian.is_some()
                && (!local_here || local.is_some_and(|facts| facts.can_request))
                && !self
                    .next_request
                    .get(&need.actor)
                    .and_then(|requests| requests.get(&known.id))
                    .is_some_and(|at| *at > need.now)
            {
                prospects.push(AcquisitionProspect {
                    resource: known.id,
                    area: known.area,
                    observed_at: known.observed_at,
                    quantity: Quantity(0),
                    needs_tool: false,
                    action: EcologyAction::RequestAccess { resource: known.id },
                });
            }
            let quantity = Quantity(need.quantity.0.min(need.room.0).min(known.available.0));
            if quantity.0 > 0 && (!local_here || local.is_some_and(|facts| facts.can_prepare)) {
                prospects.push(AcquisitionProspect {
                    resource: known.id,
                    area: known.area,
                    observed_at: known.observed_at,
                    quantity,
                    needs_tool: known.kind == ResourceKind::Coppice,
                    action: EcologyAction::Gather {
                        resource: known.id,
                        quantity,
                        unauthorized: !known.accessible,
                    },
                });
            }
        }
        prospects
    }
    /// Supplied only after physically inspecting this place. Receiving a report
    /// updates source knowledge separately and cannot fabricate this observation.
    pub fn observe_area(&mut self, actor: PersonId, area: AreaId, now: SimTime) {
        let observed = self.observed_areas.entry(actor).or_default();
        if observed.get(&area).is_some_and(|at| *at > now) {
            return;
        }
        if observed.len() == 16
            && !observed.contains_key(&area)
            && let Some(oldest) = observed
                .iter()
                .min_by_key(|(id, at)| (**at, **id))
                .map(|(id, _)| *id)
        {
            observed.remove(&oldest);
        }
        observed.insert(area, now);
    }
    pub fn exploration_areas(
        &self,
        actor: PersonId,
        area: AreaId,
        now: SimTime,
        known_areas: &[AreaId],
    ) -> Vec<(AreaId, SimTime)> {
        let Some(observed) = self.observed_areas.get(&actor) else {
            return Vec::new();
        };
        let Some(inspected) = observed.get(&area).copied().filter(|at| *at <= now) else {
            return Vec::new();
        };
        known_areas
            .iter()
            .take(16)
            .copied()
            .filter(|target| *target != area)
            .filter(|target| {
                observed
                    .get(target)
                    .is_none_or(|at| now.0.saturating_sub(at.0) >= self.rules.search_retry.0)
            })
            .map(|target| (target, inspected))
            .collect()
    }
    pub fn advance(&self, input: &DayFacts) -> Result<Self, Error> {
        self.rules.validate()?;
        input.weather.validate()?;
        for resource in self.resources.values() {
            resource.validate()?;
        }
        for plot in self.plots.values() {
            plot.validate()?;
        }
        let mut world = self.clone();
        let day = input.day;
        if day == world.day {
            return Ok(world);
        }
        if day != world.day.checked_add(1).ok_or(Error::Overflow)? {
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
            if input.weather.temperature > world.rules.base_temperature && plot.soil_water > 0 {
                plot.fertility = plot
                    .fertility
                    .saturating_add(world.rules.soil_recovery)
                    .min(1_000);
            }
        }
        world.day = day;
        Ok(world)
    }

    pub fn gathering_conditions(
        &self,
        actor: PersonId,
        resource: ResourceId,
        quantity: Quantity,
        unauthorized: bool,
        facts: GatherFacts,
    ) -> Result<GatheringConditions, Error> {
        let source = self.resources.get(&resource).ok_or(Error::Unavailable)?;
        source.validate()?;
        let mut reserved = 0u128;
        let mut working = 0usize;
        for (person, job) in &self.jobs {
            if *person != actor && job.resource == resource {
                reserved = reserved
                    .checked_add(u128::from(job.quantity.0))
                    .ok_or(Error::Overflow)?;
                working = working.checked_add(1).ok_or(Error::Overflow)?;
            }
        }
        if quantity.0 == 0
            || !facts.capable
            || source.area != facts.area
            || (!unauthorized && !source.access.contains(&actor))
            || working >= source.workers
            || u128::from(quantity.0)
                .checked_add(reserved)
                .ok_or(Error::Overflow)?
                > u128::from(source.remaining.0)
            || quantity > facts.carrying_room
            || (!facts.adult && (source.kind == ResourceKind::Coppice || quantity.0 > 1_000))
        {
            return Err(Error::Unavailable);
        }
        let duration = u64::try_from(
            (u128::from(source.labor_per_unit.0) * u128::from(quantity.0)).div_ceil(1_000),
        )
        .map_err(|_| Error::Overflow)?;
        let completes = SimTime(
            facts
                .now
                .0
                .checked_add(duration.max(1))
                .ok_or(Error::Overflow)?,
        );
        let tool_required = source.kind == ResourceKind::Coppice;
        Ok(GatheringConditions {
            completes,
            tool_required,
            ready: !tool_required || facts.tool.is_some(),
        })
    }
    pub fn prepare_gather(
        &self,
        actor: PersonId,
        resource: ResourceId,
        quantity: Quantity,
        unauthorized: bool,
        facts: GatherFacts,
    ) -> Result<GatheringPlan, Error> {
        let conditions =
            self.gathering_conditions(actor, resource, quantity, unauthorized, facts)?;
        if !conditions.ready {
            return Err(Error::Unavailable);
        }
        let source = &self.resources[&resource];
        Ok(GatheringPlan {
            resource,
            quantity,
            good: source.kind.good(),
            tool: if conditions.tool_required {
                facts.tool
            } else {
                None
            },
            completes: conditions.completes,
            unauthorized,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GatheringConditions {
    pub completes: SimTime,
    pub tool_required: bool,
    pub ready: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct GatherFacts {
    pub now: SimTime,
    pub area: AreaId,
    pub capable: bool,
    pub adult: bool,
    pub carrying_room: Quantity,
    /// An externally validated, actually usable tool slot.
    pub tool: Option<InventoryId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatheringPlan {
    pub resource: ResourceId,
    pub quantity: Quantity,
    pub good: Good,
    pub tool: Option<InventoryId>,
    pub completes: SimTime,
    pub unauthorized: bool,
}
impl GatheringPlan {
    /// The coordinator allocates or selects a real cargo compartment only after
    /// preparation succeeds; this domain never manufactures an inventory ID.
    pub fn job(&self, destination: InventoryId) -> Gathering {
        Gathering {
            resource: self.resource,
            destination,
            quantity: self.quantity,
            tool: self.tool,
            completes: self.completes,
            unauthorized: self.unauthorized,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct CollectionFacts {
    pub now: SimTime,
    pub area: AreaId,
    pub capable: bool,
    pub cargo_available: bool,
    pub tool_available: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectionEffect {
    pub resource: ResourceId,
    pub remaining: Quantity,
    pub destination: InventoryId,
    pub good: Good,
    pub quantity: Quantity,
    /// Water's collected contamination; health owns the resulting stock lot.
    pub contamination: Option<u16>,
}
pub fn prepare_collection(
    source: &Resource,
    actor: PersonId,
    job: &Gathering,
    facts: CollectionFacts,
) -> Result<CollectionEffect, Error> {
    source.validate()?;
    if source.id != job.resource
        || facts.now < job.completes
        || !facts.capable
        || facts.area != source.area
        || !facts.cargo_available
        || (job.tool.is_some() && !facts.tool_available)
        || (!job.unauthorized && !source.access.contains(&actor))
        || job.quantity.0 == 0
        || source.remaining < job.quantity
        || source.contamination > 1_000
        || (source.kind == ResourceKind::Coppice) != job.tool.is_some()
    {
        return Err(Error::Unavailable);
    }
    Ok(CollectionEffect {
        resource: source.id,
        remaining: Quantity(source.remaining.0 - job.quantity.0),
        destination: job.destination,
        good: source.kind.good(),
        quantity: job.quantity,
        contamination: (source.kind == ResourceKind::Spring).then_some(source.contamination),
    })
}

#[cfg(test)]
mod tests;
