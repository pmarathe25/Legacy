//! Natural deposits, personally observed opportunities and physical collection.
use game_core::*;
pub use game_land::Soil;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
mod advancing;
mod observation;
mod opportunities;
mod transitions;
pub use observation::{ObserverArea, PreparedObservations};
pub use opportunities::{AutonomousOptions, GatherOpportunity};
pub use transitions::*;
mod storage;
pub use advancing::{PlotInput, PreparedAdvance, ResourceInput};
pub use storage::{Queries, register_components};
pub use storage::{
    all_knowledge, all_requests, ensure_knowledge, ensure_requests, gathering, gathering_mut,
    gatherings, insert_gathering, insert_knowledge, insert_requests, knowledge, knowledge_mut,
    remove_gathering, remove_knowledge, remove_requests, requests, requests_mut,
};

/// Daily forcing for one local cell, independent of its weather provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeatherFacts {
    pub temperature: i32,
    pub rain: u64,
    pub evaporation: u64,
}
impl WeatherFacts {
    pub fn validate(&self) -> Result<(), Error> {
        if !(-200..=200).contains(&self.temperature)
            || self.rain > 1_000_100
            || self.evaporation > 1_000_000
        {
            return Err(Error::InvalidConfiguration(
                "invalid local ecology weather".into(),
            ));
        }
        Ok(())
    }
}

