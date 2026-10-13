//! Validation of persisted personal plans, memories and dispositions.
use crate::decisions::*;
use game_core::*;
use game_ecs::{Component, Store};
use std::collections::{BTreeMap, BTreeSet};

pub struct SaveFacts {
    pub people: BTreeSet<PersonId>,
    pub areas: BTreeSet<AreaId>,
    pub inventories: BTreeMap<InventoryId, Good>,
    pub sites: BTreeSet<SiteId>,
    pub projects: BTreeSet<ProjectId>,
    pub obligations: BTreeSet<ObligationId>,
    pub resources: BTreeSet<ResourceId>,
    pub time: SimTime,
    pub next_event: u64,
}

pub fn validate_store<A: Component>(
    store: &Store,
    world: Option<&Runtime>,
    facts: &SaveFacts,
    action_shape: impl Fn(&A) -> ActionShape,
) -> Result<(), Error> {
    let fail = || Error::InvalidSave("inconsistent committed state".into());
    let Some(world) = world else {
        return if stock_rows(store).next().is_some() {
            Err(fail())
        } else {
            Ok(())
        };
    };
    if crate::decisions::minds::<A>(store)
        .map(|(id, _)| id)
        .collect::<BTreeSet<_>>()
        != facts.people.clone()
        || crate::decisions::stock_rows(store)
            .map(|(id, _)| id)
            .collect::<BTreeSet<_>>()
            != facts.people.clone()
        || !world.concepts.contains_key(&world.work_concept)
        || world.last_learning_event >= facts.next_event
        || world.switching_margin < 0
    {
        return Err(fail());
    }
    for (id, mind) in crate::decisions::minds::<A>(store) {
        let id = &id;
        if !facts.people.contains(id)
            || mind.interests.values().any(|v| *v > 1_000)
            || mind
                .plan
                .as_ref()
                .is_some_and(|plan| plan.shape != action_shape(&plan.action))
            || [
                mind.personality.discipline,
                mind.personality.ambition,
                mind.personality.sociability,
                mind.personality.risk_tolerance,
                mind.personality.hedonism,
                mind.personality.empathy,
                mind.personality.aggression,
                mind.personality.honesty,
                mind.personality.patience,
            ]
            .iter()
            .any(|v| *v > 1_000)
        {
            return Err(fail());
        }
        if let Some(plan) = &mind.plan {
            use crate::decisions::{CommitmentRef, IntentionKey, Motive, Subject};
            let commitment_exists = |commitment: CommitmentRef| match commitment {
                CommitmentRef::Obligation(id) => facts.obligations.contains(&id),
                CommitmentRef::Project(id) => facts.projects.contains(&id),
            };
            let subject_exists = |subject: Subject| match subject {
                Subject::Inventory(id) => facts.inventories.contains_key(&id),
                Subject::Site(id) => facts.sites.contains(&id),
                Subject::Project(id) => facts.projects.contains(&id),
                Subject::Resource(id) => facts.resources.contains(&id),
            };
            let key_valid = match plan.key {
                IntentionKey::Relief { beneficiary, .. } => facts.people.contains(&beneficiary),
                IntentionKey::Development {
                    beneficiary,
                    subject,
                    ..
                } => facts.people.contains(&beneficiary) && subject.is_none_or(subject_exists),
                IntentionKey::Provision {
                    good,
                    beneficiary,
                    source,
                } => {
                    facts.people.contains(&beneficiary)
                        && source.is_none_or(|id| {
                            facts
                                .inventories
                                .get(&id)
                                .is_some_and(|stock| *stock == good)
                        })
                }
                IntentionKey::Commitment(commitment) => commitment_exists(commitment),
            };
            let motives_valid = plan.motives.iter().all(|motive| match motive {
                Motive::Relieve { beneficiary, .. } | Motive::Develop { beneficiary, .. } => {
                    facts.people.contains(beneficiary)
                }
                Motive::Provide {
                    requirement,
                    beneficiary,
                    ..
                } => {
                    facts.people.contains(beneficiary)
                        && requirement.quantity.0 > 0
                        && requirement.source.is_none_or(|id| {
                            facts
                                .inventories
                                .get(&id)
                                .is_some_and(|stock| *stock == requirement.good)
                        })
                }
                Motive::Fulfil { commitment } => commitment_exists(*commitment),
            });
            if !key_valid
                || !motives_valid
                || plan
                    .prospects
                    .source
                    .is_some_and(|id| !facts.inventories.contains_key(&id))
                || plan
                    .prospects
                    .site
                    .is_some_and(|id| !facts.sites.contains(&id))
                || plan
                    .prospects
                    .sponsor
                    .is_some_and(|id| !facts.people.contains(&id))
            {
                return Err(fail());
            }
        }
        if let Some(plan) = &mind.plan
            && let Some(delivery) = &plan.delivery
        {
            let target_exists = match delivery.destination {
                game_logistics::Destination::Inventory(target) => {
                    facts.inventories.contains_key(&target)
                }
                game_logistics::Destination::Contact(target) => facts.people.contains(&target),
            };
            let matches_motive = plan.motives.iter().all(|motive| match motive {
                crate::decisions::Motive::Provide {
                    requirement,
                    beneficiary,
                    ..
                } => {
                    delivery.destination == game_logistics::Destination::Contact(*beneficiary)
                        && delivery.requirement.good == requirement.good
                        && delivery.requirement.source == requirement.source
                }
                _ => true,
            });
            if !delivery.valid()
                || !target_exists
                || !matches_motive
                || delivery.last_receipt >= facts.next_event
                || delivery.requirement.source.is_some_and(|source| {
                    !facts
                        .inventories
                        .get(&source)
                        .is_some_and(|stock| *stock == delivery.requirement.good)
                })
            {
                return Err(fail());
            }
        }
        for (from, links) in &mind.routes {
            if !facts.areas.contains(from)
                || links
                    .iter()
                    .any(|(to, duration)| !facts.areas.contains(to) || duration.0 == 0)
            {
                return Err(fail());
            }
        }
        if mind.last_failure.is_some_and(|(time, inventory)| {
            time > facts.time || !facts.inventories.contains_key(&inventory)
        }) {
            return Err(fail());
        }
        for (id, fact) in &crate::decisions::stocks(store, *id).ok_or_else(fail)?.known {
            if *id != fact.inventory
                || !facts.inventories.contains_key(id)
                || !facts.areas.contains(&fact.area)
                || fact.observed_at > facts.time
                || fact.learned_at > facts.time
            {
                return Err(fail());
            }
        }
        for (child, signal) in &mind.child_signals {
            if !facts.people.contains(child)
                || !match signal.location {
                    Location::AtArea(area) => facts.areas.contains(&area),
                    Location::StoppedOnRoad {
                        from,
                        to,
                        progress,
                        duration,
                    } => {
                        facts.areas.contains(&from)
                            && facts.areas.contains(&to)
                            && from != to
                            && duration.0 > 0
                            && progress <= duration
                    }
                    Location::InTransit {
                        from,
                        to,
                        departed,
                        arrives,
                        ..
                    } => {
                        facts.areas.contains(&from)
                            && facts.areas.contains(&to)
                            && from != to
                            && departed <= signal.observed_at
                            && signal.observed_at <= arrives
                            && departed < arrives
                    }
                    Location::Carried { .. } => false,
                }
                || signal.observed_at > facts.time
                || ![0, 3_000, 20_000].contains(&signal.urgency)
                || ![0, 3_000, 20_000].contains(&signal.hydration_urgency)
            {
                return Err(fail());
            }
        }
        for id in mind.relationships.keys() {
            if !facts.people.contains(id) {
                return Err(fail());
            }
        }
        for (id, value) in &mind.values {
            if !world.concepts.contains_key(id) || !(0..=1_000).contains(&value.weight) {
                return Err(fail());
            }
        }
        for (id, site) in &mind.sites {
            if *id != site.site
                || !facts.sites.contains(id)
                || !facts.areas.contains(&site.area)
                || !facts.inventories.contains_key(&site.input)
                || !facts.inventories.contains_key(&site.tool)
            {
                return Err(fail());
            }
        }
    }
    Ok(())
}

