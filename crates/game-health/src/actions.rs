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
    Clinical {
        actor: PersonId,
        event: ClinicalEvent,
    },
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
/// Temporary borrowed lookup for the actors touched by one preparation. Every
/// job and offer column is joined once, even when many actors are cancelled.
struct HealthScopeIndex<'a> {
    jobs: BTreeMap<PersonId, BTreeMap<PersonId, &'a Job>>,
    offers: BTreeMap<PersonId, BTreeMap<(PersonId, PersonId), &'a Offer>>,
}
impl<'a> HealthScopeIndex<'a> {
    fn new(store: &'a Store, actors: &BTreeSet<PersonId>) -> Self {
        let mut index = Self {
            jobs: BTreeMap::new(),
            offers: BTreeMap::new(),
        };
        if actors.is_empty() {
            return index;
        }
        for (id, row) in jobs(store) {
            if actors.contains(&row.actor) {
                index.jobs.entry(row.actor).or_default().insert(id, row);
            }
            if let Some(patient) = row
                .patient()
                .filter(|patient| *patient != row.actor && actors.contains(patient))
            {
                index.jobs.entry(patient).or_default().insert(id, row);
            }
        }
        for (giver, patient, row) in all_offers(store) {
            if actors.contains(&giver) {
                index
                    .offers
                    .entry(giver)
                    .or_default()
                    .insert((giver, patient), row);
            }
            if patient != giver && actors.contains(&patient) {
                index
                    .offers
                    .entry(patient)
                    .or_default()
                    .insert((giver, patient), row);
            }
        }
        index
    }
    fn related_jobs(&self, actor: PersonId) -> BTreeMap<PersonId, Job> {
        self.jobs
            .get(&actor)
            .into_iter()
            .flat_map(|rows| rows.iter())
            .map(|(id, row)| (*id, (*row).clone()))
            .collect()
    }
    fn related_offers(&self, actor: PersonId) -> OfferMembership {
        self.offers
            .get(&actor)
            .into_iter()
            .flat_map(|rows| rows.iter())
            .map(|(key, row)| (*key, (*row).clone()))
            .collect()
    }
    fn patient_busy(&self, actor: PersonId) -> bool {
        self.jobs
            .get(&actor)
            .is_some_and(|rows| rows.values().any(|row| row.patient() == Some(actor)))
    }
}

