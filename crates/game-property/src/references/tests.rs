use super::*;
use game_ecs::{Schema, Store};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
struct References {
    people: BTreeMap<PersonId, PersonReference>,
    stocks: BTreeMap<InventoryId, InventoryReference>,
    due: BTreeSet<(PersonId, SimTime)>,
}
impl PropertyReferences for References {
    fn person(&self, id: PersonId) -> Option<PersonReference> {
        self.people.get(&id).copied()
    }
    fn inventory(&self, id: InventoryId) -> Option<InventoryReference> {
        self.stocks.get(&id).copied()
    }
    fn area_exists(&self, id: AreaId) -> bool {
        id == AreaId(3)
    }
    fn physical_completion_scheduled(&self, actor: PersonId, at: SimTime) -> bool {
        self.due.contains(&(actor, at))
    }
}
fn fixture() -> (Runtime, Store, ValidationFacts, References) {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    insert_claim(
        &mut store,
        ClaimId(6),
        Claim {
            id: ClaimId(6),
            claimant: PersonId(2),
            inventory: InventoryId(4),
            rights: BTreeSet::from([ClaimRight::Consume]),
            evidence: Vec::new(),
            portions: BTreeSet::from([ClaimId(6)]),
            recognized_by: BTreeSet::new(),
            opposed_by: BTreeSet::new(),
        },
    )
    .unwrap();
    insert_provenance(
        &mut store,
        InventoryId(4),
        Provenance {
            portions: BTreeMap::from([(ClaimId(6), 10)]),
        },
    )
    .unwrap();
    insert_provenance(&mut store, InventoryId(5), Provenance::default()).unwrap();
    insert_incident(
        &mut store,
        IncidentId(7),
        Incident {
            id: IncidentId(7),
            actor: PersonId(1),
            source: InventoryId(4),
            destination: InventoryId(5),
            quantity: Quantity(2),
            area: AreaId(3),
            started_at: SimTime(0),
            completes: SimTime(10),
            changed_at: SimTime(0),
            outcome: IncidentOutcome::Attempt,
            portions: BTreeMap::from([(ClaimId(6), 2)]),
        },
    )
    .unwrap();
    insert_activity(
        &mut store,
        PersonId(1),
        Activity {
            pending: Some(IncidentId(7)),
            report: None,
        },
    )
    .unwrap();
    let taking = PhysicalActionReference::Taking {
        source: InventoryId(4),
        destination: InventoryId(5),
        quantity: Quantity(2),
        completes: SimTime(10),
    };
    let references = References {
        people: BTreeMap::from([
            (
                PersonId(1),
                PersonReference {
                    alive: true,
                    area: Some(AreaId(3)),
                    action: taking,
                },
            ),
            (
                PersonId(2),
                PersonReference {
                    alive: true,
                    area: Some(AreaId(3)),
                    action: PhysicalActionReference::Idle,
                },
            ),
        ]),
        stocks: BTreeMap::from([
            (
                InventoryId(4),
                InventoryReference {
                    quantity: Quantity(10),
                    good: Good::Food,
                    carrier: None,
                },
            ),
            (
                InventoryId(5),
                InventoryReference {
                    quantity: Quantity(0),
                    good: Good::Food,
                    carrier: Some(PersonId(1)),
                },
            ),
        ]),
        due: BTreeSet::from([(PersonId(1), SimTime(10))]),
    };
    (
        Runtime { last_event: 0 },
        store,
        ValidationFacts {
            now: SimTime(0),
            next_id: 8,
            next_event: 1,
            inventory_count: 2,
        },
        references,
    )
}
fn validate(
    runtime: &Runtime,
    store: &Store,
    facts: ValidationFacts,
    references: &References,
) -> Result<(), Error> {
    runtime.validate_composition(
        store,
        facts,
        references,
        references
            .people
            .iter()
            .map(|(actor, person)| (*actor, person.action)),
    )
}

#[test]
fn foreign_reference_and_scheduled_action_consistency_are_checked_by_property() {
    let (runtime, store, facts, references) = fixture();
    validate(&runtime, &store, facts, &references).unwrap();
    let before = store.clone();
    for attack in 0..8 {
        let mut changed = references.clone();
        match attack {
            0 => {
                changed.people.remove(&PersonId(2));
            }
            1 => changed.stocks.get_mut(&InventoryId(5)).unwrap().good = Good::Tool,
            2 => changed.stocks.get_mut(&InventoryId(5)).unwrap().carrier = Some(PersonId(2)),
            3 => changed.stocks.get_mut(&InventoryId(4)).unwrap().quantity = Quantity(11),
            4 => changed.people.get_mut(&PersonId(1)).unwrap().alive = false,
            5 => changed.people.get_mut(&PersonId(1)).unwrap().area = Some(AreaId(9)),
            6 => {
                changed.people.get_mut(&PersonId(1)).unwrap().action = PhysicalActionReference::Idle
            }
            _ => {
                changed.due.clear();
            }
        }
        assert!(
            validate(&runtime, &store, facts, &changed).is_err(),
            "reference attack {attack}"
        );
        assert_eq!(store, before);
    }
    assert!(
        validate(
            &runtime,
            &store,
            ValidationFacts {
                next_id: 7,
                ..facts
            },
            &references
        )
        .is_err()
    );
    assert!(
        validate(
            &runtime,
            &store,
            ValidationFacts {
                next_event: 0,
                ..facts
            },
            &references
        )
        .is_err()
    );
}