pub fn validate_review_deadlines(now: SimTime, reviews: &[SimTime]) -> Result<(), Error> {
    if reviews.len() == 1
        && reviews[0].0.is_multiple_of(60_000)
        && reviews[0] > now
        && reviews[0].0 - now.0 <= 60_000
    {
        Ok(())
    } else {
        Err(Error::InvalidSave("inconsistent committed state".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_ecs::Schema;

    #[test]
    fn review_deadline_must_continue_the_minute_schedule() {
        assert!(validate_review_deadlines(SimTime(59_999), &[SimTime(60_000)]).is_ok());
        assert!(validate_review_deadlines(SimTime(59_999), &[SimTime(120_000)]).is_err());
        assert!(validate_review_deadlines(SimTime(59_999), &[SimTime(60_001)]).is_err());
        assert!(validate_review_deadlines(SimTime(60_000), &[SimTime(60_000)]).is_err());
    }
    #[test]
    fn standalone_validation_rejects_unknown_route_and_future_memory() {
        let mut schema = Schema::new();
        register_minds::<u8>(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        let actor = PersonId(1);
        let row = Mind {
            interests: BTreeMap::new(),
            routes: BTreeMap::new(),
            last_failure: None,
            child_signals: BTreeMap::new(),
            personality: Personality::industrious(),
            dream: Aspiration::Comfort,
            sites: BTreeMap::new(),
            relationships: BTreeMap::new(),
            values: BTreeMap::new(),
            goal: Goal::Rest,
            plan: None,
            explanation: Explanation {
                goal: Goal::Rest,
                need: 0,
                benefit: 0,
                value: 0,
                total: 0,
                blocker: None,
            },
            retry_after: SimTime(0),
        };
        insert_mind::<u8>(&mut store, actor, row).unwrap();
        insert_stocks(&mut store, actor, Stocks::default()).unwrap();
        let world = Runtime {
            concepts: BTreeMap::from([(ConceptId(1), Good::Food)]),
            work_concept: ConceptId(1),
            last_learning_event: 0,
            switching_margin: 0,
        };
        let facts = SaveFacts {
            people: BTreeSet::from([actor]),
            areas: BTreeSet::from([AreaId(1)]),
            inventories: BTreeMap::from([(InventoryId(2), Good::Food)]),
            sites: BTreeSet::new(),
            projects: BTreeSet::new(),
            obligations: BTreeSet::new(),
            resources: BTreeSet::new(),
            time: SimTime(10),
            next_event: 1,
        };
        assert!(validate_store::<u8>(&store, Some(&world), &facts, |_| ActionShape::Other).is_ok());
        mind_mut::<u8>(&mut store, actor)
            .unwrap()
            .routes
            .insert(AreaId(1), BTreeMap::from([(AreaId(99), SimDuration(1))]));
        assert!(matches!(
            validate_store::<u8>(&store, Some(&world), &facts, |_| ActionShape::Other),
            Err(Error::InvalidSave(_))
        ));
        mind_mut::<u8>(&mut store, actor).unwrap().routes.clear();
        mind_mut::<u8>(&mut store, actor).unwrap().last_failure =
            Some((SimTime(11), InventoryId(2)));
        assert!(
            validate_store::<u8>(&store, Some(&world), &facts, |_| ActionShape::Other).is_err()
        );
    }
}
