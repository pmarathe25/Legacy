//! Clinical state and pending health work share the host's component store.
use crate::*;
use game_core::*;
use game_ecs::{Schema, StableId, Store};

impl Clinical {
    /// Clinical structure is independent of the clock, rules and references.
    pub fn validate(&self) -> Result<(), Error> {
        if self.cold_remainder >= 1_000
            || self.doses.values().any(|dose| dose.remainder >= 1_000)
            || self.illnesses.iter().any(|(kind, illness)| {
                *kind != illness.kind
                    || illness.acquired > illness.onset
                    || illness.onset >= illness.recovers
            })
            || self
                .treatment
                .as_ref()
                .is_some_and(|course| course.healing > 11_000)
            || self.last_drink.is_some_and(|(source, _)| source.0 == 0)
        {
            return Err(Error::InvalidSave("invalid clinical component".into()));
        }
        Ok(())
    }
}

impl Job {
    pub fn validate(&self) -> Result<(), Error> {
        let valid_work = match self.work {
            Work::Drink { source, quantity } => source.0 != 0 && quantity.0 != 0,
            Work::Boil {
                source,
                quantity,
                tool,
            } => source.0 != 0 && quantity.0 != 0 && tool.0 != 0,
            Work::Shelter {
                timber_used,
                tool,
                capacity,
                insulation,
                started,
            } => {
                timber_used.0 != 0
                    && tool.0 != 0
                    && (1..=64).contains(&capacity)
                    && (1..=1_000).contains(&insulation)
                    && started < self.completes
            }
            Work::Treat {
                patient,
                skill_ms,
                elapsed,
            } => patient.0 != 0 && skill_ms <= u128::from(elapsed) * u128::from(u16::MAX),
        };
        if self.actor.0 == 0 || self.area.0 == 0 || self.completes.0 == 0 || !valid_work {
            return Err(Error::InvalidSave("invalid health job component".into()));
        }
        Ok(())
    }
}

pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Clinical>("health/clinical", Clinical::validate)?;
    schema.register::<Job>("health/job", Job::validate)?;
    schema.register::<WaterLot>("health/water", WaterLot::validate_fields)?;
    schema.register::<Shelter>("health/shelter", Shelter::validate)?;
    schema.register::<Occupancy>("health/occupancy", Occupancy::validate)?;
    schema.register::<Knowledge>("health/knowledge", Knowledge::validate)?;
    schema.register::<Offers>("health/offers", Offers::validate)
}

impl Shelter {
    pub fn validate(&self) -> Result<(), Error> {
        if self.id.0 == 0
            || self.area.0 == 0
            || self.custodian.0 == 0
            || self.access.iter().any(|actor| actor.0 == 0)
            || !(1..=64).contains(&self.capacity)
            || !(1..=1_000).contains(&self.insulation)
            || self.timber_used.0 == 0
            || self.labor_ms == 0
        {
            return Err(Error::InvalidSave("invalid shelter component".into()));
        }
        Ok(())
    }
}
impl Occupancy {
    pub fn validate(&self) -> Result<(), Error> {
        if self.shelter.0 == 0 {
            return Err(Error::InvalidSave("invalid shelter occupancy".into()));
        }
        Ok(())
    }
}
impl Offer {
    pub fn validate(&self) -> Result<(), Error> {
        if self.caregiver.0 == 0
            || self.patient.0 == 0
            || self.caregiver == self.patient
            || self.herbs.0 == 0
            || self.water.0 == 0
            || self.expires.0 == 0
        {
            return Err(Error::InvalidSave("invalid treatment offer".into()));
        }
        Ok(())
    }
}
impl Offers {
    pub fn validate(&self) -> Result<(), Error> {
        if self.caregiver.0 == 0 {
            return Err(Error::InvalidSave("invalid treatment caregiver".into()));
        }
        for (patient, offer) in &self.patients {
            offer.validate()?;
            if *patient != offer.patient || self.caregiver != offer.caregiver {
                return Err(Error::InvalidSave("inconsistent treatment patient".into()));
            }
        }
        Ok(())
    }
}
impl Knowledge {
    pub fn validate(&self) -> Result<(), Error> {
        for (person, symptoms) in &self.people {
            if person.0 == 0 || *person != symptoms.person {
                return Err(Error::InvalidSave("invalid remembered patient".into()));
            }
        }
        for (id, shelter) in &self.shelters {
            if id.0 == 0
                || *id != shelter.id
                || shelter.area.0 == 0
                || shelter.custodian.0 == 0
                || !(1..=64).contains(&shelter.capacity)
                || !(1..=1_000).contains(&shelter.insulation)
                || shelter.known_occupants.iter().any(|actor| actor.0 == 0)
            {
                return Err(Error::InvalidSave("invalid remembered shelter".into()));
            }
        }
        for (id, safety) in &self.water {
            if id.0 == 0 || *id != safety.inventory {
                return Err(Error::InvalidSave("invalid remembered water".into()));
            }
        }
        Ok(())
    }
}

