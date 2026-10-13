//! Saved canonical joins supplied as facts by the owning features.
use crate::*;
use game_core::*;
use game_ecs::Store;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
pub struct PersonReference {
    pub person: PersonId,
    pub born: SimTime,
    pub initial_age: SimDuration,
    pub parent_count: usize,
    pub alive: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct StockReference {
    pub good: Good,
    pub quantity: Quantity,
    pub holder: Option<PersonId>,
}
/// Read-only factual ports. Lineage owns chronology, succession eligibility,
/// guardian state, delivery provenance bounds, and assignment validation.
pub trait LineageReferences {
    fn people(&self) -> impl Iterator<Item = PersonReference>;
    fn guardian_people(&self) -> impl Iterator<Item = PersonId>;
    fn recorded_death(&self, person: PersonId, at: SimTime) -> bool;
    fn stock(&self, id: InventoryId) -> Option<StockReference>;
    fn area_exists(&self, id: AreaId) -> bool;
    fn site_exists(&self, id: SiteId) -> bool;
}

impl Runtime {
    pub fn validate_references(
        &self,
        store: &Store,
        now: SimTime,
        next_event: u64,
        ruler: PersonId,
        facts: &impl LineageReferences,
    ) -> Result<(), Error> {
        self.validate(store, now)?;
        let fail = || Error::InvalidSave("inconsistent lineage references".into());
        if next_event == 0 {
            return Err(fail());
        }
        let people: BTreeMap<_, _> = facts
            .people()
            .map(|person| (person.person, person))
            .collect();
        let history = self.history(store).map_err(|_| fail())?;
        if history.incarnations.last().map(|life| life.person) != Some(ruler)
            || guardians(store).map(|(id, _)| id).collect::<BTreeSet<_>>()
                != facts.guardian_people().collect::<BTreeSet<_>>()
        {
            return Err(fail());
        }
        validate_invocation_learning(store, |person| people.contains_key(&person))?;
        for (index, life) in history.incarnations.iter().enumerate() {
            let person = people.get(&life.person).ok_or_else(fail)?;
            if life.died_at.is_some() == person.alive
                || life
                    .died_at
                    .is_some_and(|died| !facts.recorded_death(life.person, died))
            {
                return Err(fail());
            }
            if index > 0 {
                let died = history.incarnations[index - 1].died_at.ok_or_else(fail)?;
                let first = people
                    .values()
                    .filter(|person| {
                        person.born > died
                            && person.initial_age == SimDuration(0)
                            && person.parent_count == 2
                    })
                    .min_by_key(|person| (person.born, person.person));
                if person.born != life.bound_at
                    || person.born <= died
                    || person.initial_age != SimDuration(0)
                    || person.parent_count != 2
                    || first.map(|person| person.person) != Some(life.person)
                {
                    return Err(fail());
                }
            }
        }
        for (actor, guardian) in guardians(store) {
            if !people.contains_key(&actor) {
                return Err(fail());
            }
            if let Some(plan) = &guardian.provision
                && (plan.delivery.last_receipt >= next_event
                    || plan
                        .delivery
                        .requirement
                        .source
                        .is_some_and(|id| facts.stock(id).is_none()))
            {
                return Err(fail());
            }
            if let Some(request) = &guardian.requested_delivery {
                let delivery = &request.delivery;
                if request.last_event >= next_event
                    || delivery.last_receipt >= next_event
                    || delivery.requirement.source.is_none_or(|id| {
                        !facts
                            .stock(id)
                            .is_some_and(|stock| stock.good == delivery.requirement.good)
                    })
                    || request.cargo.iter().any(|(id, quantity)| {
                        !facts.stock(*id).is_some_and(|stock| {
                            stock.good == delivery.requirement.good
                                && stock.holder == Some(actor)
                                && *quantity <= stock.quantity.0
                        })
                    })
                {
                    return Err(fail());
                }
            }
            if let Some(assignment) = &guardian.assignment {
                let valid = match assignment.request {
                    GuardianRequest::Move { destination } => facts.area_exists(destination),
                    GuardianRequest::Work { site } => facts.site_exists(site),
                    GuardianRequest::Rest => true,
                    GuardianRequest::Supply {
                        source, quantity, ..
                    } => facts.stock(source).is_some() && quantity.0 > 0,
                };
                if !valid {
                    return Err(fail());
                }
            }
            if let Some(location) = &guardian.sensed_location {
                match location {
                    Location::AtArea(area) if !facts.area_exists(*area) => return Err(fail()),
                    Location::InTransit {
                        from,
                        to,
                        departed,
                        arrives,
                        ..
                    } if !facts.area_exists(*from)
                        || !facts.area_exists(*to)
                        || departed >= arrives
                        || *departed > guardian.sensed_at =>
                    {
                        return Err(fail());
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}