#[test]
fn empty_personal_columns_and_unbacked_foreign_actions_cannot_hide_orphans() {
    let (runtime, store, facts, references) = fixture();
    for attack in 0..4 {
        let mut changed = store.clone();
        match attack {
            0 => insert_activity(&mut changed, PersonId(3), Activity::default()).unwrap(),
            1 => insert_knowledge(&mut changed, PersonId(3), Knowledge::default()).unwrap(),
            2 => insert_requests(&mut changed, PersonId(3), Requests::default()).unwrap(),
            _ => insert_attitude(
                &mut changed,
                PersonId(3),
                Attitude {
                    exclusive_claims: 0,
                    sharing: 0,
                    willingness_to_defend: 0,
                },
            )
            .unwrap(),
        }
        assert!(validate(&runtime, &changed, facts, &references).is_err());
        assert!(validate_absent(&changed, []).is_err());
    }
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    let empty = Store::with_schema(schema);
    validate_absent(&empty, []).unwrap();
    assert!(
        validate_absent(
            &empty,
            [(
                PersonId(1),
                PhysicalActionReference::Taking {
                    source: InventoryId(4),
                    destination: InventoryId(5),
                    quantity: Quantity(2),
                    completes: SimTime(10)
                }
            )]
        )
        .is_err()
    );
    assert!(
        runtime
            .validate_composition(
                &empty,
                ValidationFacts {
                    inventory_count: 0,
                    ..facts
                },
                &references,
                [(
                    PersonId(1),
                    PhysicalActionReference::Reporting {
                        recipient: PersonId(2),
                        incident: IncidentId(7),
                        completes: SimTime(10)
                    }
                )]
            )
            .is_err()
    );
}

#[test]
fn report_references_require_live_available_recipients_and_matching_completion() {
    let (runtime, mut store, facts, mut references) = fixture();
    remove_activity(&mut store, PersonId(1)).unwrap();
    let incident = incident_mut(&mut store, IncidentId(7)).unwrap();
    incident.outcome = IncidentOutcome::Interrupted;
    incident.changed_at = SimTime(1);
    let snapshot = IncidentKnowledge {
        incident: incident.clone(),
        learned_at: SimTime(1),
        source: IncidentSource::Observed,
    };
    insert_activity(
        &mut store,
        PersonId(2),
        Activity {
            pending: None,
            report: Some(Report {
                sender: PersonId(2),
                recipient: PersonId(1),
                area: AreaId(3),
                started_at: SimTime(1),
                completes: SimTime(3),
                snapshot,
            }),
        },
    )
    .unwrap();
    references.people.get_mut(&PersonId(1)).unwrap().action = PhysicalActionReference::Idle;
    references.people.get_mut(&PersonId(2)).unwrap().action = PhysicalActionReference::Reporting {
        recipient: PersonId(1),
        incident: IncidentId(7),
        completes: SimTime(3),
    };
    references.due = BTreeSet::from([(PersonId(2), SimTime(3))]);
    let facts = ValidationFacts {
        now: SimTime(1),
        ..facts
    };
    validate(&runtime, &store, facts, &references).unwrap();
    for attack in 0..5 {
        let mut changed = references.clone();
        match attack {
            0 => changed.people.get_mut(&PersonId(1)).unwrap().alive = false,
            1 => changed.people.get_mut(&PersonId(1)).unwrap().area = None,
            2 => {
                changed.people.get_mut(&PersonId(1)).unwrap().action =
                    PhysicalActionReference::Other
            }
            3 => {
                changed.people.get_mut(&PersonId(2)).unwrap().action = PhysicalActionReference::Idle
            }
            _ => changed.due.clear(),
        }
        assert!(
            validate(&runtime, &store, facts, &changed).is_err(),
            "report reference attack {attack}"
        );
    }
    activity_mut(&mut store, PersonId(2))
        .unwrap()
        .report
        .as_mut()
        .unwrap()
        .snapshot
        .source = IncidentSource::Reported {
        sender: PersonId(99),
    };
    assert!(validate(&runtime, &store, facts, &references).is_err());
}