macro_rules! component_ports {
    ($get:ident,$get_mut:ident,$iter:ident,$insert:ident,$remove:ident,$component:ty,$id:ident,$validate:ident) => {
        pub fn $get(store: &Store, id: $id) -> Option<&$component> {
            store.get(StableId(id.0))
        }
        pub fn $get_mut(store: &mut Store, id: $id) -> Option<&mut $component> {
            store.get_mut(StableId(id.0))
        }
        pub fn $iter(store: &Store) -> impl Iterator<Item = ($id, &$component)> {
            store.iter::<$component>().map(|(id, row)| ($id(id.0), row))
        }
        pub fn $insert(store: &mut Store, id: $id, row: $component) -> Result<(), Error> {
            row.$validate()?;
            store.insert_or_spawn(StableId(id.0), row)
        }
        pub fn $remove(store: &mut Store, id: $id) -> Result<Option<$component>, Error> {
            if !store.contains(StableId(id.0)) {
                return Ok(None);
            }
            store.remove(StableId(id.0))
        }
    };
}
component_ports!(
    water_lot,
    water_lot_mut,
    water_lots,
    insert_water_lot,
    remove_water_lot,
    WaterLot,
    InventoryId,
    validate_fields
);
component_ports!(
    occupancy,
    occupancy_mut,
    occupancies,
    insert_occupancy,
    remove_occupancy,
    Occupancy,
    PersonId,
    validate
);
component_ports!(
    knowledge,
    knowledge_mut,
    knowledges,
    insert_knowledge,
    remove_knowledge,
    Knowledge,
    PersonId,
    validate
);
pub fn offers(store: &Store, caregiver: PersonId) -> Option<&Offers> {
    store.get(StableId(caregiver.0))
}
pub fn offers_mut(store: &mut Store, caregiver: PersonId) -> Option<&mut Offers> {
    store.get_mut(StableId(caregiver.0))
}
pub fn offer_rows(store: &Store) -> impl Iterator<Item = (PersonId, &Offers)> {
    store
        .iter::<Offers>()
        .map(|(id, row)| (PersonId(id.0), row))
}
pub fn insert_offers(store: &mut Store, caregiver: PersonId, row: Offers) -> Result<(), Error> {
    row.validate()?;
    if caregiver != row.caregiver {
        return Err(Error::InvalidSave("inconsistent caregiver entity".into()));
    }
    store.insert_or_spawn(StableId(caregiver.0), row)
}
pub fn remove_offers(store: &mut Store, caregiver: PersonId) -> Result<Option<Offers>, Error> {
    if !store.contains(StableId(caregiver.0)) {
        return Ok(None);
    }
    store.remove(StableId(caregiver.0))
}

