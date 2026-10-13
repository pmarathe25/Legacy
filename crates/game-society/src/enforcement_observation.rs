//! Factual observation of an admitted enforcement closing projection.
use crate::enforcement::*;
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, StableId, Store};
pub struct EnforcementObservationPublication {
    guard: ReadGuard,
    last_event: Option<u64>,
}
pub fn stage_enforcement_observation(
    store: &Store,
    batch: &mut Batch,
    facts: AdoptionObservationFacts<'_>,
    mut reads: ReadSet,
    mut writes: ReadFootprint,
) -> Result<EnforcementObservationPublication, Error> {
    use std::collections::{BTreeMap, BTreeSet};
    let mut people = BTreeMap::new();
    for person in facts.people {
        if let Some(old) = people.insert(person.id, person)
            && old != person
        {
            return Err(Error::Unavailable);
        }
    }
    let mut by_area = BTreeMap::<AreaId, Vec<PersonId>>::new();
    for person in people.values() {
        if let Location::AtArea(area) = person.location
            && person.vitals.alive
            && person.vitals.health > 0
        {
            by_area.entry(area).or_default().push(person.id);
        }
    }
    let areas = by_area.keys().copied().collect::<BTreeSet<_>>();
    let observers = by_area.values().flatten().copied().collect::<BTreeSet<_>>();
    reads.membership::<Case>(store, |_, _| true);
    reads.membership::<Policies>(store, |_, _| true);
    reads.membership::<crate::institutions::Institution>(store, move |_, row| {
        areas.contains(&row.charter.jurisdiction)
    });
    let case_ids = cases(store)
        .map(|(id, _)| StableId(id.0))
        .chain(batch.edited_ids::<Case>())
        .collect::<BTreeSet<_>>();
    let mut announcements = Vec::new();
    for id in case_ids {
        reads.read::<Case>(store, id);
        if let Some(case) = batch.projected::<Case>(store, id)?
            && case.changed_at == facts.now
            && case.status != EnforcementStatus::Filing
            && (case
                .notice_area
                .is_some_and(|area| by_area.contains_key(&area))
                || !case.witnesses.is_disjoint(&observers))
        {
            announcements.push(case.clone());
        }
    }
    let institutions = crate::institutions(store)
        .map(|(id, _)| id)
        .chain(
            batch
                .edited_ids::<crate::institutions::Institution>()
                .map(|id| InstitutionId(id.0)),
        )
        .collect::<BTreeSet<_>>();
    let mut local_policies = BTreeMap::<AreaId, Vec<(InstitutionId, Policies)>>::new();
    for institution in institutions {
        let id = StableId(institution.0);
        if let Some(row) = batch.projected::<crate::institutions::Institution>(store, id)?
            && by_area.contains_key(&row.charter.jurisdiction)
        {
            reads.read::<crate::institutions::Institution>(store, id);
            let policies = batch
                .projected::<Policies>(store, id)?
                .cloned()
                .unwrap_or_default();
            reads.read::<Policies>(store, id);
            local_policies
                .entry(row.charter.jurisdiction)
                .or_default()
                .push((institution, policies));
        }
    }
    for (area, locals) in &by_area {
        let contacts = locals
            .iter()
            .copied()
            .take(64)
            .map(|person| {
                (
                    person,
                    ReportedContact {
                        person,
                        area: *area,
                        observed_at: facts.now,
                    },
                )
            })
            .collect::<Vec<_>>();
        let strikes = facts
            .assaults
            .iter()
            .filter(|strike| {
                facts
                    .last_event
                    .is_some_and(|cursor| strike.event.0 > cursor)
                    && strike.observed_at == facts.now
                    && strike.area == *area
            })
            .collect::<Vec<_>>();
        for actor in locals {
            let id = StableId(actor.0);
            reads.read::<Memory>(store, id);
            let mut memory = batch
                .projected::<Memory>(store, id)?
                .cloned()
                .unwrap_or_default();
            for (person, contact) in &contacts {
                memory.contacts.insert(*person, contact.clone());
            }
            // Strike hearing uses the same capped local listener order as production.
            if locals.iter().take(64).any(|listener| listener == actor) {
                for strike in &strikes {
                    memory.assaults.insert(strike.event, (*strike).clone());
                }
            }
            for (institution, policies) in local_policies.get(area).into_iter().flatten() {
                if policies.offices.is_empty() {
                    if let Some(known) = memory.known_policies.get_mut(institution) {
                        known.clear();
                    }
                    continue;
                }
                let known = memory.known_policies.entry(*institution).or_default();
                known.retain(|office, _| policies.offices.contains_key(office));
                for (office, policy) in &policies.offices {
                    let mut observed = policy.clone();
                    observed.observed_at = facts.now;
                    known.insert(*office, observed);
                }
            }
            for case in &announcements {
                if case.witnesses.contains(actor) || case.notice_area == Some(*area) {
                    memory.known.insert(
                        case.id,
                        super::view_of_case(
                            case,
                            *actor,
                            facts.now,
                            KnowledgeSource::Observed(*actor),
                        ),
                    );
                }
            }
            for view in memory.known.values_mut() {
                if let Some(contact) = memory.contacts.get(&view.target)
                    && view
                        .contact
                        .as_ref()
                        .is_none_or(|old| old.observed_at < contact.observed_at)
                {
                    view.contact = Some(contact.clone());
                }
            }
            if batch.projected::<Memory>(store, id)? != Some(&memory) {
                writes.protect::<Memory>(id);
                batch.insert(id, memory);
            }
        }
    }
    Ok(EnforcementObservationPublication {
        guard: reads.bind(store, batch, &writes)?,
        last_event: facts.last_event,
    })
}
impl EnforcementObservationPublication {
    /// Exact native opening and staged row witnesses for host publication.
    pub fn row_guard(&self) -> ReadGuard {
        self.guard.clone()
    }