/// An explicit calendar boundary and the cells whose weather is available.
/// Missing cells do not receive inferred global weather.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalWeatherDay {
    pub day: u64,
    pub day_of_year: u64,
    pub cells: BTreeMap<AreaId, WeatherFacts>,
}
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
        if self.id.0 == 0
            || self.area.0 == 0
            || self.capacity.0 == 0
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
#[serde(deny_unknown_fields)]
pub struct Plot {
    pub id: SiteId,
    pub area: AreaId,
    pub access: BTreeSet<PersonId>,
    pub custodian: Option<PersonId>,
}
impl Plot {
    pub fn validate(&self) -> Result<(), Error> {
        if self.id.0 == 0 || self.area.0 == 0 {
            return Err(Error::InvalidConfiguration(
                "invalid undeveloped plot".into(),
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gathering {
    pub resource: ResourceId,
    pub destination: InventoryId,
    pub quantity: Quantity,
    pub tool: Option<InventoryId>,
    pub completes: SimTime,
    pub unauthorized: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Knowledge {
    pub known_resources: BTreeMap<ResourceId, ResourceView>,
    pub known_plots: BTreeMap<SiteId, PlotView>,
    pub observed_areas: BTreeMap<AreaId, SimTime>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requests {
    pub next_request: BTreeMap<ResourceId, SimTime>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub rules: EcologyRules,
    pub day: u64,
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
impl Runtime {
    /// Remote prospects consume the person's dated resource views only. Local
    /// preparation facts come from the physical collection owner at this point.
    pub fn acquisition_prospects(
        &self,
        store: &game_ecs::Store,
        need: AcquisitionNeed,
        local: &[LocalAcquisition],
    ) -> Vec<AcquisitionProspect> {
        let mut prospects = Vec::new();
        for known in self
            .knowledge(store, need.actor)
            .into_iter()
            .flat_map(|known| known.known_resources.values())
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
                    .requests(store, need.actor)
                    .and_then(|requests| requests.next_request.get(&known.id))
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
    pub fn observe_area(
        &self,
        store: &mut game_ecs::Store,
        actor: PersonId,
        area: AreaId,
        now: SimTime,
    ) -> Result<(), Error> {
        let mut knowledge = self.knowledge(store, actor).cloned().unwrap_or_default();
        if !observation::remember_area(&mut knowledge, area, now) {
            return Ok(());
        }
        self.insert_knowledge(store, actor, knowledge)
    }
    pub fn exploration_areas(
        &self,
        store: &game_ecs::Store,
        actor: PersonId,
        area: AreaId,
        now: SimTime,
        known_areas: &[AreaId],
    ) -> Vec<(AreaId, SimTime)> {
        let Some(knowledge) = self.knowledge(store, actor) else {
            return Vec::new();
        };
        let observed = &knowledge.observed_areas;
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
    /// A source/body ceiling. Access, work spaces, gear and the chosen amount's
    /// deadline are validated separately by prepare_gather.
    pub fn gather_upper_bound(
        &self,
        store: &game_ecs::Store,
        actor: PersonId,
        resource: ResourceId,
        facts: GatherFacts,
    ) -> Result<Quantity, Error> {
        let source = self.resource(store, resource).ok_or(Error::Unavailable)?;
        source.validate()?;
        let (reserved, _) = self.gathering_reservations(store, actor, resource)?;
        let available = u128::from(source.remaining.0).saturating_sub(reserved) as u64;
        Ok(Quantity(
            available
                .min(facts.carrying_room.0)
                .min(if facts.adult { u64::MAX } else { 1_000 }),
        ))
    }

    fn gathering_reservations(
        &self,
        store: &game_ecs::Store,
        actor: PersonId,
        resource: ResourceId,
    ) -> Result<(u128, usize), Error> {
        let mut reserved = 0u128;
        let mut working = 0usize;
        for (person, job) in self.gatherings(store) {
            if person != actor && job.resource == resource {
                reserved = reserved
                    .checked_add(u128::from(job.quantity.0))
                    .ok_or(Error::Overflow)?;
                working = working.checked_add(1).ok_or(Error::Overflow)?;
            }
        }
        Ok((reserved, working))
    }

    pub fn gathering_conditions(
        &self,
        store: &game_ecs::Store,
        actor: PersonId,
        resource: ResourceId,
        quantity: Quantity,
        unauthorized: bool,
        facts: GatherFacts,
    ) -> Result<GatheringConditions, Error> {
        let source = self.resource(store, resource).ok_or(Error::Unavailable)?;
        source.validate()?;
        let (reserved, working) = self.gathering_reservations(store, actor, resource)?;
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
        store: &game_ecs::Store,
        actor: PersonId,
        resource: ResourceId,
        quantity: Quantity,
        unauthorized: bool,
        facts: GatherFacts,
    ) -> Result<GatheringPlan, Error> {
        let conditions =
            self.gathering_conditions(store, actor, resource, quantity, unauthorized, facts)?;
        if !conditions.ready {
            return Err(Error::Unavailable);
        }
        let source = self.resource(store, resource).ok_or(Error::Unavailable)?;
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[cfg(test)]
mod free_port_tests;

mod cancellation;
pub use cancellation::PreparedGatheringCancellation;

/// Apply already accepted site permissions through their physical owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedSiteAccess {
    before: BTreeMap<SiteId, Plot>,
    after: BTreeMap<SiteId, Plot>,
}
impl Runtime {
    pub fn prepare_site_access(
        &self,
        store: &game_ecs::Store,
        changes: &[(SiteId, PersonId, PersonId, bool)],
    ) -> Result<PreparedSiteAccess, Error> {
        let mut before = BTreeMap::new();
        let mut after: BTreeMap<SiteId, Plot> = BTreeMap::new();
        for (id, grantor, grantee, allowed) in changes {
            let original = self.plot(store, *id).ok_or(Error::Unavailable)?;
            if original.custodian != Some(*grantor) || grantee.0 == 0 {
                return Err(Error::Unavailable);
            }
            before.entry(*id).or_insert_with(|| original.clone());
            let row = after.entry(*id).or_insert_with(|| original.clone());
            if *allowed {
                row.access.insert(*grantee);
            } else {
                row.access.remove(grantee);
            }
        }
        Ok(PreparedSiteAccess { before, after })
    }
}
impl PreparedSiteAccess {
    pub fn check(&self, runtime: &Runtime, store: &game_ecs::Store) -> Result<(), Error> {
        if self
            .before
            .iter()
            .any(|(id, row)| runtime.plot(store, *id) != Some(row))
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn stage(&self, batch: &mut game_ecs::Batch) {
        for (id, row) in &self.after {
            batch.insert(game_ecs::StableId(id.0), row.clone());
        }
    }
}