pub fn shelter(store: &Store, id: ShelterId) -> Option<&Shelter> {
    store.get(StableId(id.0))
}
pub fn shelter_mut(store: &mut Store, id: ShelterId) -> Option<&mut Shelter> {
    store.get_mut(StableId(id.0))
}
pub fn shelters(store: &Store) -> impl Iterator<Item = (ShelterId, &Shelter)> {
    store
        .iter::<Shelter>()
        .map(|(id, row)| (ShelterId(id.0), row))
}
pub fn insert_shelter(store: &mut Store, shelter: Shelter) -> Result<(), Error> {
    shelter.validate()?;
    store.insert_or_spawn(StableId(shelter.id.0), shelter)
}
pub fn remove_shelter(store: &mut Store, id: ShelterId) -> Result<Option<Shelter>, Error> {
    if !store.contains(StableId(id.0)) {
        return Ok(None);
    }
    store.remove(StableId(id.0))
}
pub fn offer(store: &Store, caregiver: PersonId, patient: PersonId) -> Option<&Offer> {
    offers(store, caregiver)?.patients.get(&patient)
}
pub fn offer_mut(store: &mut Store, caregiver: PersonId, patient: PersonId) -> Option<&mut Offer> {
    offers_mut(store, caregiver)?.patients.get_mut(&patient)
}
pub fn all_offers(store: &Store) -> impl Iterator<Item = (PersonId, PersonId, &Offer)> {
    offer_rows(store).flat_map(|(giver, row)| {
        row.patients
            .iter()
            .map(move |(patient, offer)| (giver, *patient, offer))
    })
}
pub fn insert_offer(store: &mut Store, offer: Offer) -> Result<(), Error> {
    offer.validate()?;
    let mut row = offers(store, offer.caregiver)
        .cloned()
        .unwrap_or_else(|| Offers {
            caregiver: offer.caregiver,
            patients: Default::default(),
        });
    let caregiver = offer.caregiver;
    row.patients.insert(offer.patient, offer);
    insert_offers(store, caregiver, row)
}
pub fn remove_offer(
    store: &mut Store,
    caregiver: PersonId,
    patient: PersonId,
) -> Result<Option<Offer>, Error> {
    let Some(mut row) = offers(store, caregiver).cloned() else {
        return Ok(None);
    };
    let removed = row.patients.remove(&patient);
    if removed.is_some() {
        if row.patients.is_empty() {
            remove_offers(store, caregiver)?;
        } else {
            insert_offers(store, caregiver, row)?;
        }
    }
    Ok(removed)
}

pub fn clinical(store: &Store, actor: PersonId) -> Option<&Clinical> {
    store.get(StableId(actor.0))
}
pub fn clinical_mut(store: &mut Store, actor: PersonId) -> Option<&mut Clinical> {
    store.get_mut(StableId(actor.0))
}
pub fn clinicals(store: &Store) -> impl Iterator<Item = (PersonId, &Clinical)> {
    store
        .iter::<Clinical>()
        .map(|(id, row)| (PersonId(id.0), row))
}
pub fn insert_clinical(
    store: &mut Store,
    actor: PersonId,
    clinical: Clinical,
) -> Result<(), Error> {
    clinical.validate()?;
    store.insert_or_spawn(StableId(actor.0), clinical)
}
pub fn remove_clinical(store: &mut Store, actor: PersonId) -> Result<Option<Clinical>, Error> {
    if !store.contains(StableId(actor.0)) {
        return Ok(None);
    }
    store.remove(StableId(actor.0))
}
pub fn job(store: &Store, actor: PersonId) -> Option<&Job> {
    store.get(StableId(actor.0))
}
pub fn job_mut(store: &mut Store, actor: PersonId) -> Option<&mut Job> {
    store.get_mut(StableId(actor.0))
}
pub fn jobs(store: &Store) -> impl Iterator<Item = (PersonId, &Job)> {
    store.iter::<Job>().map(|(id, row)| (PersonId(id.0), row))
}
pub fn insert_job(store: &mut Store, job: Job) -> Result<(), Error> {
    job.validate()?;
    store.insert_or_spawn(StableId(job.actor.0), job)
}
pub fn remove_job(store: &mut Store, actor: PersonId) -> Result<Option<Job>, Error> {
    if !store.contains(StableId(actor.0)) {
        return Ok(None);
    }
    store.remove(StableId(actor.0))
}
