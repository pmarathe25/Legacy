use crate::*;
use game_core::*;
use game_ecs::{Batch, StableId, Store};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
pub struct ActorFacts {
    pub actor: PersonId,
    pub now: SimTime,
    pub area: Option<AreaId>,
    pub present: bool,
    pub capable: bool,
    pub at_area: bool,
    pub carried: bool,
    pub tactical: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct StockFacts {
    pub id: InventoryId,
    pub good: Good,
    pub quantity: Quantity,
    pub available: Quantity,
    pub usable: bool,
    pub tool_available: bool,
}
impl StockFacts {
    fn permits(&self, id: InventoryId, good: Good, quantity: Quantity) -> bool {
        self.id == id && self.good == good && self.usable && self.available >= quantity
    }
}
#[derive(Clone, Copy, Debug)]
pub struct PatientFacts {
    pub id: PersonId,
    pub present: bool,
    pub contact: bool,
    pub resting: bool,
    pub physiology: ClinicalPhysiology,
}
pub enum ActionFacts {
    Drink {
        water: StockFacts,
        physiology: ClinicalPhysiology,
    },
    Boil {
        water: StockFacts,
        fuel: StockFacts,
        tool: StockFacts,
    },
    ShelterBuild {
        timber: StockFacts,
        tool: StockFacts,
    },
    ShelterAccess {
        recipient_exists: bool,
    },
    ShelterOccupancy {
        leaving: Vec<PersonId>,
    },
    RequestCare {
        giver: ActorFacts,
        patient: PatientFacts,
        medicine: u16,
        supplies: Option<(StockFacts, StockFacts)>,
    },
    OfferCare {
        patient: PatientFacts,
        medicine: u16,
        herbs: StockFacts,
        water: StockFacts,
    },
    AcceptCare {
        giver_present: bool,
        contact: bool,
    },
    RefuseCare,
    Treat {
        patient: PatientFacts,
        medicine: u16,
        herbs: StockFacts,
        water: StockFacts,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HealthEffect {
    Interrupt {
        actor: PersonId,
    },
    ClearWork {
        actor: PersonId,
        completes: SimTime,
    },
    Start {
        actor: PersonId,
        kind: HealthWorkKind,
        completes: SimTime,
    },
    Rest {
        actor: PersonId,
    },
    Consume {
        actor: PersonId,
        source: InventoryId,
        quantity: Quantity,
    },
    Drank {
        actor: PersonId,
        source: InventoryId,
        quantity: Quantity,
    },
    Purified {
        actor: PersonId,
        source: InventoryId,
    },
    ShelterBuilt {
        shelter: ShelterId,
        builder: PersonId,
    },
    ShelterAccess {
        shelter: ShelterId,
        actor: PersonId,
        person: PersonId,
        allowed: bool,
    },
    ShelterEntered {
        shelter: ShelterId,
        actor: PersonId,
    },
    ShelterLeft {
        shelter: ShelterId,
        actor: PersonId,
    },
    Offered {
        caregiver: PersonId,
        patient: PersonId,
    },
    Accepted {
        caregiver: PersonId,
        patient: PersonId,
    },
    Refused {
        caregiver: PersonId,
        patient: PersonId,
    },
    Treated {
        caregiver: PersonId,
        patient: PersonId,
    },
}
fn related_jobs(store: &Store, actor: PersonId) -> BTreeMap<PersonId, Job> {
    jobs(store)
        .filter(|(_, job)| job.actor == actor || job.patient() == Some(actor))
        .map(|(id, job)| (id, job.clone()))
        .collect()
}

#[derive(Clone, Debug)]
pub struct PreparedHealth {
    rules: Rules,
    jobs: BTreeMap<PersonId, (Option<Job>, Option<Job>)>,
    offers: BTreeMap<(PersonId, PersonId), (Option<Offer>, Option<Offer>)>,
    clinical: BTreeMap<PersonId, (Option<Clinical>, Option<Clinical>)>,
    water: BTreeMap<InventoryId, (Option<WaterLot>, Option<WaterLot>)>,
    water_reads: BTreeMap<InventoryId, Option<WaterLot>>,
    shelters: BTreeMap<ShelterId, (Option<Shelter>, Option<Shelter>)>,
    occupancy: BTreeMap<PersonId, (Option<ShelterId>, Option<ShelterId>)>,
    water_knowledge: BTreeMap<PersonId, (Option<Knowledge>, Knowledge)>,
    job_reads: BTreeMap<PersonId, BTreeMap<PersonId, Job>>,
    occupancy_catalog: Option<BTreeMap<PersonId, ShelterId>>,
    effects: Vec<HealthEffect>,
}
impl PreparedHealth {
    fn new(world: &Runtime) -> Self {
        Self {
            rules: world.rules.clone(),
            jobs: BTreeMap::new(),
            offers: BTreeMap::new(),
            clinical: BTreeMap::new(),
            water: BTreeMap::new(),
            water_reads: BTreeMap::new(),
            shelters: BTreeMap::new(),
            occupancy: BTreeMap::new(),
            water_knowledge: BTreeMap::new(),
            job_reads: BTreeMap::new(),
            occupancy_catalog: None,
            effects: Vec::new(),
        }
    }
    pub fn effects(&self) -> &[HealthEffect] {
        &self.effects
    }
    fn set_job(&mut self, store: &Store, id: PersonId, after: Option<Job>) {
        self.jobs.insert(id, (job(store, id).cloned(), after));
    }
    fn set_offer(&mut self, world: &Runtime, key: (PersonId, PersonId), after: Option<Offer>) {
        self.offers
            .insert(key, (world.offers.get(&key).cloned(), after));
    }
    fn observe_jobs(&mut self, store: &Store, actor: PersonId) {
        self.job_reads
            .entry(actor)
            .or_insert_with(|| related_jobs(store, actor));
    }
    fn cancel(&mut self, world: &Runtime, store: &Store, actor: PersonId) {
        let related = related_jobs(store, actor);
        self.job_reads
            .entry(actor)
            .or_insert_with(|| related.clone());
        for (id, job) in related {
            self.set_job(store, id, None);
            self.effects.push(HealthEffect::ClearWork {
                actor: job.actor,
                completes: job.completes,
            });
        }
        for key in world
            .offers
            .keys()
            .filter(|(giver, patient)| *giver == actor || *patient == actor)
        {
            self.set_offer(world, *key, None);
        }
    }
    fn start(
        &mut self,
        world: &Runtime,
        store: &Store,
        actor: ActorFacts,
        kind: HealthWorkKind,
        duration: SimDuration,
        work: Work,
    ) -> Result<(), Error> {
        let completes = SimTime(actor.now.0.checked_add(duration.0).ok_or(Error::Overflow)?);
        self.cancel(world, store, actor.actor);
        self.effects
            .push(HealthEffect::Interrupt { actor: actor.actor });
        self.set_job(
            store,
            actor.actor,
            Some(Job {
                actor: actor.actor,
                area: actor.area.ok_or(Error::Unavailable)?,
                completes,
                work,
            }),
        );
        self.effects.push(HealthEffect::Start {
            actor: actor.actor,
            kind,
            completes,
        });
        Ok(())
    }
    fn receive(
        &mut self,
        world: &Runtime,
        store: &Store,
        patient: PersonId,
        physiology: ClinicalPhysiology,
        now: SimTime,
        intake: (StockFacts, Quantity),
    ) -> Result<(), Error> {
        let (source, quantity) = intake;
        let before = clinical(store, patient).cloned().unwrap_or_default();
        let lot = world
            .water
            .get(&source.id)
            .cloned()
            .unwrap_or_else(|| WaterLot::new(source.quantity.0, 0));
        self.water_reads
            .insert(source.id, world.water.get(&source.id).cloned());
        let dose = consume_water(&lot, quantity.0)?.consumed_load;
        let after = receive_water(
            &before,
            &world.rules,
            physiology,
            now,
            source.id,
            quantity,
            dose,
        )?;
        self.clinical
            .insert(patient, (clinical(store, patient).cloned(), Some(after)));
        Ok(())
    }
    fn leave(&mut self, world: &Runtime, actors: &[PersonId]) {
        for actor in actors {
            if let Some(shelter) = world.occupancy.get(actor) {
                self.occupancy.insert(*actor, (Some(*shelter), None));
                self.effects.push(HealthEffect::ShelterLeft {
                    shelter: *shelter,
                    actor: *actor,
                });
            }
        }
    }
    pub fn validate(&self, world: &Runtime, store: &Store) -> Result<(), Error> {
        if world.rules != self.rules
            || self
                .job_reads
                .iter()
                .any(|(actor, before)| *before != related_jobs(store, *actor))
            || self
                .occupancy_catalog
                .as_ref()
                .is_some_and(|before| before != &world.occupancy)
            || self
                .jobs
                .iter()
                .any(|(id, (before, _))| job(store, *id) != before.as_ref())
            || self
                .offers
                .iter()
                .any(|(id, (before, _))| world.offers.get(id) != before.as_ref())
            || self
                .clinical
                .iter()
                .any(|(id, (before, _))| clinical(store, *id) != before.as_ref())
            || self
                .water
                .iter()
                .any(|(id, (before, _))| world.water.get(id) != before.as_ref())
            || self
                .water_reads
                .iter()
                .any(|(id, before)| world.water.get(id) != before.as_ref())
            || self
                .shelters
                .iter()
                .any(|(id, (before, _))| world.shelters.get(id) != before.as_ref())
            || self
                .occupancy
                .iter()
                .any(|(id, (before, _))| world.occupancy.get(id) != before.as_ref())
            || self
                .water_knowledge
                .iter()
                .any(|(id, (before, _))| world.known.get(id) != before.as_ref())
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn commit(
        self,
        world: &mut Runtime,
        store: &mut Store,
    ) -> Result<Vec<HealthEffect>, Error> {
        self.validate(world, store)?;
        let mut batch = Batch::new();
        for (id, (_, after)) in &self.jobs {
            if let Some(value) = after {
                value.validate()?;
                batch.insert(StableId(id.0), value.clone());
            } else {
                batch.remove::<Job>(StableId(id.0));
            }
        }
        for (id, (_, after)) in &self.clinical {
            if let Some(value) = after {
                value.validate()?;
                batch.insert(StableId(id.0), value.clone());
            } else {
                batch.remove::<Clinical>(StableId(id.0));
            }
        }
        store.apply_batch(batch)?;
        macro_rules! install {
            ($patch:expr,$target:expr) => {
                for (id, (_, after)) in $patch {
                    if let Some(value) = after {
                        $target.insert(id, value);
                    } else {
                        $target.remove(&id);
                    }
                }
            };
        }
        install!(self.offers, world.offers);
        install!(self.water, world.water);
        install!(self.shelters, world.shelters);
        install!(self.occupancy, world.occupancy);
        for (id, (_, after)) in self.water_knowledge {
            world.known.insert(id, after);
        }
        Ok(self.effects)
    }
}
impl Runtime {
    pub fn prepare_cancel(&self, store: &Store, actor: PersonId) -> PreparedHealth {
        let mut change = PreparedHealth::new(self);
        change.cancel(self, store, actor);
        change
    }
    pub fn prepare_leave(&self, _store: &Store, actors: &[PersonId]) -> PreparedHealth {
        let mut change = PreparedHealth::new(self);
        change.leave(self, actors);
        change
    }
    pub fn patient_busy(&self, store: &Store, actor: PersonId) -> bool {
        jobs(store).any(|(_, job)| matches!(job.work,Work::Treat {patient,..} if patient==actor))
    }
    pub fn prepare_action(
        &self,
        store: &Store,
        actor: ActorFacts,
        action: &HealthAction,
        facts: ActionFacts,
    ) -> Result<PreparedHealth, Error> {
        validate_rules(&self.rules)?;
        let area = actor.area.ok_or(Error::Unavailable)?;
        let passive = matches!(
            action,
            HealthAction::AcceptTreatment { .. }
                | HealthAction::RefuseTreatment { .. }
                | HealthAction::RequestTreatment { .. }
        );
        if !actor.present || (!passive && !actor.capable) {
            return Err(Error::Unavailable);
        }
        if matches!(
            action,
            HealthAction::BuildShelter { .. }
                | HealthAction::ShelterAccess { .. }
                | HealthAction::EnterShelter { .. }
                | HealthAction::LeaveShelter
        ) && !actor.at_area
            || actor.tactical
                && matches!(
                    action,
                    HealthAction::BuildShelter { .. }
                        | HealthAction::EnterShelter { .. }
                        | HealthAction::LeaveShelter
                )
            || actor.carried
                && matches!(
                    action,
                    HealthAction::EnterShelter { .. } | HealthAction::LeaveShelter
                )
        {
            return Err(Error::Unavailable);
        }
        let r = &self.rules;
        let mut change = PreparedHealth::new(self);
        match (action, facts) {
            (HealthAction::Drink { source }, ActionFacts::Drink { water, .. }) => {
                if !water.permits(*source, Good::Water, r.drink_quantity) {
                    return Err(Error::Unavailable);
                }
                preflight_intake(r, actor.now)?;
                change.start(
                    self,
                    store,
                    actor,
                    HealthWorkKind::Drink,
                    r.drinking_time,
                    Work::Drink {
                        source: *source,
                        quantity: r.drink_quantity,
                    },
                )?;
            }
            (
                HealthAction::Boil { source, fuel, tool },
                ActionFacts::Boil {
                    water,
                    fuel: timber,
                    tool: kit,
                },
            ) => {
                if !water.permits(*source, Good::Water, r.drink_quantity)
                    || !timber.permits(*fuel, Good::Timber, r.boiling_fuel)
                    || !kit.permits(*tool, Good::Tool, Quantity(1_000))
                    || !kit.tool_available
                {
                    return Err(Error::Unavailable);
                }
                change.start(
                    self,
                    store,
                    actor,
                    HealthWorkKind::Boil,
                    r.boiling_time,
                    Work::Boil {
                        source: *source,
                        quantity: r.drink_quantity,
                        tool: *tool,
                    },
                )?;
                change.effects.push(HealthEffect::Consume {
                    actor: actor.actor,
                    source: *fuel,
                    quantity: r.boiling_fuel,
                });
            }
            (
                HealthAction::BuildShelter {
                    timber,
                    tool,
                    capacity,
                    insulation,
                },
                ActionFacts::ShelterBuild {
                    timber: stock,
                    tool: kit,
                },
            ) => {
                let quantity = shelter_cost(r, *capacity, *insulation)?;
                if !stock.permits(*timber, Good::Timber, quantity)
                    || !kit.permits(*tool, Good::Tool, Quantity(1_000))
                    || !kit.tool_available
                {
                    return Err(Error::Unavailable);
                }
                let duration = SimDuration(
                    r.shelter_time_per_place
                        .0
                        .checked_mul(u64::from(*capacity))
                        .ok_or(Error::Overflow)?,
                );
                change.start(
                    self,
                    store,
                    actor,
                    HealthWorkKind::Shelter,
                    duration,
                    Work::Shelter {
                        timber_used: quantity,
                        tool: *tool,
                        capacity: *capacity,
                        insulation: *insulation,
                        started: actor.now,
                    },
                )?;
                change.effects.push(HealthEffect::Consume {
                    actor: actor.actor,
                    source: *timber,
                    quantity,
                });
            }
            (
                HealthAction::ShelterAccess {
                    shelter,
                    person,
                    allowed,
                },
                ActionFacts::ShelterAccess { recipient_exists },
            ) => {
                let s = self.shelters.get(shelter).ok_or(Error::Unavailable)?;
                if s.custodian != actor.actor
                    || s.area != area
                    || !recipient_exists
                    || *person == actor.actor
                {
                    return Err(Error::Unavailable);
                }
                let mut after = s.clone();
                if *allowed {
                    after.access.insert(*person);
                } else {
                    after.access.remove(person);
                }
                change
                    .shelters
                    .insert(*shelter, (Some(s.clone()), Some(after)));
                change.effects.push(HealthEffect::ShelterAccess {
                    shelter: *shelter,
                    actor: actor.actor,
                    person: *person,
                    allowed: *allowed,
                });
            }
            (HealthAction::EnterShelter { shelter }, ActionFacts::ShelterOccupancy { leaving }) => {
                let s = self.shelters.get(shelter).ok_or(Error::Unavailable)?;
                if s.area != area
                    || !s.access.contains(&actor.actor)
                    || self
                        .occupancy
                        .values()
                        .filter(|id| **id == *shelter)
                        .count()
                        >= s.capacity as usize
                    || self.occupancy.get(&actor.actor) == Some(shelter)
                {
                    return Err(Error::Unavailable);
                }
                change.occupancy_catalog = Some(self.occupancy.clone());
                change.leave(self, &leaving);
                change.cancel(self, store, actor.actor);
                change.occupancy.insert(
                    actor.actor,
                    (self.occupancy.get(&actor.actor).copied(), Some(*shelter)),
                );
                change
                    .effects
                    .push(HealthEffect::Interrupt { actor: actor.actor });
                change
                    .effects
                    .push(HealthEffect::Rest { actor: actor.actor });
                change.effects.push(HealthEffect::ShelterEntered {
                    shelter: *shelter,
                    actor: actor.actor,
                });
            }
            (HealthAction::LeaveShelter, ActionFacts::ShelterOccupancy { leaving }) => {
                if !self.occupancy.contains_key(&actor.actor) {
                    return Err(Error::Unavailable);
                }
                change.leave(self, &leaving);
            }
            (
                HealthAction::RequestTreatment { caregiver },
                ActionFacts::RequestCare {
                    giver,
                    patient,
                    medicine,
                    supplies,
                },
            ) => {
                if *caregiver != giver.actor
                    || *caregiver == actor.actor
                    || !giver.capable
                    || !patient.contact
                {
                    return Err(Error::Unavailable);
                }
                if let Some((herbs, water)) = supplies {
                    return self.prepare_action(
                        store,
                        giver,
                        &HealthAction::OfferTreatment {
                            patient: actor.actor,
                            herbs: herbs.id,
                            water: water.id,
                        },
                        ActionFacts::OfferCare {
                            patient,
                            medicine,
                            herbs,
                            water,
                        },
                    );
                }
                change.effects.push(HealthEffect::Refused {
                    caregiver: *caregiver,
                    patient: actor.actor,
                });
            }
            (
                HealthAction::OfferTreatment {
                    patient,
                    herbs,
                    water,
                },
                ActionFacts::OfferCare {
                    patient: target,
                    medicine,
                    herbs: plants,
                    water: stock,
                },
            ) => {
                if *patient != target.id
                    || *patient == actor.actor
                    || !target.present
                    || !target.contact
                    || medicine == 0
                    || !plants.permits(*herbs, Good::Herb, r.treatment_herbs)
                    || !stock.permits(*water, Good::Water, r.treatment_water)
                {
                    return Err(Error::Unavailable);
                }
                let expires = SimTime(
                    actor
                        .now
                        .0
                        .checked_add(r.offer_duration.0)
                        .ok_or(Error::Overflow)?,
                );
                change.set_offer(
                    self,
                    (actor.actor, *patient),
                    Some(Offer {
                        caregiver: actor.actor,
                        patient: *patient,
                        herbs: *herbs,
                        water: *water,
                        expires,
                        accepted: false,
                    }),
                );
                change.effects.push(HealthEffect::Offered {
                    caregiver: actor.actor,
                    patient: *patient,
                });
            }
            (
                HealthAction::AcceptTreatment { caregiver },
                ActionFacts::AcceptCare {
                    giver_present,
                    contact,
                },
            ) => {
                let offer = self
                    .offers
                    .get(&(*caregiver, actor.actor))
                    .ok_or(Error::Unavailable)?;
                if offer.accepted || offer.expires <= actor.now || !giver_present || !contact {
                    return Err(Error::Unavailable);
                }
                change.cancel(self, store, actor.actor);
                change.set_offer(
                    self,
                    (*caregiver, actor.actor),
                    Some(Offer {
                        accepted: true,
                        ..offer.clone()
                    }),
                );
                change
                    .effects
                    .push(HealthEffect::Interrupt { actor: actor.actor });
                change
                    .effects
                    .push(HealthEffect::Rest { actor: actor.actor });
                change.effects.push(HealthEffect::Accepted {
                    caregiver: *caregiver,
                    patient: actor.actor,
                });
            }
            (HealthAction::RefuseTreatment { caregiver }, ActionFacts::RefuseCare) => {
                if !self.offers.contains_key(&(*caregiver, actor.actor)) {
                    return Err(Error::Unavailable);
                }
                change.cancel(self, store, actor.actor);
                change.effects.push(HealthEffect::Refused {
                    caregiver: *caregiver,
                    patient: actor.actor,
                });
            }
            (
                HealthAction::Treat {
                    patient,
                    herbs,
                    water,
                },
                ActionFacts::Treat {
                    patient: target,
                    medicine,
                    herbs: plants,
                    water: stock,
                },
            ) => {
                if *patient != target.id
                    || !target.present
                    || !target.contact
                    || medicine == 0
                    || self.patient_busy(store, *patient)
                    || !plants.permits(*herbs, Good::Herb, r.treatment_herbs)
                    || !stock.permits(*water, Good::Water, r.treatment_water)
                    || *patient != actor.actor
                        && (!target.resting
                            || !self.offers.get(&(actor.actor, *patient)).is_some_and(|o| {
                                o.accepted
                                    && o.herbs == *herbs
                                    && o.water == *water
                                    && o.expires > actor.now
                            }))
                {
                    return Err(Error::Unavailable);
                }
                actor
                    .now
                    .0
                    .checked_add(r.treatment_time.0)
                    .and_then(|t| t.checked_add(r.treatment_duration.0))
                    .ok_or(Error::Overflow)?;
                preflight_intake(r, actor.now)?;
                change.observe_jobs(store, *patient);
                change.start(
                    self,
                    store,
                    actor,
                    HealthWorkKind::Treat,
                    r.treatment_time,
                    Work::Treat {
                        patient: *patient,
                        skill_ms: 0,
                        elapsed: 0,
                    },
                )?;
                change.receive(
                    self,
                    store,
                    *patient,
                    target.physiology,
                    actor.now,
                    (stock, r.treatment_water),
                )?;
                change.effects.push(HealthEffect::Consume {
                    actor: actor.actor,
                    source: *herbs,
                    quantity: r.treatment_herbs,
                });
                change.effects.push(HealthEffect::Consume {
                    actor: *patient,
                    source: *water,
                    quantity: r.treatment_water,
                });
            }
            _ => return Err(Error::Unavailable),
        }
        Ok(change)
    }
}
pub fn preflight_intake(rules: &Rules, now: SimTime) -> Result<(), Error> {
    now.0
        .checked_add(rules.incubation.0)
        .and_then(|t| t.checked_add(rules.illness_duration.0))
        .ok_or(Error::Overflow)?;
    Ok(())
}
pub enum FinishFacts {
    Drink {
        water: StockFacts,
        physiology: ClinicalPhysiology,
    },
    Boil {
        water: StockFacts,
    },
    Shelter {
        id: ShelterId,
    },
    Treat {
        patient: PatientFacts,
    },
}
#[derive(Clone, Copy, Debug)]
pub struct ProgressFacts {
    pub area: Option<AreaId>,
    pub fatigue: u64,
    pub fatigue_limit: u64,
    pub tool_available: bool,
    pub patient_present: bool,
    pub patient_contact: bool,
    pub patient_resting: bool,
}
pub fn can_progress(job: &Job, facts: ProgressFacts) -> bool {
    if facts.area != Some(job.area)
        || !matches!(job.work, Work::Drink { .. }) && facts.fatigue >= facts.fatigue_limit
    {
        return false;
    }
    match job.work {
        Work::Drink { .. } => true,
        Work::Boil { .. } | Work::Shelter { .. } => facts.tool_available,
        Work::Treat { patient, .. } => {
            facts.patient_present
                && facts.patient_contact
                && (patient == job.actor || facts.patient_resting)
        }
    }
}
impl Job {
    pub fn required_tool(&self) -> Option<InventoryId> {
        match self.work {
            Work::Boil { tool, .. } | Work::Shelter { tool, .. } => Some(tool),
            _ => None,
        }
    }
    pub fn patient(&self) -> Option<PersonId> {
        match self.work {
            Work::Treat { patient, .. } => Some(patient),
            _ => None,
        }
    }
}
impl Runtime {
    pub fn prepare_finish(
        &self,
        store: &Store,
        actor: PersonId,
        now: SimTime,
        valid: bool,
        facts: FinishFacts,
    ) -> Result<PreparedHealth, Error> {
        let job = job(store, actor).ok_or(Error::Unavailable)?;
        if job.completes > now {
            return Err(Error::Unavailable);
        }
        let mut change = PreparedHealth::new(self);
        change.set_job(store, actor, None);
        change.effects.push(HealthEffect::ClearWork {
            actor,
            completes: job.completes,
        });
        if !valid {
            return Ok(change);
        }
        match (&job.work, facts) {
            (Work::Drink { source, quantity }, FinishFacts::Drink { water, physiology }) => {
                if !water.permits(*source, Good::Water, *quantity) {
                    return Ok(change);
                }
                change.receive(self, store, actor, physiology, now, (water, *quantity))?;
                change.effects.push(HealthEffect::Consume {
                    actor,
                    source: *source,
                    quantity: *quantity,
                });
                change.effects.push(HealthEffect::Drank {
                    actor,
                    source: *source,
                    quantity: *quantity,
                });
            }
            (
                Work::Boil {
                    source, quantity, ..
                },
                FinishFacts::Boil { water },
            ) => {
                if !water.permits(*source, Good::Water, *quantity) {
                    return Ok(change);
                }
                let lot = self
                    .water
                    .get(source)
                    .cloned()
                    .unwrap_or_else(|| WaterLot::new(water.quantity.0, 0));
                let result = purify_water(&lot, quantity.0)?;
                let load = result.remaining.load();
                change.water.insert(
                    *source,
                    (self.water.get(source).cloned(), Some(result.remaining)),
                );
                let mut knowledge = self.known.get(&actor).cloned().unwrap_or_default();
                knowledge.water.insert(
                    *source,
                    WaterSafetyView {
                        inventory: *source,
                        purified: load == 0,
                        suspected_unsafe: load > 0,
                        observed_at: now,
                    },
                );
                change
                    .water_knowledge
                    .insert(actor, (self.known.get(&actor).cloned(), knowledge));
                change.effects.push(HealthEffect::Purified {
                    actor,
                    source: *source,
                });
            }
            (
                Work::Shelter {
                    timber_used,
                    capacity,
                    insulation,
                    started,
                    ..
                },
                FinishFacts::Shelter { id },
            ) => {
                if id.0 == 0 || self.shelters.contains_key(&id) {
                    return Err(Error::Unavailable);
                }
                let shelter = Shelter {
                    id,
                    area: job.area,
                    custodian: actor,
                    access: BTreeSet::from([actor]),
                    capacity: *capacity,
                    insulation: *insulation,
                    built_at: now,
                    timber_used: *timber_used,
                    labor_ms: now.0.checked_sub(started.0).ok_or(Error::Overflow)?,
                };
                change.shelters.insert(id, (None, Some(shelter)));
                change.effects.push(HealthEffect::ShelterBuilt {
                    shelter: id,
                    builder: actor,
                });
            }
            (
                Work::Treat {
                    patient,
                    skill_ms,
                    elapsed,
                },
                FinishFacts::Treat { patient: target },
            ) => {
                if *patient != target.id
                    || !target.present
                    || !target.contact
                    || *patient != actor && !target.resting
                {
                    return Ok(change);
                }
                let mut after = clinical(store, *patient).cloned().unwrap_or_default();
                after.treatment = Some(treatment_course(&self.rules, now, *skill_ms, *elapsed)?);
                change
                    .clinical
                    .insert(*patient, (clinical(store, *patient).cloned(), Some(after)));
                change.effects.push(HealthEffect::Treated {
                    caregiver: actor,
                    patient: *patient,
                });
            }
            _ => return Err(Error::Unavailable),
        }
        Ok(change)
    }
    pub fn prepare_expire_offers(&self, now: SimTime) -> PreparedHealth {
        let mut change = PreparedHealth::new(self);
        for (key, offer) in &self.offers {
            if offer.expires <= now {
                change.set_offer(self, *key, None);
            }
        }
        change
    }
    pub fn reserved_stock(&self, store: &Store, source: InventoryId, except: PersonId) -> u64 {
        jobs(store)
            .filter(|(_, job)| job.actor != except)
            .filter_map(|(_, j)| match j.work {
                Work::Drink {
                    source: id,
                    quantity,
                }
                | Work::Boil {
                    source: id,
                    quantity,
                    ..
                } if id == source => Some(quantity.0),
                _ => None,
            })
            .fold(0, u64::saturating_add)
    }
    pub fn allocation_bound(&self, store: &Store, next: SimTime) -> u64 {
        jobs(store)
            .filter(|(_, job)| job.completes <= next && matches!(job.work, Work::Shelter { .. }))
            .count() as u64
    }
}