type OfferMembership = BTreeMap<(PersonId, PersonId), Offer>;

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
    knowledge_reads: BTreeMap<PersonId, Option<Knowledge>>,
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
            knowledge_reads: BTreeMap::new(),
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
    fn observe_jobs(&mut self, index: &HealthScopeIndex<'_>, actor: PersonId) {
        self.job_reads
            .entry(actor)
            .or_insert_with(|| index.related_jobs(actor));
    }
    fn cancel(
        &mut self,
        _world: &Runtime,
        store: &Store,
        index: &HealthScopeIndex<'_>,
        actor: PersonId,
    ) {
        let related = index.related_jobs(actor);
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
        let related = index.related_offers(actor);
        self.offer_reads
            .entry(actor)
            .or_insert_with(|| related.clone());
        for key in related.keys() {
            self.set_offer(store, *key, None);
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn start(
        &mut self,
        world: &Runtime,
        store: &Store,
        index: &HealthScopeIndex<'_>,
        actor: ActorFacts,
        kind: HealthWorkKind,
        duration: SimDuration,
        work: Work,
    ) -> Result<(), Error> {
        let completes = SimTime(actor.now.0.checked_add(duration.0).ok_or(Error::Overflow)?);
        self.cancel(world, store, index, actor.actor);
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
            || !self.memberships_match(store)
            || self
                .shelter_reads
                .iter()
                .any(|(id, before)| crate::shelter(store, *id) != before.as_ref())
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
            || self
                .knowledge_reads
                .iter()
                .any(|(id, before)| knowledge(store, *id) != before.as_ref())
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
        let (reads, admitted) = self.transaction_reads(store);
        let rules = self.rules.clone();
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
        let guard = reads.bind(store, batch, &admitted)?;
        Ok(HealthFinalization {
            effects: self.effects,
            rules,
            guard,
        })
    }
    pub fn commit(
        self,
        world: &mut Runtime,
        store: &mut Store,
    ) -> Result<Vec<HealthEffect>, Error> {
        let mut batch = Batch::new();
        let finish = self.stage_into(world, store, &mut batch)?;
        finish.validate_batch(world, store, &batch)?;
        store.apply_batch(batch)?;
        Ok(finish.finish())
    }
}
pub struct HealthFinalization {
    effects: Vec<HealthEffect>,
    rules: Rules,
    guard: game_ecs::ReadGuard,
}
impl HealthFinalization {
    pub fn validate_batch(
        &self,
        runtime: &Runtime,
        store: &Store,
        batch: &Batch,
    ) -> Result<(), Error> {
        if runtime.rules != self.rules {
            return Err(Error::Unavailable);
        }
        self.guard.validate(store, batch)
    }
    pub fn finish(self) -> Vec<HealthEffect> {
        self.effects
    }
}
pub struct PreparedHealthCancellations {
    change: PreparedHealth,
    groups: Vec<(PersonId, Vec<HealthEffect>)>,
}
/// Select clearing from cancellation effects and a factual receiving work stamp.
pub fn cancellation_activity_resets(
    effects: &[HealthEffect],
    mut activity: impl FnMut(PersonId) -> HealthActivityState,
) -> Vec<PersonId> {
    effects.iter().filter_map(|effect| match *effect {
        HealthEffect::ClearWork { actor, completes }
            if matches!(activity(actor), HealthActivityState::Working { completes: at } if at == completes) => Some(actor),
        _ => None,
    }).collect()
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
        let finish = self.change.stage_into(runtime, store, batch)?;
        Ok(HealthCancellationFinalization {
            groups: self.groups,
            finish,
        })
    }
    pub fn commit(
        self,
        runtime: &Runtime,
        store: &mut Store,
    ) -> Result<Vec<(PersonId, Vec<HealthEffect>)>, Error> {
        let mut batch = Batch::new();
        let finish = self.stage_into(runtime, store, &mut batch)?;
        finish.validate_batch(runtime, store, &batch)?;
        store.apply_batch(batch)?;
        Ok(finish.finish())
    }
}
pub struct HealthCancellationFinalization {
    groups: Vec<(PersonId, Vec<HealthEffect>)>,
    finish: HealthFinalization,
}
impl HealthCancellationFinalization {
    pub fn validate_batch(
        &self,
        runtime: &Runtime,
        store: &Store,
        batch: &Batch,
    ) -> Result<(), Error> {
        self.finish.validate_batch(runtime, store, batch)
    }
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
        let index = HealthScopeIndex::new(store, &actors.iter().copied().collect());
        let mut groups = Vec::new();
        let mut seen = BTreeSet::new();
        let mut cleared = BTreeSet::new();
        for actor in actors.iter().copied().filter(|actor| seen.insert(*actor)) {
            change.cancel(self, store, &index, actor);
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
        let index = HealthScopeIndex::new(store, &BTreeSet::from([actor]));
        change.cancel(self, store, &index, actor);
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
        let actors = match action {
            HealthAction::Treat { patient, .. } => BTreeSet::from([actor.actor, *patient]),
            HealthAction::Drink { .. }
            | HealthAction::Boil { .. }
            | HealthAction::BuildShelter { .. }
            | HealthAction::EnterShelter { .. }
            | HealthAction::AcceptTreatment { .. }
            | HealthAction::RefuseTreatment { .. } => BTreeSet::from([actor.actor]),
            _ => BTreeSet::new(),
        };
        let index = HealthScopeIndex::new(store, &actors);
        match (action, facts) {
            (HealthAction::Drink { source }, ActionFacts::Drink { water, .. }) => {
                if !water.permits(*source, Good::Water, r.drink_quantity) {
                    return Err(Error::Unavailable);
                }
                preflight_intake(r, actor.now)?;
                change.start(
                    self,
                    store,
                    &index,
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
                    &index,
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
                    &index,
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
                let members = shelter_members(store, *shelter);
                if s.area != area
                    || !s.access.contains(&actor.actor)
                    || members.len() >= s.capacity as usize
                    || occupancy(store, actor.actor).is_some_and(|row| row.shelter == *shelter)
                {
                    return Err(Error::Unavailable);
                }
                change.occupancy_reads.insert(*shelter, members);
                change.shelter_reads.insert(*shelter, Some(s.clone()));
                change.leave(store, &leaving);
                change.cancel(self, store, &index, actor.actor);
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
                change.cancel(self, store, &index, actor.actor);
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
                change.cancel(self, store, &index, actor.actor);
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
                    || index.patient_busy(*patient)
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
                change.observe_jobs(&index, *patient);
                change.start(
                    self,
                    store,
                    &index,
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
    /// Health owns which pending jobs claim physical water quantities.
    pub fn stock_reservations(
        &self,
        store: &Store,
        source: Option<InventoryId>,
    ) -> Vec<(PersonId, InventoryId, Quantity)> {
        jobs(store)
            .filter_map(|(_, job)| {
                let (inventory, quantity) = match job.work {
                    Work::Drink { source, quantity }
                    | Work::Boil {
                        source, quantity, ..
                    } => (source, quantity),
                    _ => return None,
                };
                source
                    .is_none_or(|wanted| inventory == wanted)
                    .then_some((job.actor, inventory, quantity))
            })
            .collect()
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

/// Opening physical quantities supplied by the material owner. Health owns
/// contamination conservation and the observer's subjective water memory.
#[derive(Clone, Copy, Debug)]
pub enum WaterReaction {
    Transfer {
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
        source_before: Quantity,
        destination_before: Quantity,
    },
    Consume {
        source: InventoryId,
        quantity: Quantity,
        source_before: Quantity,
    },
    Produce {
        actor: PersonId,
        output: InventoryId,
        quantity: Quantity,
        quantity_before: Quantity,
    },
    Spoil {
        inventory: InventoryId,
    },
}
#[derive(Clone, Copy, Debug)]
pub enum HealthReaction {
    Born {
        child: PersonId,
        inherited_shelter: Option<ShelterId>,
    },
    Died {
        actor: PersonId,
    },
    IllnessStarted {
        actor: PersonId,
        kind: IllnessKind,
    },
}
impl Runtime {
    pub fn prepare_water_reaction(
        &self,
        store: &Store,
        now: SimTime,
        reaction: WaterReaction,
    ) -> Result<PreparedHealth, Error> {
        let mut change = PreparedHealth::new(self);
        match reaction {
            WaterReaction::Transfer {
                actor,
                source,
                destination,
                quantity,
                source_before,
                destination_before,
            } => {
                if source == destination || quantity.0 == 0 {
                    return Err(Error::Unavailable);
                }
                let source_lot = water_lot(store, source)
                    .cloned()
                    .unwrap_or_else(|| WaterLot::new(source_before.0, 0));
                let destination_lot = water_lot(store, destination)
                    .cloned()
                    .unwrap_or_else(|| WaterLot::new(destination_before.0, 0));
                if source_lot.quantity() != source_before.0
                    || destination_lot.quantity() != destination_before.0
                {
                    return Err(Error::Unavailable);
                }
                let result = transfer_water(WaterTransferInput {
                    source: &source_lot,
                    destination: &destination_lot,
                    quantity: quantity.0,
                })?;
                change.water.insert(
                    source,
                    (water_lot(store, source).cloned(), Some(result.source)),
                );
                change.water.insert(
                    destination,
                    (
                        water_lot(store, destination).cloned(),
                        Some(result.destination),
                    ),
                );
                let before = knowledge(store, actor).cloned();
                if let Some(source_known) =
                    before.as_ref().and_then(|known| known.water.get(&source))
                {
                    let destination_known = before
                        .as_ref()
                        .and_then(|known| known.water.get(&destination));
                    let purified = source_known.purified
                        && (destination_before.0 == 0
                            || destination_known.is_some_and(|known| known.purified));
                    let suspected_unsafe = source_known.suspected_unsafe
                        || destination_known.is_some_and(|known| known.suspected_unsafe);
                    let mut after = before.clone().unwrap_or_default();
                    after.water.insert(
                        destination,
                        WaterSafetyView {
                            inventory: destination,
                            purified,
                            suspected_unsafe,
                            observed_at: now,
                        },
                    );
                    change.water_knowledge.insert(actor, (before, after));
                } else {
                    // Selection also depends on the absence of source memory.
                    change
                        .water_knowledge
                        .insert(actor, (before.clone(), before.unwrap_or_default()));
                }
            }
            WaterReaction::Consume {
                source,
                quantity,
                source_before,
            } => {
                let before = water_lot(store, source).cloned();
                let opening = before
                    .clone()
                    .unwrap_or_else(|| WaterLot::new(source_before.0, 0));
                if opening.quantity() != source_before.0 {
                    return Err(Error::Unavailable);
                }
                let result = consume_water(&opening, quantity.0)?;
                change
                    .water
                    .insert(source, (before, Some(result.remaining)));
            }
            WaterReaction::Produce {
                actor,
                output,
                quantity,
                quantity_before,
            } => {
                let before = water_lot(store, output).cloned();
                if before
                    .as_ref()
                    .is_some_and(|lot| lot.quantity() != quantity_before.0)
                {
                    return Err(Error::Unavailable);
                }
                let quantity_after = quantity_before
                    .0
                    .checked_add(quantity.0)
                    .ok_or(Error::Overflow)?;
                let after =
                    WaterLot::new(quantity_after, before.as_ref().map_or(0, WaterLot::load));
                after.validate_fields()?;
                change.water.insert(output, (before, Some(after)));
                let before = knowledge(store, actor).cloned();
                let mut after = before.clone().unwrap_or_default();
                after.water.remove(&output);
                change.water_knowledge.insert(actor, (before, after));
            }
            WaterReaction::Spoil { inventory } => {
                change.water.insert(
                    inventory,
                    (
                        water_lot(store, inventory).cloned(),
                        Some(WaterLot::new(0, 0)),
                    ),
                );
            }
        }
        Ok(change)
    }
    pub fn prepare_reaction(
        &self,
        store: &Store,
        now: SimTime,
        reaction: HealthReaction,
    ) -> Result<PreparedHealth, Error> {
        let mut change = PreparedHealth::new(self);
        match reaction {
            HealthReaction::Born {
                child,
                inherited_shelter,
            } => {
                if child.0 == 0 {
                    return Err(Error::Unavailable);
                }
                if let Some(id) = inherited_shelter {
                    let row = shelter(store, id).cloned().ok_or(Error::Unavailable)?;
                    change.shelter_reads.insert(id, Some(row));
                    change.occupancy.insert(
                        child,
                        (
                            occupancy(store, child).cloned(),
                            Some(Occupancy { shelter: id }),
                        ),
                    );
                }
                if clinical(store, child).is_none() {
                    change
                        .clinical
                        .insert(child, (None, Some(Clinical::default())));
                }
            }
            HealthReaction::Died { actor } => {
                change
                    .occupancy
                    .insert(actor, (occupancy(store, actor).cloned(), None));
            }
            HealthReaction::IllnessStarted { actor, kind } => {
                change.react_illness_started(store, &self.rules, now, actor, kind);
            }
        }
        Ok(change)
    }
    pub fn prepare_water_received(
        &self,
        store: &Store,
        patient: PersonId,
        source: StockFacts,
        quantity: Quantity,
        physiology: ClinicalPhysiology,
        now: SimTime,
    ) -> Result<PreparedHealth, Error> {
        if patient.0 == 0 || source.good != Good::Water || source.quantity < quantity {
            return Err(Error::Unavailable);
        }
        let mut change = PreparedHealth::new(self);
        change.receive(self, store, patient, physiology, now, (source, quantity))?;
        Ok(change)
    }
}
impl Runtime {
    pub fn birth_shelter(
        &self,
        parents: &[(PersonId, bool)],
        mut shelter: impl FnMut(PersonId) -> Option<ShelterId>,
    ) -> Option<ShelterId> {
        parents
            .iter()
            .find(|(_, gestates)| *gestates)
            .and_then(|(parent, _)| shelter(*parent))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FinishReadiness {
    pub area: Option<AreaId>,
    pub equipment_available: bool,
    pub active_deadline: Option<SimTime>,
}
pub fn finish_ready(job: &Job, facts: FinishReadiness) -> bool {
    facts.area == Some(job.area)
        && (!matches!(job.work, Work::Boil { .. } | Work::Shelter { .. })
            || facts.equipment_available)
        && facts.active_deadline == Some(job.completes)
}
impl Runtime {
    pub fn prepare_clinical_reconciliation(
        &self,
        store: &Store,
        now: SimTime,
        mut alive: impl FnMut(PersonId) -> bool,
    ) -> Result<PreparedHealth, Error> {
        let mut change = PreparedHealth::new(self);
        let actors = clinicals(store)
            .map(|(actor, state)| (actor, state, alive(actor)))
            .collect::<Vec<_>>();
        let dead = actors
            .iter()
            .filter(|(_, _, alive)| !alive)
            .map(|(actor, _, _)| *actor)
            .collect();
        let index = HealthScopeIndex::new(store, &dead);
        for (actor, state, alive) in actors {
            if !alive {
                change.cancel(self, store, &index, actor);
                continue;
            }
            let transition = reconcile_clinical(state, &self.rules, now, true)?;
            change
                .clinical
                .insert(actor, (Some(state.clone()), Some(transition.state)));
            for event in transition.events {
                if let ClinicalEvent::Started(kind) = event {
                    change.react_illness_started(store, &self.rules, now, actor, kind);
                }
                change.effects.push(HealthEffect::Clinical { actor, event });
            }
        }
        Ok(change)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum HealthActivityState {
    Other,
    Working { completes: SimTime },
}
#[derive(Clone, Copy, Debug)]
pub enum HealthActivityChange {
    Idle,
    Work {
        kind: HealthWorkKind,
        completes: SimTime,
    },
    Rest,
}
pub struct HealthOperations {
    pub interruptions: Vec<PersonId>,
    pub consumptions: Vec<(InventoryId, Quantity)>,
    pub activities: BTreeMap<PersonId, HealthActivityChange>,
}
impl PreparedHealth {
    /// Select receiving operations from owner effects. Adapters translate these
    /// activity requests into their public representation and order transactions.
    pub fn operations(
        &self,
        mut activity: impl FnMut(PersonId) -> HealthActivityState,
    ) -> HealthOperations {
        let mut result = HealthOperations {
            interruptions: Vec::new(),
            consumptions: Vec::new(),
            activities: BTreeMap::new(),
        };
        for effect in &self.effects {
            match *effect {
                HealthEffect::Interrupt { actor } => result.interruptions.push(actor),
                HealthEffect::Consume {
                    source, quantity, ..
                } => result.consumptions.push((source, quantity)),
                HealthEffect::ClearWork { actor, completes } => {
                    if matches!(activity(actor), HealthActivityState::Working { completes: deadline } if deadline == completes)
                    {
                        result.activities.insert(actor, HealthActivityChange::Idle);
                    }
                }
                HealthEffect::Start {
                    actor,
                    kind,
                    completes,
                } => {
                    result
                        .activities
                        .insert(actor, HealthActivityChange::Work { kind, completes });
                }
                HealthEffect::Rest { actor } => {
                    result.activities.insert(actor, HealthActivityChange::Rest);
                }
                _ => {}
            }
        }
        result
    }
    /// Consume contamination with the same owned transition that receives its
    /// clinical dose. Physical quantities are supplied at phase opening.
    pub fn with_consumption_reactions(
        mut self,
        store: &Store,
        mut source: impl FnMut(InventoryId) -> Result<(Good, Quantity), Error>,
    ) -> Result<Self, Error> {
        let mut quantities = BTreeMap::<InventoryId, u64>::new();
        for effect in &self.effects {
            if let HealthEffect::Consume {
                source, quantity, ..
            } = effect
            {
                let sum = quantities.entry(*source).or_default();
                *sum = sum.checked_add(quantity.0).ok_or(Error::Overflow)?;
            }
        }
        for (id, quantity) in quantities {
            let (good, opening_quantity) = source(id)?;
            if good != Good::Water {
                continue;
            }
            let before = water_lot(store, id).cloned();
            let opening = before
                .clone()
                .unwrap_or_else(|| WaterLot::new(opening_quantity.0, 0));
            if opening.quantity() != opening_quantity.0 {
                return Err(Error::Unavailable);
            }
            let after = consume_water(&opening, quantity)?.remaining;
            if self.water.contains_key(&id) {
                return Err(Error::Unavailable);
            }
            self.water.insert(id, (before, Some(after)));
        }
        Ok(self)
    }
}

impl PreparedHealth {
    /// Validate the union of selected scopes once per component column. Exact
    /// rows retain each member's identity, so moving inside the union is stale.
    fn memberships_match(&self, store: &Store) -> bool {
        if !self.job_reads.is_empty() {
            let actors = self.job_reads.keys().copied().collect::<BTreeSet<_>>();
            let expected = self
                .job_reads
                .values()
                .flat_map(|rows| rows.iter())
                .map(|(id, row)| (*id, row))
                .collect::<BTreeMap<_, _>>();
            let actual = jobs(store)
                .filter(|(_, row)| {
                    actors.contains(&row.actor)
                        || row
                            .patient()
                            .is_some_and(|patient| actors.contains(&patient))
                })
                .collect::<BTreeMap<_, _>>();
            if expected != actual {
                return false;
            }
        }
        if !self.occupancy_reads.is_empty() {
            let shelters = self
                .occupancy_reads
                .keys()
                .copied()
                .collect::<BTreeSet<_>>();
            let expected = self
                .occupancy_reads
                .values()
                .flat_map(|rows| rows.iter())
                .map(|(id, row)| (*id, row))
                .collect::<BTreeMap<_, _>>();
            let actual = occupancies(store)
                .filter(|(_, row)| shelters.contains(&row.shelter))
                .collect::<BTreeMap<_, _>>();
            if expected != actual {
                return false;
            }
        }
        if !self.offer_reads.is_empty() || self.expired_reads.is_some() {
            let actors = self.offer_reads.keys().copied().collect::<BTreeSet<_>>();
            let expires = self.expired_reads.as_ref().map(|(now, _)| *now);
            let expected = self
                .offer_reads
                .values()
                .flat_map(|rows| rows.iter())
                .chain(self.expired_reads.iter().flat_map(|(_, rows)| rows.iter()))
                .map(|(key, row)| (*key, row))
                .collect::<BTreeMap<_, _>>();
            let actual = all_offers(store)
                .filter(|(giver, patient, row)| {
                    actors.contains(giver)
                        || actors.contains(patient)
                        || expires.is_some_and(|now| row.expires <= now)
                })
                .map(|(giver, patient, row)| ((giver, patient), row))
                .collect::<BTreeMap<_, _>>();
            if expected != actual {
                return false;
            }
        }
        true
    }
    fn transaction_reads(&self, store: &Store) -> (game_ecs::ReadSet, game_ecs::ReadFootprint) {
        let mut reads = game_ecs::ReadSet::new();
        let mut admitted = game_ecs::ReadFootprint::new();
        macro_rules! rows {
            ($field:ident, $row:ty) => {
                for (id, (before, _)) in &self.$field {
                    reads.expect_row::<$row>(StableId(id.0), before.clone());
                    admitted.protect::<$row>(StableId(id.0));
                }
            };
        }
        rows!(jobs, Job);
        rows!(offers, Offers);
        rows!(clinical, Clinical);
        rows!(water, WaterLot);
        rows!(shelters, Shelter);
        rows!(occupancy, Occupancy);
        rows!(water_knowledge, Knowledge);
        for (id, before) in &self.water_reads {
            reads.expect_row::<WaterLot>(StableId(id.0), before.clone());
        }
        for (id, before) in &self.knowledge_reads {
            if !self.water_knowledge.contains_key(id) {
                reads.expect_row::<Knowledge>(StableId(id.0), before.clone());
            }
        }
        for (id, before) in &self.shelter_reads {
            reads.expect_row::<Shelter>(StableId(id.0), before.clone());
        }
        if !self.job_reads.is_empty() {
            let actors = self.job_reads.keys().copied().collect::<BTreeSet<_>>();
            reads.membership::<Job>(store, move |_, job| {
                actors.contains(&job.actor)
                    || job
                        .patient()
                        .is_some_and(|patient| actors.contains(&patient))
            });
            let rows = self
                .job_reads
                .values()
                .flat_map(|rows| rows.iter())
                .collect::<BTreeMap<_, _>>();
            for (id, row) in rows {
                if !self.jobs.contains_key(id) {
                    reads.expect_row::<Job>(StableId(id.0), Some(row.clone()));
                }
            }
        }
        if !self.occupancy_reads.is_empty() {
            let shelters = self
                .occupancy_reads
                .keys()
                .copied()
                .collect::<BTreeSet<_>>();
            reads.membership::<Occupancy>(store, move |_, row| shelters.contains(&row.shelter));
            let rows = self
                .occupancy_reads
                .values()
                .flat_map(|rows| rows.iter())
                .collect::<BTreeMap<_, _>>();
            for (actor, row) in rows {
                if !self.occupancy.contains_key(actor) {
                    reads.expect_row::<Occupancy>(StableId(actor.0), Some(row.clone()));
                }
            }
        }
        if !self.offer_reads.is_empty() || self.expired_reads.is_some() {
            let actors = self.offer_reads.keys().copied().collect::<BTreeSet<_>>();
            let expires = self.expired_reads.as_ref().map(|(now, _)| *now);
            let ids = reads.membership::<Offers>(store, move |_, row| {
                actors.contains(&row.caregiver)
                    || row.patients.values().any(|offer| {
                        actors.contains(&offer.patient)
                            || expires.is_some_and(|now| offer.expires <= now)
                    })
            });
            for id in ids {
                if !self.offers.contains_key(&PersonId(id.0)) {
                    let _ = reads.read::<Offers>(store, id);
                }
            }
        }
        (reads, admitted)
    }
}

impl PreparedHealth {
    /// Couple an intake's opening dose with its physical water debit so both
    /// belong to one Health plan and one final batch guard.
    pub fn with_water_consumption(
        mut self,
        store: &Store,
        source: InventoryId,
        quantity: Quantity,
        source_before: Quantity,
    ) -> Result<Self, Error> {
        let before = water_lot(store, source).cloned();
        let opening = before
            .clone()
            .unwrap_or_else(|| WaterLot::new(source_before.0, 0));
        if opening.quantity() != source_before.0 || self.water.contains_key(&source) {
            return Err(Error::Unavailable);
        }
        let after = consume_water(&opening, quantity.0)?.remaining;
        self.water.insert(source, (before, Some(after)));
        Ok(self)
    }
}

#[cfg(test)]
#[path = "scope_tests.rs"]
mod scope_tests;

impl Runtime {
    /// Prepare the recipient's clinical intake and the source's contaminant
    /// debit together, so a Cargo debit can share their one Health final guard.
    pub fn prepare_care_water_received(
        &self,
        store: &Store,
        patient: PersonId,
        source: StockFacts,
        quantity: Quantity,
        physiology: ClinicalPhysiology,
        now: SimTime,
    ) -> Result<PreparedHealth, Error> {
        self.prepare_water_received(store, patient, source, quantity, physiology, now)?
            .with_water_consumption(store, source.id, quantity, source.quantity)
    }
}

impl PreparedHealth {
    /// Enteric symptoms may make recent personally consumed water suspect.
    /// Resolve against this plan's projected Clinical and Knowledge rows so the
    /// origin transition and receiving memory reaction publish together.
    fn react_illness_started(
        &mut self,
        store: &Store,
        rules: &Rules,
        now: SimTime,
        actor: PersonId,
        kind: IllnessKind,
    ) {
        if kind != IllnessKind::Enteric {
            return;
        }
        let opening = clinical(store, actor).cloned();
        let projected = self
            .clinical
            .entry(actor)
            .or_insert_with(|| (opening.clone(), opening));
        let Some((source, drank)) = projected.1.as_ref().and_then(|row| row.last_drink) else {
            return;
        };
        if now.0.saturating_sub(drank.0) > rules.incubation.0 {
            return;
        }
        let before = knowledge(store, actor).cloned();
        self.knowledge_reads
            .entry(actor)
            .or_insert_with(|| before.clone());
        let mut after = self
            .water_knowledge
            .get(&actor)
            .map(|(_, after)| after.clone())
            .or_else(|| before.clone())
            .unwrap_or_default();
        if after.water.get(&source).is_some_and(|known| known.purified) {
            return;
        }
        after.water.insert(
            source,
            WaterSafetyView {
                inventory: source,
                purified: false,
                suspected_unsafe: true,
                observed_at: now,
            },
        );
        self.water_knowledge.insert(actor, (before, after));
    }
}
