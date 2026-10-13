//! Cultivation admission, selection, completion and saved references.
use crate::*;
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, StableId, Store};

/// Factual links supplied by the owners of people, places, stocks and schedules.
/// These methods report identity and existing state; Agriculture decides validity.
pub trait SaveLinks {
    fn next_id(&self) -> u64;
    fn physical_available(&self) -> bool;
    fn weather_day(&self) -> Option<u64>;
    fn area_exists(&self, area: AreaId) -> bool;
    fn person_exists(&self, actor: PersonId) -> bool;
    fn workshop_exists(&self, site: SiteId) -> bool;
    fn inventory(&self, id: InventoryId) -> Option<(Good, Option<AreaId>)>;
    fn actor_location(&self, actor: PersonId) -> Option<Location>;
    fn actor_alive(&self, actor: PersonId) -> bool;
    fn activity(&self, actor: PersonId) -> Option<(SiteId, FarmOperation, SimTime)>;
    fn completion_scheduled(&self, actor: PersonId, at: SimTime) -> bool;
}
impl Runtime {
    /// Upper-bound closing output needed by the transaction's resource budget.
    pub fn output_reserve(
        &self,
        store: &Store,
        field: SiteId,
        operation: FarmOperation,
    ) -> Result<Vec<(SiteId, Good, Quantity)>, Error> {
        if operation != FarmOperation::Harvest {
            return Ok(Vec::new());
        }
        let rules = self.rules_for(store, field)?;
        Ok(vec![
            (field, Good::Food, rules.food_yield),
            (field, Good::Seed, rules.seed_yield),
        ])
    }
    pub fn validate_links(
        &self,
        store: &Store,
        now: SimTime,
        links: &impl SaveLinks,
    ) -> Result<(), Error> {
        let invalid = || Error::InvalidSave("invalid calendar, climate, crop or field".into());
        if links.weather_day().is_some_and(|day| self.day != day) {
            return Err(invalid());
        }
        for (id, field) in self.fields(store) {
            if id != field.id
                || id.0 >= links.next_id()
                || !links.physical_available()
                || links.workshop_exists(id)
                || !links.area_exists(field.area)
                || field
                    .custodian
                    .is_some_and(|actor| !links.person_exists(actor))
                || field
                    .access
                    .iter()
                    .chain(
                        self.crop(store, id)
                            .ok_or_else(invalid)?
                            .contributions
                            .keys(),
                    )
                    .any(|actor| !links.person_exists(*actor))
            {
                return Err(invalid());
            }
            let mut unique = BTreeSet::new();
            for (id, good) in [
                (field.seed, Good::Seed),
                (field.tool, Good::Tool),
                (field.water, Good::Water),
                (field.food, Good::Food),
            ] {
                if !unique.insert(id) || links.inventory(id) != Some((good, Some(field.area))) {
                    return Err(invalid());
                }
            }
        }
        for (actor, job) in self.cultivations(store) {
            let field = self.field(store, job.field).ok_or_else(invalid)?;
            if !links.person_exists(actor)
                || job.completes <= now
                || links.actor_location(actor) != Some(Location::AtArea(field.area))
                || !links.actor_alive(actor)
                || links.activity(actor) != Some((job.field, job.operation, job.completes))
                || !links.completion_scheduled(actor, job.completes)
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
    pub fn select_cultivation(
        &self,
        store: &Store,
        mut facts: impl FnMut(SiteId) -> Option<BeginFacts>,
    ) -> Option<(SiteId, FarmOperation)> {
        self.fields(store)
            .find_map(|(id, _)| self.select_field_cultivation(store, id, facts(id)?))
    }
    pub fn select_field_cultivation(
        &self,
        store: &Store,
        id: SiteId,
        facts: BeginFacts,
    ) -> Option<(SiteId, FarmOperation)> {
        let (field, crop, soil, rules) = self.field_bundle(store, id).ok()?;
        let operation = next_operation(crop, soil, rules)?;
        prepare_begin(field, crop, soil, rules, operation, facts).ok()?;
        Some((id, operation))
    }
    /// The current work claim determines the required tool; no caller chooses it.
    pub fn cultivation_tool(
        &self,
        store: &Store,
        _actor: PersonId,
        has_body: bool,
        activity: Option<SiteId>,
    ) -> Option<InventoryId> {
        if !has_body {
            return None;
        }
        self.field(store, activity?).map(|field| field.tool)
    }
    pub fn prepare_start(
        &self,
        store: &Store,
        field: SiteId,
        operation: FarmOperation,
        facts: BeginFacts,
    ) -> Result<PreparedCultivationStart, Error> {
        let (land, crop, soil, rules) = self.field_bundle(store, field)?;
        let effect = prepare_begin(land, crop, soil, rules, operation, facts)?;
        let mut reads = ReadSet::new();
        reads.read::<Field>(store, StableId(field.0));
        reads.read::<Crop>(store, StableId(field.0));
        reads.read::<Soil>(store, StableId(field.0));
        reads.read::<Cultivation>(store, StableId(facts.actor.0));
        reads.membership::<Cultivation>(store, move |_, job| job.field == field);
        Ok(PreparedCultivationStart {
            actor: facts.actor,
            definition: crop.definition.clone(),
            rules: rules.clone(),
            reads,
            effect,
        })
    }
    pub fn prepare_finish(
        &self,
        store: &Store,
        actor: PersonId,
        facts: FinishFacts,
    ) -> Result<Option<PreparedCultivationFinish>, Error> {
        let Some(job) = self.cultivation(store, actor) else {
            return Ok(None);
        };
        let (field, crop, soil, rules) = self.field_bundle(store, job.field)?;
        let mut reads = ReadSet::new();
        reads.read::<Cultivation>(store, StableId(actor.0));
        reads.read::<Field>(store, StableId(job.field.0));
        reads.read::<Crop>(store, StableId(job.field.0));
        reads.read::<Soil>(store, StableId(job.field.0));
        let (outcome, completion) = if !facts.equipped {
            (FinishOutcome::Interrupted, None)
        } else if !facts.alive {
            (FinishOutcome::Dead, None)
        } else {
            let access = facts.access.ok_or(Error::Unavailable)?;
            let completion = prepare_completion(
                field,
                crop,
                soil,
                job,
                rules,
                CompletionFacts {
                    now: facts.now,
                    eligible: true,
                    outputs_accessible: access.capable
                        && access.tool_available
                        && access.field_available
                        && field.access.contains(&actor)
                        && access.outputs_accessible,
                    food_room: access.food_room,
                    seed_room: access.seed_room,
                },
            )?;
            (
                FinishOutcome::Completed {
                    job: job.clone(),
                    harvest: completion.harvest,
                },
                Some(completion),
            )
        };
        Ok(Some(PreparedCultivationFinish {
            actor,
            field: job.field,
            definition: crop.definition.clone(),
            rules: rules.clone(),
            reads,
            outcome,
            completion,
        }))
    }
}
/// Absence of runtime is checked by the owner even when no contextual links exist.
pub fn validate_runtime_presence(store: &Store, runtime: Option<&Runtime>) -> Result<(), Error> {
    if runtime.is_none()
        && (store.iter::<Field>().next().is_some()
            || store.iter::<Crop>().next().is_some()
            || cultivations(store).next().is_some())
    {
        return Err(Error::InvalidSave(
            "agricultural components have no runtime".into(),
        ));
    }
    Ok(())
}
pub struct PreparedCultivationStart {
    actor: PersonId,
    definition: CropId,
    rules: CropRules,
    reads: ReadSet,
    effect: BeginEffect,
}
impl PreparedCultivationStart {
    pub fn effect(&self) -> &BeginEffect {
        &self.effect
    }
    pub fn stage(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<AgriculturePublication, Error> {
        if runtime.catalog.crops.get(&self.definition) != Some(&self.rules) {
            return Err(Error::Unavailable);
        }
        let field = StableId(self.effect.job.field.0);
        let actor = StableId(self.actor.0);
        let mut writes = ReadFootprint::new();
        writes.protect::<Crop>(field);
        writes.protect::<Cultivation>(actor);
        batch
            .insert(field, self.effect.crop)
            .insert(actor, self.effect.job);
        Ok(AgriculturePublication {
            guard: self.reads.bind(store, batch, &writes)?,
            definition: self.definition,
            rules: self.rules,
            default_crop: false,
        })
    }
}
#[derive(Clone, Copy, Debug)]
pub struct FinishFacts {
    pub now: SimTime,
    pub equipped: bool,
    pub alive: bool,
    pub access: Option<BeginFacts>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FinishOutcome {
    Interrupted,
    Dead,
    Completed {
        job: Cultivation,
        harvest: Option<HarvestOutput>,
    },
}
pub struct PreparedCultivationFinish {
    actor: PersonId,
    field: SiteId,
    definition: CropId,
    rules: CropRules,
    reads: ReadSet,
    outcome: FinishOutcome,
    completion: Option<CropCompletion>,
}
impl PreparedCultivationFinish {
    pub fn outcome(&self) -> &FinishOutcome {
        &self.outcome
    }
    pub fn stage(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<AgriculturePublication, Error> {
        if runtime.catalog.crops.get(&self.definition) != Some(&self.rules) {
            return Err(Error::Unavailable);
        }
        let field = StableId(self.field.0);
        let actor = StableId(self.actor.0);
        let mut writes = ReadFootprint::new();
        writes.protect::<Cultivation>(actor);
        batch.remove::<Cultivation>(actor);
        if let Some(completion) = self.completion {
            writes.protect::<Crop>(field);
            writes.protect::<Soil>(field);
            batch
                .insert(field, completion.crop)
                .insert(field, completion.soil);
        }
        Ok(AgriculturePublication {
            guard: self.reads.bind(store, batch, &writes)?,
            definition: self.definition,
            rules: self.rules,
            default_crop: false,
        })
    }
}

/// Agriculture's eligibility conjunction consumes factual capability inputs.
pub fn cultivation_capable(
    vitals: &Vitals,
    fatigue_limit: u64,
    independent: bool,
    combat_restricted: bool,
    caring: bool,
    stage: Option<LifeStage>,
) -> bool {
    vitals.alive
        && vitals.health > 0
        && vitals.fatigue < fatigue_limit
        && independent
        && !combat_restricted
        && !caring
        && stage.is_none_or(|stage| matches!(stage, LifeStage::Adult | LifeStage::Elder))
}
pub fn cultivation_tool_available(stock: Option<Quantity>, allocated: bool) -> bool {
    stock.is_some_and(|quantity| quantity.0 >= 1_000) && allocated
}
impl Runtime {
    pub fn field_available(&self, store: &Store, actor: PersonId, field: SiteId) -> bool {
        !self
            .cultivations(store)
            .any(|(person, job)| person != actor && job.field == field)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct StockFacts {
    pub good: Good,
    pub accessible: bool,
    pub area: Option<AreaId>,
    pub quantity: Quantity,
}
pub fn available_stock(stock: Option<StockFacts>, area: AreaId, good: Good) -> Option<Quantity> {
    stock
        .filter(|stock| stock.good == good && stock.accessible && stock.area == Some(area))
        .map(|stock| stock.quantity)
}

/// An accepted construction effect becomes a field while retaining Land's soil.
/// The requesting owner supplies physical stock references and access facts.
pub struct PreparedFieldEstablishment {
    definition: CropId,
    rules: CropRules,
    reads: ReadSet,
    field: Field,
    crop: Crop,
}
impl Runtime {
    pub fn prepare_establish_field(
        &self,
        store: &Store,
        field: Field,
    ) -> Result<PreparedFieldEstablishment, Error> {
        let id = StableId(field.id.0);
        if self.field(store, field.id).is_some() || self.crop(store, field.id).is_some() {
            return Err(Error::Unavailable);
        }
        crate::storage::validate_field(&field)?;
        self.soil(store, field.id)
            .ok_or(Error::Unavailable)?
            .validate()?;
        let crop = Crop::new(self.catalog.default_crop.clone());
        let rules = self
            .catalog
            .crops
            .get(&crop.definition)
            .ok_or(Error::Unavailable)?;
        crop.validate(rules)?;
        let mut reads = ReadSet::new();
        reads.read::<Field>(store, id);
        reads.read::<Crop>(store, id);
        reads.read::<Soil>(store, id);
        Ok(PreparedFieldEstablishment {
            definition: crop.definition.clone(),
            rules: rules.clone(),
            reads,
            field,
            crop,
        })
    }
}
impl PreparedFieldEstablishment {
    pub fn stage(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<AgriculturePublication, Error> {
        if runtime.catalog.default_crop != self.definition
            || runtime.catalog.crops.get(&self.definition) != Some(&self.rules)
        {
            return Err(Error::Unavailable);
        }
        let id = StableId(self.field.id.0);
        let mut writes = ReadFootprint::new();
        writes.protect::<Field>(id);
        writes.protect::<Crop>(id);
        batch.insert(id, self.field).insert(id, self.crop);
        Ok(AgriculturePublication {
            guard: self.reads.bind(store, batch, &writes)?,
            definition: self.definition,
            rules: self.rules,
            default_crop: true,
        })
    }
}

/// Owner finalization retains consulted catalog metadata alongside the shared
/// canonical-row guard. Validate after every owner has staged its effects.
pub struct AgriculturePublication {
    guard: ReadGuard,
    definition: CropId,
    rules: CropRules,
    default_crop: bool,
}
impl AgriculturePublication {
    pub fn validate(&self, runtime: &Runtime, store: &Store, batch: &Batch) -> Result<(), Error> {
        if runtime.catalog.crops.get(&self.definition) != Some(&self.rules)
            || self.default_crop && runtime.catalog.default_crop != self.definition
        {
            return Err(Error::Unavailable);
        }
        self.guard.validate(store, batch)
    }
}

impl Runtime {
    /// Remembered farm plans forecast the authored default crop. Established
    /// fields retain their own crop definitions and are queried separately.
    pub fn remembered_food_share(
        &self,
        fertility: Option<u16>,
        remembered_farm_plan: bool,
        numerator: u64,
        denominator: u64,
    ) -> Result<Option<u128>, Error> {
        let Some(fertility) = fertility.filter(|_| remembered_farm_plan) else {
            return Ok(None);
        };
        let crop = self
            .catalog
            .crops
            .get(&self.catalog.default_crop)
            .ok_or_else(|| Error::InvalidConfiguration("unknown default crop definition".into()))?;
        projected_food_share(crop, fertility, numerator, denominator).map(Some)
    }

    /// Direct local field observation uses the actor's raw placement. A carried
    /// or travelling actor does not acquire a stationary field observation.
    pub fn local_field(&self, store: &Store, site: SiteId, raw_location: &Location) -> bool {
        self.field(store, site)
            .is_some_and(|field| *raw_location == Location::AtArea(field.area))
    }
}

/// Forecast only an actually remembered plan with its remembered fertility.
/// Missing agriculture is unavailable precisely when those facts require it.
pub fn known_food_share(
    runtime: Option<&Runtime>,
    fertility: Option<u16>,
    remembered_plan: bool,
    numerator: u64,
    denominator: u64,
) -> Result<Option<u128>, Error> {
    let Some(fertility) = fertility.filter(|_| remembered_plan) else {
        return Ok(None);
    };
    runtime.ok_or(Error::Unavailable)?.remembered_food_share(
        Some(fertility),
        true,
        numerator,
        denominator,
    )
}
