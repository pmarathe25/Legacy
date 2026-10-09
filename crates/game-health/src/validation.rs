//! Validation against explicit reference catalogs, without consulting a world.
use crate::*;
use game_core::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HealthWorkLink {
    pub kind: HealthWorkKind,
    pub completes: SimTime,
}

#[derive(Clone, Debug)]
pub struct PersonCatalog {
    pub alive: bool,
    pub location: Location,
    pub health_work: Option<HealthWorkLink>,
}

#[derive(Clone, Copy, Debug)]
pub struct InventoryCatalog {
    pub good: Good,
    pub quantity: Quantity,
}

#[derive(Clone, Copy, Debug)]
pub struct ValidationCatalog<'a> {
    pub now: SimTime,
    pub people: &'a BTreeMap<PersonId, PersonCatalog>,
    pub areas: &'a BTreeSet<AreaId>,
    pub inventories: &'a BTreeMap<InventoryId, InventoryCatalog>,
}

fn invalid(message: &str) -> Error {
    Error::InvalidSave(format!("health: {message}"))
}

impl ValidationCatalog<'_> {
    fn person(&self, id: PersonId) -> Result<&PersonCatalog, Error> {
        self.people
            .get(&id)
            .ok_or_else(|| invalid("unknown person"))
    }
    fn area(&self, id: AreaId) -> Result<(), Error> {
        if self.areas.contains(&id) {
            Ok(())
        } else {
            Err(invalid("unknown area"))
        }
    }
    fn stock(&self, id: InventoryId, good: Good) -> Result<&InventoryCatalog, Error> {
        self.inventories
            .get(&id)
            .filter(|s| s.good == good)
            .ok_or_else(|| invalid("unknown or wrong-kind inventory"))
    }
    fn past(&self, time: SimTime) -> Result<(), Error> {
        if time <= self.now {
            Ok(())
        } else {
            Err(invalid("future observation or history"))
        }
    }
}

fn shelter_shape(capacity: u32, insulation: u16) -> Result<(), Error> {
    if !(1..=64).contains(&capacity) || !(1..=1_000).contains(&insulation) {
        return Err(invalid("invalid shelter dimensions"));
    }
    Ok(())
}

