//! Contextual validation depends only on factual references supplied by owners.
use crate::recruitment as stored_recruitment;
use crate::*;
use game_agreements::AgreementTerm;
use game_cargo::{Inventory, InventoryLocation};
use game_core::*;
use game_ecs::Store;
use std::collections::{BTreeMap, BTreeSet};
pub struct ValidationCatalog<'a> {
    pub now: SimTime,
    pub next_id: u64,
    pub next_event: u64,
    pub people: &'a BTreeSet<PersonId>,
    pub areas: &'a BTreeSet<AreaId>,
    pub carrying_capacity: Quantity,
}
pub trait ValidationFacts {
    fn inventory(&self, id: InventoryId) -> Option<&Inventory>;
    fn agreement_parties(&self, id: AgreementId) -> Option<&BTreeSet<PersonId>>;
    fn talk_deadline(&self, actor: PersonId) -> Option<SimTime>;
    fn at_area(&self, actor: PersonId) -> Option<AreaId>;
    fn talking_people(&self) -> impl Iterator<Item = PersonId>;
}
pub fn validate_context(
    runtime: &Runtime,
    store: &Store,
    catalog: ValidationCatalog<'_>,
    facts: &impl ValidationFacts,
) -> Result<(), Error> {
    let fail = || Error::InvalidSave("inconsistent expedition commitments".into());
    runtime.validate_storage(store, catalog.now)?;
    if runtime.last_event >= catalog.next_event {
        return Err(fail());
    }
    let people = catalog.people;
    let areas = catalog.areas;
    let objective_valid = |objective: &MissionObjective| match objective {
        MissionObjective::Visit { destination } => areas.contains(destination),
        MissionObjective::Deliver {
            source,
            destination,
            quantity,
        } => {
            quantity.0 > 0
                && facts
                    .inventory(*source)
                    .zip(facts.inventory(*destination))
                    .is_some_and(|(a, b)| {
                        a.id != b.id && a.good == b.good && quantity.0 <= b.capacity.0
                    })
        }
        MissionObjective::Defeat { person } | MissionObjective::Confine { person, .. } => {
            people.contains(person)
        }
        MissionObjective::Rescue { person, return_to } => {
            people.contains(person) && areas.contains(return_to)
        }
        MissionObjective::Patrol {
            areas: selected, ..
        } => {
            !selected.is_empty()
                && selected.len() <= 16
                && selected.iter().all(|a| areas.contains(a))
        }
    };
    let location_valid = |location: &Location, observed_at: SimTime| match location {
        Location::AtArea(area) => areas.contains(area),
        Location::Carried { carrier } => people.contains(carrier),
        Location::StoppedOnRoad { from, to, .. } => areas.contains(from) && areas.contains(to),
        Location::InTransit {
            from, to, departed, ..
        } => areas.contains(from) && areas.contains(to) && *departed <= observed_at,
    };
    for (id, m) in missions(store) {
        let roster: BTreeSet<_> = m
            .proposal
            .recruits
            .iter()
            .copied()
            .chain([m.proposal.leader])
            .collect();
        if id != m.id
            || id.0 == 0
            || id.0 >= catalog.next_id
            || !people.contains(&m.sponsor)
            || !areas.contains(&m.proposal.home)
            || !objective_valid(&m.proposal.objective)
            || !(1..=32).contains(&m.members.len())
            || roster != m.members.keys().copied().collect()
            || m.created_at > catalog.now
            || m.proposal.deadline <= m.created_at
            || m.departed_at
                .is_some_and(|at| at < m.created_at || at > catalog.now)
            || m.proposal.supplies.len() > 32
        {
            return Err(fail());
        }
        for (person, terms) in &m.proposal.compensation {
            if !m.members.contains_key(person)
                || terms.len() > 8
                || terms.iter().any(|term| {
                    term.amount() == 0
                        || term.parties()[0] == term.parties()[1]
                        || term
                            .parties()
                            .iter()
                            .any(|p| *p != m.sponsor && *p != *person)
                        || match term {
                            AgreementTerm::Delivery {
                                source,
                                destination,
                                quantity,
                                ..
                            } => !facts
                                .inventory(*source)
                                .zip(facts.inventory(*destination))
                                .is_some_and(|(a, b)| {
                                    a.id != b.id && a.good == b.good && quantity.0 <= b.capacity.0
                                }),
                            AgreementTerm::Access { inventory, .. } => {
                                !facts.inventory(*inventory).is_some()
                            }
                            _ => true,
                        }
                })
            {
                return Err(fail());
            }
        }
        for supply in &m.proposal.supplies {
            if !m.members.contains_key(&supply.carrier)
                || !facts.inventory(supply.source).is_some()
                || supply.quantity.0 == 0
                || supply.quantity > catalog.carrying_capacity
            {
                return Err(fail());
            }
        }
        for (person, member) in &m.members {
            let offer = stored_recruitment(store, member.recruitment).ok_or_else(fail)?;
            if *person != member.person
                || !people.contains(person)
                || offer.mission != id
                || offer.person != *person
                || member.received_at > catalog.now
                || !objective_valid(&member.directive)
                || member.visited.iter().any(|a| !areas.contains(a))
                || member
                    .contacts
                    .iter()
                    .any(|(p, a)| !people.contains(p) || !areas.contains(a))
                || member
                    .last_leg_from
                    .is_some_and(|area| !areas.contains(&area))
                || member.explanation.len() > 4096
                || member.declared_explanation.len() > 4096
            {
                return Err(fail());
            }
            if let Some(record) = &member.confinement
                && (member.directive.target_person() != Some(record.person)
                    || !matches!(member.directive, MissionObjective::Confine { .. })
                    || record.captured_at > catalog.now
                    || record
                        .released_at
                        .is_some_and(|at| at > catalog.now || at < record.captured_at))
            {
                return Err(fail());
            }
            if let MissionObjective::Deliver { quantity, .. } = member.directive
                && member.delivered > quantity.0
            {
                return Err(fail());
            }
            let mut attributed = BTreeMap::<InventoryId, u128>::new();
            for (source, tags) in &member.supply_cargo {
                let supply = m
                    .proposal
                    .supplies
                    .iter()
                    .filter(|supply| supply.carrier == *person && supply.source == *source)
                    .max_by_key(|supply| supply.quantity)
                    .ok_or_else(fail)?;
                let good = facts.inventory(*source).ok_or_else(fail)?.good;
                let mut total = 0u128;
                if tags.is_empty() {
                    return Err(fail());
                }
                for (inventory, quantity) in tags {
                    if *quantity == 0
                        || !facts.inventory(*inventory).is_some_and(|stock| {
                            stock.good == good
                                && stock.location == InventoryLocation::Carried(*person)
                        })
                    {
                        return Err(fail());
                    }
                    total += u128::from(*quantity);
                    *attributed.entry(*inventory).or_default() += u128::from(*quantity);
                }
                if total > u128::from(supply.quantity.0) {
                    return Err(fail());
                }
            }
            if attributed.iter().any(|(inventory, q)| {
                *q > u128::from(
                    facts
                        .inventory(*inventory)
                        .expect("inventory exists")
                        .quantity
                        .0,
                )
            }) {
                return Err(fail());
            }
            for (inventory, quantity) in &member.cargo {
                if *quantity == 0
                    || !facts.inventory(*inventory).is_some_and(|i| {
                        i.location == InventoryLocation::Carried(*person)
                            && i.quantity.0 >= *quantity
                    })
                {
                    return Err(fail());
                }
            }
        }
    }
    for (id, offer) in recruitments(store) {
        let m = mission(store, offer.mission).ok_or_else(fail)?;
        if id != offer.id
            || id.0 == 0
            || id.0 >= catalog.next_id
            || !m
                .members
                .get(&offer.person)
                .is_some_and(|person| person.recruitment == id)
            || offer.compensation.len() > 8
            || offer.agreement.is_some_and(|id| {
                !facts
                    .agreement_parties(id)
                    .is_some_and(|parties| *parties == BTreeSet::from([m.sponsor, offer.person]))
            })
        {
            return Err(fail());
        }
        for term in &offer.compensation {
            if term.amount() == 0
                || term.parties()[0] == term.parties()[1]
                || term
                    .parties()
                    .iter()
                    .any(|p| *p != m.sponsor && *p != offer.person)
                || !matches!(
                    term,
                    AgreementTerm::Delivery { .. } | AgreementTerm::Access { .. }
                )
            {
                return Err(fail());
            }
            match term {
                AgreementTerm::Delivery {
                    source,
                    destination,
                    ..
                } if !facts.inventory(*source).is_some()
                    || !facts.inventory(*destination).is_some() =>
                {
                    return Err(fail());
                }
                AgreementTerm::Access { inventory, .. }
                    if !facts.inventory(*inventory).is_some() =>
                {
                    return Err(fail());
                }
                _ => {}
            }
        }
    }
    for (observer, personal) in knowledges(store) {
        let views = &personal.missions;
        if !people.contains(&observer) {
            return Err(fail());
        }
        for (id, view) in views {
            if *id != view.id
                || !mission(store, *id).is_some()
                || view.observed_at > catalog.now
                || !people.contains(&view.sponsor)
                || !people.contains(&view.leader)
                || !areas.contains(&view.home)
                || !objective_valid(&view.objective)
                || view.companions.len() > 32
                || view.recruitments.len() > 32
                || view.reported_by.is_some_and(|p| !people.contains(&p))
            {
                return Err(fail());
            }
            if view.supplies.iter().any(|supply| {
                !people.contains(&supply.carrier) || facts.inventory(supply.source).is_none()
            }) {
                return Err(fail());
            }
            for companion in &view.companions {
                if !people.contains(&companion.person)
                    || companion.observed_at > catalog.now
                    || companion
                        .location
                        .as_ref()
                        .is_some_and(|location| !location_valid(location, companion.observed_at))
                {
                    return Err(fail());
                }
            }
            for recruitment in &view.recruitments {
                if !stored_recruitment(store, recruitment.id)
                    .is_some_and(|offer| offer.person == recruitment.person && offer.mission == *id)
                    || recruitment
                        .payment_agreement
                        .is_some_and(|id| facts.agreement_parties(id).is_none())
                {
                    return Err(fail());
                }
            }
        }
    }
    let mut occupied = BTreeSet::new();
    for (speaker, talk) in talks(store) {
        if speaker != talk.speaker
            || !people.contains(&speaker)
            || !people.contains(&talk.listener)
            || !areas.contains(&talk.area)
            || talk.completes <= catalog.now
        {
            return Err(fail());
        }
        if !occupied.insert(talk.speaker)
            || facts.talk_deadline(talk.speaker) != Some(talk.completes)
            || ![talk.speaker, talk.listener]
                .iter()
                .all(|actor| facts.at_area(*actor) == Some(talk.area))
        {
            return Err(fail());
        }
        match &talk.conversation {
            Conversation::Objective {
                mission,
                leader,
                objective,
            } => {
                if !crate::mission(store, *mission).is_some_and(|m| {
                    m.proposal.leader == *leader
                        && talk.listener == *leader
                        && (m.sponsor == speaker || *leader == speaker)
                }) || !objective_valid(objective)
                {
                    return Err(fail());
                }
            }
            Conversation::Retreat { mission, leader } => {
                if !crate::mission(store, *mission).is_some_and(|m| {
                    m.proposal.leader == *leader
                        && talk.listener == *leader
                        && (m.sponsor == speaker || *leader == speaker)
                }) {
                    return Err(fail());
                }
            }
            Conversation::Report {
                recipient,
                snapshot,
            } => {
                if *recipient != talk.listener
                    || !mission(store, snapshot.id).is_some()
                    || snapshot.observed_at > catalog.now
                    || !people.contains(&snapshot.sponsor)
                    || !people.contains(&snapshot.leader)
                    || !areas.contains(&snapshot.home)
                    || !objective_valid(&snapshot.objective)
                    || snapshot
                        .reported_by
                        .is_some_and(|person| !people.contains(&person))
                    || snapshot.companions.iter().any(|member| {
                        !people.contains(&member.person)
                            || member.observed_at > catalog.now
                            || member.location.as_ref().is_some_and(|location| {
                                !location_valid(location, member.observed_at)
                            })
                    })
                    || snapshot.supplies.iter().any(|supply| {
                        !people.contains(&supply.carrier)
                            || facts.inventory(supply.source).is_none()
                    })
                    || snapshot.recruitments.iter().any(|offer| {
                        !stored_recruitment(store, offer.id).is_some_and(|row| {
                            row.person == offer.person && row.mission == snapshot.id
                        })
                    })
                {
                    return Err(fail());
                }
            }
        }
    }
    if facts
        .talking_people()
        .any(|actor| !occupied.contains(&actor))
    {
        return Err(fail());
    }
    Ok(())
}

