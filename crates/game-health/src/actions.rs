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

type OfferMembership = BTreeMap<(PersonId, PersonId), Offer>;

fn related_offers(store: &Store, actor: PersonId) -> OfferMembership {
    all_offers(store)
        .filter(|(giver, patient, _)| *giver == actor || *patient == actor)
        .map(|(giver, patient, offer)| ((giver, patient), offer.clone()))
        .collect()
}
fn expired_offers(store: &Store, now: SimTime) -> OfferMembership {
    all_offers(store)
        .filter(|(_, _, offer)| offer.expires <= now)
        .map(|(giver, patient, offer)| ((giver, patient), offer.clone()))
        .collect()
}
fn shelter_members(store: &Store, shelter: ShelterId) -> BTreeMap<PersonId, Occupancy> {
    occupancies(store)
        .filter(|(_, row)| row.shelter == shelter)
        .map(|(actor, row)| (actor, row.clone()))
        .collect()
}

#[derive(Clone, Debug)]
pub struct PreparedHealth {
    rules: Rules,
    jobs: BTreeMap<PersonId, (Option<Job>, Option<Job>)>,
    offers: BTreeMap<PersonId, (Option<Offers>, Option<Offers>)>,
    clinical: BTreeMap<PersonId, (Option<Clinical>, Option<Clinical>)>,
    water: BTreeMap<InventoryId, (Option<WaterLot>, Option<WaterLot>)>,
    water_reads: BTreeMap<InventoryId, Option<WaterLot>>,
    shelters: BTreeMap<ShelterId, (Option<Shelter>, Option<Shelter>)>,
    occupancy: BTreeMap<PersonId, (Option<Occupancy>, Option<Occupancy>)>,
    water_knowledge: BTreeMap<PersonId, (Option<Knowledge>, Knowledge)>,
    job_reads: BTreeMap<PersonId, BTreeMap<PersonId, Job>>,
    occupancy_reads: BTreeMap<ShelterId, BTreeMap<PersonId, Occupancy>>,
    shelter_reads: BTreeMap<ShelterId, Option<Shelter>>,
    offer_reads: BTreeMap<PersonId, OfferMembership>,
    expired_reads: Option<(SimTime, OfferMembership)>,
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
            occupancy_reads: BTreeMap::new(),
            shelter_reads: BTreeMap::new(),
            offer_reads: BTreeMap::new(),
            expired_reads: None,
            effects: Vec::new(),
        }
    }
    pub fn effects(&self) -> &[HealthEffect] {
        &self.effects
    }
    fn set_job(&mut self, store: &Store, id: PersonId, after: Option<Job>) {
        self.jobs.insert(id, (job(store, id).cloned(), after));
    }
    fn set_offer(&mut self, store: &Store, key: (PersonId, PersonId), after: Option<Offer>) {
        let patch = self.offers.entry(key.0).or_insert_with(|| {
            let before = offers(store, key.0).cloned();
            (before.clone(), before)
        });
        let mut row = patch.1.take().unwrap_or_else(|| Offers {
            caregiver: key.0,
            patients: BTreeMap::new(),
        });
        if let Some(offer) = after {
            row.patients.insert(key.1, offer);
        } else {
            row.patients.remove(&key.1);
        }
        patch.1 = (!row.patients.is_empty()).then_some(row);
    }
    fn observe_jobs(&mut self, store: &Store, actor: PersonId) {
        self.job_reads
            .entry(actor)
            .or_insert_with(|| related_jobs(store, actor));
    }
    fn cancel(&mut self, _world: &Runtime, store: &Store, actor: PersonId) {
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
        let related = related_offers(store, actor);
        self.offer_reads
            .entry(actor)
            .or_insert_with(|| related.clone());
        for key in related.keys() {
            self.set_offer(store, *key, None);
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
        let lot = water_lot(store, source.id)
            .cloned()
            .unwrap_or_else(|| WaterLot::new(source.quantity.0, 0));
        self.water_reads
            .insert(source.id, water_lot(store, source.id).cloned());
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
    fn leave(&mut self, store: &Store, actors: &[PersonId]) {
        for actor in actors {
            if let Some(occupancy) = occupancy(store, *actor) {
                self.occupancy
                    .insert(*actor, (Some(occupancy.clone()), None));
                self.effects.push(HealthEffect::ShelterLeft {
                    shelter: occupancy.shelter,
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
                .occupancy_reads
                .iter()
                .any(|(shelter, before)| *before != shelter_members(store, *shelter))
            || self
                .shelter_reads
                .iter()
                .any(|(id, before)| crate::shelter(store, *id) != before.as_ref())
            || self
                .offer_reads
                .iter()
                .any(|(actor, before)| *before != related_offers(store, *actor))
            || self
                .expired_reads
                .as_ref()
                .is_some_and(|(now, before)| *before != expired_offers(store, *now))
            || self
                .jobs
                .iter()
                .any(|(id, (before, _))| job(store, *id) != before.as_ref())
            || self
                .offers
                .iter()
                .any(|(id, (before, _))| offers(store, *id) != before.as_ref())
            || self
                .clinical
                .iter()
                .any(|(id, (before, _))| clinical(store, *id) != before.as_ref())
            || self
                .water
                .iter()
                .any(|(id, (before, _))| water_lot(store, *id) != before.as_ref())
            || self
                .water_reads
                .iter()
                .any(|(id, before)| water_lot(store, *id) != before.as_ref())
            || self
                .shelters
                .iter()
                .any(|(id, (before, _))| shelter(store, *id) != before.as_ref())
            || self
                .occupancy
                .iter()
                .any(|(id, (before, _))| occupancy(store, *id) != before.as_ref())
            || self
                .water_knowledge
                .iter()
                .any(|(id, (before, _))| knowledge(store, *id) != before.as_ref())
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    /// Guard and stage only health-owned sparse rows. The caller discards the
    /// transient batch on failure, and publishes finalization after joint apply.
    pub fn stage_into(
        self,
        world: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<HealthFinalization, Error> {
        self.validate(world, store)?;
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
        macro_rules! apply_rows {
            ($field:ident, $row:ty, $validate:path) => {
                for (id, (_, after)) in &self.$field {
                    if let Some(value) = after {
                        $validate(value)?;
                        batch.insert(StableId(id.0), value.clone());
                    } else {
                        batch.remove::<$row>(StableId(id.0));
                    }
                }
            };
        }
        apply_rows!(offers, Offers, Offers::validate);
        apply_rows!(water, WaterLot, WaterLot::validate_fields);
        apply_rows!(shelters, Shelter, Shelter::validate);
        apply_rows!(occupancy, Occupancy, Occupancy::validate);
        for (id, (_, after)) in self.water_knowledge {
            after.validate()?;
            batch.insert(StableId(id.0), after);
        }
        Ok(HealthFinalization {
            effects: self.effects,
        })
    }
    pub fn commit(
        self,
        world: &mut Runtime,
        store: &mut Store,
    ) -> Result<Vec<HealthEffect>, Error> {
        let mut batch = Batch::new();
        let finish = self.stage_into(world, store, &mut batch)?;
        store.apply_batch(batch)?;
        Ok(finish.finish())
    }
}
pub struct HealthFinalization {
    effects: Vec<HealthEffect>,
}
impl HealthFinalization {
    pub fn finish(self) -> Vec<HealthEffect> {
        self.effects
    }
}
pub struct PreparedHealthCancellations {
    change: PreparedHealth,
    groups: Vec<(PersonId, Vec<HealthEffect>)>,
}
impl PreparedHealthCancellations {
    pub fn effects(&self) -> impl Iterator<Item = (PersonId, &[HealthEffect])> {
        self.groups
            .iter()
            .map(|(actor, effects)| (*actor, effects.as_slice()))
    }
    pub fn validate(&self, runtime: &Runtime, store: &Store) -> Result<(), Error> {
        self.change.validate(runtime, store)
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<HealthCancellationFinalization, Error> {
        self.change.stage_into(runtime, store, batch)?;
        Ok(HealthCancellationFinalization {
            groups: self.groups,
        })
    }
    pub fn commit(
        self,
        runtime: &Runtime,
        store: &mut Store,
    ) -> Result<Vec<(PersonId, Vec<HealthEffect>)>, Error> {
        let mut batch = Batch::new();
        let finish = self.stage_into(runtime, store, &mut batch)?;
        store.apply_batch(batch)?;
        Ok(finish.finish())
    }
}
pub struct HealthCancellationFinalization {
    groups: Vec<(PersonId, Vec<HealthEffect>)>,
}
impl HealthCancellationFinalization {
    pub fn finish(self) -> Vec<(PersonId, Vec<HealthEffect>)> {
        self.groups
    }
}
impl Runtime {
    pub fn prepare_cancel_many(
        &self,
        store: &Store,
        actors: &[PersonId],
    ) -> PreparedHealthCancellations {
        let mut change = PreparedHealth::new(self);
        let mut groups = Vec::new();
        let mut seen = BTreeSet::new();
        let mut cleared = BTreeSet::new();
        for actor in actors.iter().copied().filter(|actor| seen.insert(*actor)) {
            change.cancel(self, store, actor);
            let effects = std::mem::take(&mut change.effects)
                .into_iter()
                .filter(|effect| match effect {
                    HealthEffect::ClearWork { actor, completes } => {
                        cleared.insert((*actor, *completes))
                    }
                    _ => true,
                })
                .collect();
            groups.push((actor, effects));
        }
        PreparedHealthCancellations { change, groups }
    }
    pub fn prepare_cancel(&self, store: &Store, actor: PersonId) -> PreparedHealth {
        let mut change = PreparedHealth::new(self);
        change.cancel(self, store, actor);
        change
    }
    pub fn prepare_leave(&self, store: &Store, actors: &[PersonId]) -> PreparedHealth {
        let mut change = PreparedHealth::new(self);
        change.leave(store, actors);
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
                let s = crate::shelter(store, *shelter).ok_or(Error::Unavailable)?;
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
                let s = crate::shelter(store, *shelter).ok_or(Error::Unavailable)?;
                if s.area != area
                    || !s.access.contains(&actor.actor)
                    || shelter_members(store, *shelter).len() >= s.capacity as usize
                    || occupancy(store, actor.actor).is_some_and(|row| row.shelter == *shelter)
                {
                    return Err(Error::Unavailable);
                }
                change
                    .occupancy_reads
                    .insert(*shelter, shelter_members(store, *shelter));
                change.shelter_reads.insert(*shelter, Some(s.clone()));
                change.leave(store, &leaving);
                change.cancel(self, store, actor.actor);
                change.occupancy.insert(
                    actor.actor,
                    (
                        occupancy(store, actor.actor).cloned(),
                        Some(Occupancy { shelter: *shelter }),
                    ),
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
                if occupancy(store, actor.actor).is_none() {
                    return Err(Error::Unavailable);
                }
                change.leave(store, &leaving);
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
                    store,
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
                let offer = offer(store, *caregiver, actor.actor).ok_or(Error::Unavailable)?;
                if offer.accepted || offer.expires <= actor.now || !giver_present || !contact {
                    return Err(Error::Unavailable);
                }
                change.cancel(self, store, actor.actor);
                change.set_offer(
                    store,
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
                if offer(store, *caregiver, actor.actor).is_none() {
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
                            || !offer(store, actor.actor, *patient).is_some_and(|o| {
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
                let lot = water_lot(store, *source)
                    .cloned()
                    .unwrap_or_else(|| WaterLot::new(water.quantity.0, 0));
                let result = purify_water(&lot, quantity.0)?;
                let load = result.remaining.load();
                change.water.insert(
                    *source,
                    (water_lot(store, *source).cloned(), Some(result.remaining)),
                );
                let mut knowledge = crate::knowledge(store, actor).cloned().unwrap_or_default();
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
                    .insert(actor, (crate::knowledge(store, actor).cloned(), knowledge));
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
                if id.0 == 0 || shelter(store, id).is_some() {
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
    pub fn prepare_expire_offers(&self, store: &Store, now: SimTime) -> PreparedHealth {
        let mut change = PreparedHealth::new(self);
        let expired = expired_offers(store, now);
        change.expired_reads = Some((now, expired.clone()));
        for key in expired.keys() {
            change.set_offer(store, *key, None);
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

#[cfg(test)]
#[path = "action_staging_tests.rs"]
mod staging_tests;

#[path = "placement.rs"]
mod placement;
pub use placement::*;