/// Check authoritative state and cached references. Remembered permissions,
/// symptoms, occupancy and water safety may be stale; they are not compared to
/// current facts. Physical occupancy does not imply access and can exceed
/// nominal capacity after an actual birth.
pub fn validate_world(world: &World, catalog: ValidationCatalog<'_>) -> Result<(), Error> {
    validate_rules(&world.rules).map_err(|_| invalid("invalid rules"))?;
    catalog.past(world.last_review)?;
    for (id, lot) in &world.water {
        lot.validate(catalog.stock(*id, Good::Water)?.quantity.0)?;
    }
    for (actor, clinical) in &world.clinical {
        let person = catalog.person(*actor)?;
        if clinical.cold_remainder >= 1_000 {
            return Err(invalid("invalid cold fraction"));
        }
        for dose in clinical.doses.values() {
            if dose.remainder >= 1_000 || dose.amount > world.rules.infection_threshold {
                return Err(invalid("invalid infection dose"));
            }
        }
        for (kind, infection) in &clinical.illnesses {
            if *kind != infection.kind
                || infection.acquired > infection.onset
                || infection.onset >= infection.recovers
                || (infection.symptomatic && infection.onset > catalog.now)
                || (person.alive
                    && (infection.recovers <= catalog.now
                        || infection.symptomatic != (infection.onset <= catalog.now)))
            {
                return Err(invalid("invalid infection timeline"));
            }
            catalog.past(infection.acquired)?;
        }
        if let Some(course) = &clinical.treatment
            && (course.healing > 11_000 || (person.alive && course.until <= catalog.now))
        {
            return Err(invalid("invalid treatment course"));
        }
        if let Some((source, at)) = clinical.last_drink {
            catalog.stock(source, Good::Water)?;
            catalog.past(at)?;
        }
    }
    for (id, shelter) in &world.shelters {
        if *id != shelter.id || shelter.timber_used.0 == 0 || shelter.labor_ms == 0 {
            return Err(invalid("invalid shelter history"));
        }
        shelter_shape(shelter.capacity, shelter.insulation)?;
        catalog.area(shelter.area)?;
        catalog.person(shelter.custodian)?;
        for actor in &shelter.access {
            catalog.person(*actor)?;
        }
        catalog.past(shelter.built_at)?;
    }
    for (actor, shelter) in &world.occupancy {
        let person = catalog.person(*actor)?;
        let shelter = world
            .shelters
            .get(shelter)
            .ok_or_else(|| invalid("unknown occupied shelter"))?;
        if !person.alive || person.location != Location::AtArea(shelter.area) {
            return Err(invalid("nonlocal or dead shelter occupant"));
        }
    }
    for ((caregiver, patient), offer) in &world.offers {
        if (*caregiver, *patient) != (offer.caregiver, offer.patient)
            || caregiver == patient
            || offer.expires <= catalog.now
        {
            return Err(invalid("invalid treatment offer"));
        }
        catalog.person(*caregiver)?;
        catalog.person(*patient)?;
        catalog.stock(offer.herbs, Good::Herb)?;
        catalog.stock(offer.water, Good::Water)?;
    }
    for (actor, job) in &world.jobs {
        let person = catalog.person(*actor)?;
        catalog.area(job.area)?;
        let kind = match &job.work {
            Work::Drink { source, quantity } => {
                catalog.stock(*source, Good::Water)?;
                if quantity.0 == 0 {
                    return Err(invalid("empty drinking job"));
                }
                HealthWorkKind::Drink
            }
            Work::Boil {
                source,
                quantity,
                tool,
            } => {
                catalog.stock(*source, Good::Water)?;
                catalog.stock(*tool, Good::Tool)?;
                if quantity.0 == 0 {
                    return Err(invalid("empty boiling job"));
                }
                HealthWorkKind::Boil
            }
            Work::Shelter {
                timber_used,
                tool,
                capacity,
                insulation,
                started,
            } => {
                catalog.stock(*tool, Good::Tool)?;
                shelter_shape(*capacity, *insulation)?;
                catalog.past(*started)?;
                if timber_used.0 == 0 {
                    return Err(invalid("empty shelter materials"));
                }
                HealthWorkKind::Shelter
            }
            Work::Treat {
                patient,
                skill_ms,
                elapsed,
            } => {
                catalog.person(*patient)?;
                if *elapsed > catalog.now.0
                    || *skill_ms > u128::from(*elapsed) * u128::from(u16::MAX)
                {
                    return Err(invalid("invalid medical practice history"));
                }
                HealthWorkKind::Treat
            }
        };
        if *actor != job.actor
            || !person.alive
            || job.completes <= catalog.now
            || person.location != Location::AtArea(job.area)
            || person.health_work
                != Some(HealthWorkLink {
                    kind,
                    completes: job.completes,
                })
        {
            return Err(invalid("inconsistent pending health work"));
        }
    }
    for (actor, person) in catalog.people {
        if person.health_work.is_some() && !world.jobs.contains_key(actor) {
            return Err(invalid("health activity without job"));
        }
    }
    for (observer, knowledge) in &world.known {
        catalog.person(*observer)?;
        for (id, symptoms) in &knowledge.people {
            catalog.person(*id)?;
            if *id != symptoms.person {
                return Err(invalid("inconsistent remembered person"));
            }
            catalog.past(symptoms.observed_at)?;
        }
        for (id, shelter) in &knowledge.shelters {
            if *id != shelter.id || !world.shelters.contains_key(id) {
                return Err(invalid("unknown remembered shelter"));
            }
            shelter_shape(shelter.capacity, shelter.insulation)?;
            catalog.area(shelter.area)?;
            catalog.person(shelter.custodian)?;
            for actor in &shelter.known_occupants {
                catalog.person(*actor)?;
            }
            catalog.past(shelter.observed_at)?;
        }
        for (id, safety) in &knowledge.water {
            catalog.stock(*id, Good::Water)?;
            if *id != safety.inventory {
                return Err(invalid("inconsistent remembered water"));
            }
            catalog.past(safety.observed_at)?;
        }
    }
    Ok(())
}