/// Runtime prerequisites are factual capability presence, checked by service.
#[derive(Clone, Copy, Debug)]
pub struct RuntimeAvailability {
    pub physical: bool,
    pub decisions: bool,
    pub conflict: bool,
    pub agreements: bool,
}
pub fn validate_native_context(
    runtime: Option<&Runtime>,
    store: &Store,
    catalog: ValidationCatalog<'_>,
    availability: RuntimeAvailability,
    facts: &impl ValidationFacts,
) -> Result<(), Error> {
    let Some(runtime) = runtime else {
        if missions(store).next().is_some()
            || recruitments(store).next().is_some()
            || knowledges(store).next().is_some()
            || talks(store).next().is_some()
        {
            return Err(Error::InvalidSave(
                "mission components without runtime".into(),
            ));
        }
        return Ok(());
    };
    if !availability.physical
        || !availability.decisions
        || !availability.conflict
        || !availability.agreements
    {
        return Err(Error::InvalidSave(
            "inconsistent expedition commitments".into(),
        ));
    }
    validate_context(runtime, store, catalog, facts)
}

pub fn saved_objective_valid(
    store: &Store,
    objective: &MissionObjective,
    people: &BTreeSet<PersonId>,
    areas: &BTreeSet<AreaId>,
) -> bool {
    let area = |a: &AreaId| areas.contains(a);
    let person = |id: &PersonId| people.contains(id);
    match objective {
        MissionObjective::Visit { destination } => area(destination),
        MissionObjective::Deliver {
            source,
            destination,
            quantity,
        } => {
            quantity.0 > 0
                && source != destination
                && game_cargo::inventory(store, *source)
                    .zip(game_cargo::inventory(store, *destination))
                    .is_some_and(|(a, b)| a.good == b.good)
        }
        MissionObjective::Defeat { person: p } | MissionObjective::Confine { person: p, .. } => {
            person(p)
        }
        MissionObjective::Rescue {
            person: p,
            return_to,
        } => person(p) && area(return_to),
        MissionObjective::Patrol { areas, .. } => {
            !areas.is_empty() && areas.len() <= 16 && areas.iter().all(area)
        }
    }
}
pub fn validate_saved_view(
    store: &Store,
    v: &MissionView,
    now: SimTime,
    people: &BTreeSet<PersonId>,
    areas: &BTreeSet<AreaId>,
    agreements_enabled: bool,
) -> Result<(), Error> {
    let bad = || Error::InvalidSave("Invalid dated mission report".into());
    if crate::mission(store, v.id).is_none()
        || !people.contains(&v.sponsor)
        || !people.contains(&v.leader)
        || !areas.contains(&v.home)
        || !saved_objective_valid(store, &v.objective, people, areas)
        || v.observed_at > now
        || v.companions.len() > 32
        || v.recruitments.len() > 32
        || v.supplies.len() > 32
        || v.reported_by.is_some_and(|p| !people.contains(&p))
    {
        return Err(bad());
    }
    for c in &v.companions {
        if !people.contains(&c.person)
            || c.observed_at > now
            || c.location.as_ref().is_some_and(|l| match l {
                Location::AtArea(a) => !areas.contains(a),
                Location::Carried { carrier } => !people.contains(carrier),
                Location::StoppedOnRoad {
                    from,
                    to,
                    progress,
                    duration,
                } => {
                    !areas.contains(from)
                        || !areas.contains(to)
                        || from == to
                        || duration.0 == 0
                        || progress > duration
                }
                Location::InTransit {
                    from,
                    to,
                    departed,
                    arrives,
                    road_start,
                } => {
                    !areas.contains(from)
                        || !areas.contains(to)
                        || from == to
                        || departed >= arrives
                        || road_start.is_some_and(|p| !p.valid())
                }
            })
        {
            return Err(bad());
        }
    }
    for r in &v.recruitments {
        if !crate::recruitment(store, r.id)
            .is_some_and(|a| a.person == r.person && a.mission == v.id)
            || r.payment_agreement.is_some_and(|id| {
                !agreements_enabled || game_agreements::agreement(store, id).is_none()
            })
            || r.compensation.iter().any(|t| {
                t.parties().iter().any(|p| !people.contains(p))
                    || match t {
                        AgreementTerm::Delivery {
                            source,
                            destination,
                            ..
                        } => {
                            game_cargo::inventory(store, *source).is_none()
                                || game_cargo::inventory(store, *destination).is_none()
                        }
                        AgreementTerm::Access { inventory, .. } => {
                            game_cargo::inventory(store, *inventory).is_none()
                        }
                        _ => true,
                    }
            })
        {
            return Err(bad());
        }
    }
    if v.supplies.iter().any(|supply| {
        !people.contains(&supply.carrier)
            || game_cargo::inventory(store, supply.source).is_none()
            || supply.quantity.0 == 0
    }) {
        return Err(bad());
    }
    Ok(())
}