    pub fn validate(
        &self,
        runtime: Option<&World>,
        store: &Store,
        batch: &Batch,
    ) -> Result<(), Error> {
        if runtime.map(|world| world.last_event) != self.last_event {
            return Err(Error::Unavailable);
        }
        self.guard.validate(store, batch)
    }
    pub fn finish(self, through: u64) -> World {
        World {
            last_event: through,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_ecs::Schema;

    fn person(id: u64, alive: bool) -> PersonFacts {
        PersonFacts {
            id: PersonId(id),
            inventory: InventoryId(id + 100),
            location: Location::AtArea(AreaId(1)),
            vitals: Vitals {
                hunger: 0,
                fatigue: 0,
                health: u64::from(alive),
                alive,
            },
            independent: true,
            combat_restricted: false,
        }
    }
    fn fixture(reject: bool) -> Store {
        let mut schema = Schema::new();
        crate::register_components(&mut schema).unwrap();
        schema
            .register::<Case>("society/enforcement-case", |_| Ok(()))
            .unwrap();
        schema
            .register::<Policies>("society/enforcement-policies", |_| Ok(()))
            .unwrap();
        let validator: fn(&Memory) -> Result<(), Error> = if reject {
            |memory| {
                if memory.assaults.is_empty() {
                    Ok(())
                } else {
                    Err(Error::Unavailable)
                }
            }
        } else {
            |_| Ok(())
        };
        schema
            .register::<Memory>("society/enforcement-memory", validator)
            .unwrap();
        let mut store = Store::with_schema(schema);
        store
            .insert_or_spawn(StableId(1), Memory::default())
            .unwrap();
        store
            .insert_or_spawn(StableId(2), Memory::default())
            .unwrap();
        store
    }
    fn stage(store: &Store, batch: &mut Batch) -> EnforcementObservationPublication {
        let people = [person(1, true), person(2, false)];
        let assaults = [WitnessedAssault {
            event: EventId(4),
            attacker: PersonId(1),
            target: PersonId(2),
            area: AreaId(1),
            damage: 1,
            observed_at: SimTime(5),
        }];
        stage_enforcement_observation(
            store,
            batch,
            AdoptionObservationFacts {
                now: SimTime(5),
                people: &people,
                assaults: &assaults,
                last_event: Some(3),
            },
            ReadSet::new(),
            ReadFootprint::new(),
        )
        .unwrap()
    }
    #[test]
    fn projected_dead_target_is_not_a_listener_and_receives_no_refresh() {
        let mut store = fixture(false);
        let mut batch = Batch::new();
        let receipt = stage(&store, &mut batch);
        receipt
            .validate(Some(&World { last_event: 3 }), &store, &batch)
            .unwrap();
        store.apply_batch(batch).unwrap();
        let observed = memory(&store, PersonId(1)).unwrap();
        assert!(observed.assaults.contains_key(&EventId(4)));
        assert_eq!(
            observed.contacts.keys().copied().collect::<Vec<_>>(),
            vec![PersonId(1)]
        );
        assert_eq!(memory(&store, PersonId(2)), Some(&Memory::default()));
    }
    #[test]
    fn late_memory_edit_invalidates_the_prepared_observer() {
        let store = fixture(false);
        let mut batch = Batch::new();
        let receipt = stage(&store, &mut batch);
        batch.insert(StableId(1), Memory::default());
        assert!(
            receipt
                .validate(Some(&World { last_event: 3 }), &store, &batch)
                .is_err()
        );
        assert_eq!(memory(&store, PersonId(1)), Some(&Memory::default()));
    }
    #[test]
    fn receiver_schema_rejection_preserves_source_memory() {
        let mut store = fixture(true);
        let mut batch = Batch::new();
        let _receipt = stage(&store, &mut batch);
        assert!(store.apply_batch(batch).is_err());
        assert_eq!(memory(&store, PersonId(1)), Some(&Memory::default()));
        assert_eq!(memory(&store, PersonId(2)), Some(&Memory::default()));
    }
}
