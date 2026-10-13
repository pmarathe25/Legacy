use crate::*;
use game_core::*;
use game_ecs::{Batch, ReadSet, Schema, StableId, Store};
use std::collections::BTreeMap;

fn founder() -> Guardian {
    Guardian {
        target: Some(PersonId(1)),
        sensed_location: Some(Location::AtArea(AreaId(7))),
        sensed_at: SimTime(0),
        goal: GuardianGoal::Mentoring,
        explanation: "Present".into(),
        assessments: BTreeMap::new(),
        assignment: None,
        requested_delivery: None,
        provision: None,
        knows_invocation: true,
    }
}
fn fixture() -> (Runtime, Store) {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    game_characters::people::register_people(&mut schema).unwrap();
    schema
        .register::<u64>("fixture/fact", |value| {
            if *value <= 10 {
                Ok(())
            } else {
                Err(Error::Unavailable)
            }
        })
        .unwrap();
    let mut store = Store::with_schema(schema);
    for id in [1, 2, 3, 10, 11] {
        store.spawn(StableId(id)).unwrap();
    }
    let runtime = Runtime::initialize(
        &mut store,
        StableId(1),
        MentorshipRules::default(),
        History {
            incarnations: vec![Incarnation {
                person: PersonId(1),
                bound_at: SimTime(0),
                died_at: None,
            }],
        },
        [(PersonId(2), founder()), (PersonId(3), founder())],
    )
    .unwrap();
    (runtime, store)
}
fn birth(person: u64, born: u64) -> ActualBirth {
    ActualBirth {
        person: PersonId(person),
        born: SimTime(born),
        initial_age: SimDuration(0),
        parent_count: 2,
        alive: true,
        location: Location::AtArea(AreaId(8)),
    }
}
fn bold() -> GuardianTraits {
    GuardianTraits {
        discipline: 0,
        patience: 0,
        risk_tolerance: 1_000,
    }
}
fn request_facts(now: u64) -> RequestFacts {
    RequestFacts {
        now: SimTime(now),
        requester_independent: true,
        requester_alive: true,
        requester_idle: true,
        requester_in_combat: false,
        requester_restricted: false,
        requester_in_care: false,
        requester_location: Location::AtArea(AreaId(7)),
        guardian_location: Location::AtArea(AreaId(7)),
        guardian_constrained: false,
        guardian_in_combat: false,
        guardian_duty_pending: false,
        signal_urgency: 0,
    }
}

#[test]
fn canonical_death_and_actual_birth_continue_after_store_and_metadata_restore() {
    let (runtime, mut store) = fixture();
    let metadata = runtime.clone();
    assert_eq!(
        runtime
            .prepare_death(&store, true, SimTime(5), &BTreeMap::new())
            .unwrap()
            .commit(&runtime, &mut store)
            .unwrap(),
        Succession::Unchanged
    );
    assert_eq!(
        runtime
            .prepare_death(&store, false, SimTime(5), &BTreeMap::new())
            .unwrap()
            .commit(&runtime, &mut store)
            .unwrap(),
        Succession::Died {
            predecessor: PersonId(1)
        }
    );
    assert_eq!(runtime.current(&store), None);
    assert_eq!(
        runtime.waiting(&store),
        Some(Waiting {
            predecessor: PersonId(1),
            died_at: SimTime(5)
        })
    );
    assert!(guardians(&store).all(|(_, guardian)| guardian.target.is_none()));
    let encoded = serde_json::to_value(&runtime).unwrap();
    assert_eq!(encoded.as_object().unwrap().len(), 2);
    assert!(encoded.get("incarnations").is_none());
    assert!(encoded.get("waiting").is_none());
    assert!(encoded.get("guardians").is_none());
    let restored: Runtime = serde_json::from_value(encoded).unwrap();
    let mut resumed = Store::restore(
        store.schema().clone(),
        serde_json::from_slice(&serde_json::to_vec(&store.snapshot().unwrap()).unwrap()).unwrap(),
    )
    .unwrap();
    let births = [birth(11, 6), birth(10, 6)];
    let expected = Succession::Bound {
        predecessor: PersonId(1),
        successor: PersonId(10),
    };
    assert_eq!(
        runtime
            .prepare_birth(&store, &births, SimTime(6), &BTreeMap::new())
            .unwrap()
            .commit(&runtime, &mut store)
            .unwrap(),
        expected
    );
    assert_eq!(
        restored
            .prepare_birth(
                &resumed,
                &[births[1].clone(), births[0].clone()],
                SimTime(6),
                &BTreeMap::new()
            )
            .unwrap()
            .commit(&restored, &mut resumed)
            .unwrap(),
        expected
    );
    assert_eq!(store, resumed);
    assert_eq!(runtime, metadata);
    runtime.validate(&store, SimTime(6)).unwrap();
    assert_eq!(runtime.current(&store), Some(PersonId(10)));
    assert_eq!(runtime.waiting(&store), None);
    assert!(
        guardians(&store).all(|(_, guardian)| guardian.target == Some(PersonId(10))
            && guardian.sensed_location == Some(Location::AtArea(AreaId(8))))
    );
}

#[test]
fn canonical_assessment_decides_requests_and_guards_other_owner_facts() {
    let (runtime, mut store) = fixture();
    let request = GuardianRequest::Move {
        destination: AreaId(8),
    };
    let refusal = runtime
        .prepare_request(
            &store,
            PersonId(2),
            PersonId(1),
            request.clone(),
            request_facts(1),
        )
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert_eq!(refusal, RequestDecision::Refused(Refusal::Unready.reason()));
    for (conduct, at) in [
        (Conduct::Meal, 2),
        (Conduct::Production, 3),
        (Conduct::Care, 4),
    ] {
        runtime
            .prepare_conduct(
                &store,
                PersonId(2),
                bold(),
                Observation {
                    conduct,
                    stage: LifeStage::Adult,
                    at: SimTime(at),
                    witnessed: true,
                },
            )
            .unwrap()
            .commit(&runtime, &mut store)
            .unwrap();
    }
    assert!(guardian(&store, PersonId(2)).unwrap().assessments[&PersonId(1)].ready);
    store.insert(StableId(1), 2_u64).unwrap();
    let mut facts = ReadSet::new();
    facts.read::<u64>(&store, StableId(1));
    let prepared = runtime
        .prepare_request(
            &store,
            PersonId(2),
            PersonId(1),
            request.clone(),
            request_facts(5),
        )
        .unwrap()
        .with_factual_reads(facts);
    store.insert(StableId(1), 3_u64).unwrap();
    let before = store.snapshot().unwrap();
    assert!(prepared.commit(&runtime, &mut store).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
    assert_eq!(
        runtime
            .prepare_request(
                &store,
                PersonId(2),
                PersonId(1),
                request.clone(),
                request_facts(5)
            )
            .unwrap()
            .commit(&runtime, &mut store)
            .unwrap(),
        RequestDecision::Accepted
    );
    assert_eq!(
        guardian(&store, PersonId(2))
            .unwrap()
            .assignment
            .as_ref()
            .unwrap()
            .request,
        request
    );
    assert!(
        guardian(&store, PersonId(3))
            .unwrap()
            .assessments
            .is_empty()
    );
}

#[test]
fn canonical_succession_rejects_changed_guardians_and_invalid_foreign_rows_atomically() {
    let (runtime, mut store) = fixture();
    let prepared = runtime
        .prepare_death(&store, false, SimTime(5), &BTreeMap::new())
        .unwrap();
    let mut batch = Batch::new();
    let staged = prepared.stage_into(&runtime, &store, &mut batch).unwrap();
    assert!(staged.validate(&runtime, &store, &Batch::new()).is_err());
    batch.insert(StableId(1), 11_u64);
    staged.validate(&runtime, &store, &batch).unwrap();
    let before = store.snapshot().unwrap();
    assert!(store.apply_batch(batch).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
    let prepared = runtime
        .prepare_death(&store, false, SimTime(5), &BTreeMap::new())
        .unwrap();
    store.insert(StableId(10), founder()).unwrap();
    let before = store.snapshot().unwrap();
    assert!(prepared.commit(&runtime, &mut store).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
    let prepared = runtime
        .prepare_conduct(
            &store,
            PersonId(2),
            bold(),
            Observation {
                conduct: Conduct::Meal,
                stage: LifeStage::Adult,
                at: SimTime(2),
                witnessed: true,
            },
        )
        .unwrap();
    let mut batch = Batch::new();
    let staged = prepared.stage_into(&runtime, &store, &mut batch).unwrap();
    batch.insert(StableId(2), founder());
    assert!(staged.validate(&runtime, &store, &batch).is_err());
}

#[test]
fn canonical_care_and_learning_rows_are_updated_by_owner_effects() {
    let (runtime, mut store) = fixture();
    let need = NeedObservation {
        location: Location::AtArea(AreaId(7)),
        observed_at: SimTime(2),
        urgency: 100,
        hydration_urgency: 0,
        preference: None,
    };
    let facts = ProvisionFacts {
        now: SimTime(2),
        together: true,
        applicable: Some(true),
        need: Some(&need),
        doses: CareDoses {
            food: Quantity(1_000),
            water: Quantity(250),
        },
    };
    let provision = runtime
        .prepare_care_provision(&store, PersonId(2), facts)
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap()
        .unwrap();
    assert_eq!(provision.delivery.requirement.good, Good::Food);
    assert!(
        runtime
            .prepare_care_receipt(
                &store,
                PersonId(2),
                PersonId(1),
                Good::Food,
                Quantity(500),
                1
            )
            .unwrap()
            .commit(&runtime, &mut store)
            .unwrap()
    );
    assert_eq!(
        guardian(&store, PersonId(2))
            .unwrap()
            .provision
            .as_ref()
            .unwrap()
            .delivery
            .remaining,
        Quantity(500)
    );
    assert_eq!(
        runtime
            .prepare_learning(&store, PersonId(1), 1_100)
            .unwrap()
            .commit(&runtime, &mut store)
            .unwrap(),
        1_000
    );
    assert_eq!(invocation_learning(&store, PersonId(1)), 1_000);
    assert!(runtime.invocation_ready(&store, PersonId(1)));
    assert_eq!(
        runtime
            .prepare_learning(&store, PersonId(1), 1)
            .unwrap()
            .commit(&runtime, &mut store)
            .unwrap(),
        0
    );
}

#[test]
fn canonical_review_owns_priority_and_assignment_completion() {
    let (runtime, mut store) = fixture();
    runtime
        .prepare_request(
            &store,
            PersonId(2),
            PersonId(1),
            GuardianRequest::Rest,
            request_facts(1),
        )
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    let location = Location::AtArea(AreaId(7));
    let facts = ReviewFacts {
        now: SimTime(2),
        location: &location,
        activity: GuardianActivity::Idle,
        confined: true,
        incapacitated: false,
        combat_restricted: false,
        in_combat: false,
        serving_objective: false,
    };
    let decision = runtime
        .prepare_review(
            &store,
            PersonId(2),
            facts,
            Some(location.clone()),
            true,
            None,
        )
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert_eq!(decision.step, ReviewStep::Escape);
    assert_eq!(decision.action, Some(ReviewAction::Escape));
    let guardian = guardian(&store, PersonId(2)).unwrap();
    assert_eq!(guardian.goal, GuardianGoal::Confined);
    assert!(guardian.assignment.is_none());
    assert_eq!(guardian.target, Some(PersonId(1)));
}

#[test]
fn initialization_and_intrinsic_save_validation_reject_invalid_rows_before_publication() {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    store.spawn(StableId(1)).unwrap();
    let before = store.snapshot().unwrap();
    assert!(
        Runtime::initialize(
            &mut store,
            StableId(1),
            MentorshipRules::default(),
            History {
                incarnations: Vec::new()
            },
            []
        )
        .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
    let history = History {
        incarnations: vec![Incarnation {
            person: PersonId(1),
            bound_at: SimTime(0),
            died_at: None,
        }],
    };
    let mut bad = founder();
    bad.target = None;
    assert!(
        Runtime::initialize(
            &mut store,
            StableId(1),
            MentorshipRules::default(),
            history,
            [(PersonId(1), bad)]
        )
        .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
    let (runtime, store) = fixture();
    let mut malformed = store.snapshot().unwrap();
    let history_row = malformed.columns.get_mut("lineage/history").unwrap();
    history_row[0].value = serde_json::value::to_raw_value(&History {
        incarnations: Vec::new(),
    })
    .unwrap();
    assert!(Store::restore(store.schema().clone(), malformed).is_err());
    runtime.validate(&store, SimTime(0)).unwrap();
}

#[test]
fn canonical_named_supply_uses_conserved_receipts_and_survives_restore() {
    let (runtime, mut store) = fixture();
    let facts = SupplyFacts {
        now: SimTime(1),
        requester_independent: true,
        requester_available: true,
        fresh_contact: true,
        guardian_capable: true,
        conversation_available: true,
        health_busy: false,
        known_source: Some(StockKnowledge {
            inventory: InventoryId(30),
            good: Good::Food,
            quantity: Quantity(1_000),
            accessible: true,
            area: AreaId(7),
            observed_at: SimTime(1),
            learned_at: SimTime(1),
            source: KnowledgeSource::Observed(PersonId(2)),
        }),
        source_route_known: true,
        immediate_care: false,
        requester_area: Some(AreaId(7)),
        source_carried: true,
        room: Quantity(2_000),
        carrying_capacity: Quantity(2_000),
        cursor: 0,
    };
    assert!(matches!(
        runtime
            .prepare_supply(
                &store,
                PersonId(2),
                PersonId(1),
                Good::Food,
                InventoryId(30),
                Quantity(1_000),
                &facts
            )
            .unwrap()
            .commit(&runtime, &mut store)
            .unwrap(),
        SupplyAnswer::Accept(_)
    ));
    assert_eq!(
        runtime.requested_delivery_quantity(&store, PersonId(2), Good::Food, Some(InventoryId(30))),
        Some(Quantity(1_000))
    );
    let receipt = SupplyReceipt::Transfer {
        actor: PersonId(2),
        source: InventoryId(30),
        destination: InventoryId(31),
        good: Good::Food,
        quantity: Quantity(400),
        destination_holder: Some(PersonId(1)),
    };
    assert_eq!(
        runtime
            .prepare_supply_receipts(&store, PersonId(2), &[(EventId(1), receipt)], 1)
            .unwrap()
            .commit(&runtime, &mut store)
            .unwrap(),
        Quantity(400)
    );
    let mut resumed = Store::restore(store.schema().clone(), store.snapshot().unwrap()).unwrap();
    for store in [&mut store, &mut resumed] {
        let receipt = SupplyReceipt::Transfer {
            actor: PersonId(2),
            source: InventoryId(30),
            destination: InventoryId(31),
            good: Good::Food,
            quantity: Quantity(600),
            destination_holder: Some(PersonId(1)),
        };
        assert_eq!(
            runtime
                .prepare_supply_receipts(store, PersonId(2), &[(EventId(2), receipt)], 2)
                .unwrap()
                .commit(&runtime, store)
                .unwrap(),
            Quantity(600)
        );
    }
    assert_eq!(store, resumed);
    assert_eq!(
        runtime.requested_delivery_quantity(&store, PersonId(2), Good::Food, None),
        Some(Quantity(0))
    );
}

struct References {
    people: Vec<PersonReference>,
    deaths: Vec<(PersonId, SimTime)>,
}
impl LineageReferences for References {
    fn people(&self) -> impl Iterator<Item = PersonReference> {
        self.people.iter().copied()
    }
    fn guardian_people(&self) -> impl Iterator<Item = PersonId> {
        [PersonId(2), PersonId(3)].into_iter()
    }
    fn recorded_death(&self, person: PersonId, at: SimTime) -> bool {
        self.deaths.contains(&(person, at))
    }
    fn stock(&self, _: InventoryId) -> Option<StockReference> {
        None
    }
    fn area_exists(&self, id: AreaId) -> bool {
        [AreaId(7), AreaId(8)].contains(&id)
    }
    fn site_exists(&self, _: SiteId) -> bool {
        false
    }
}
#[test]
fn canonical_save_reference_validation_owns_historical_birth_selection() {
    let (runtime, mut store) = fixture();
    runtime
        .prepare_death(&store, false, SimTime(5), &BTreeMap::new())
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    runtime
        .prepare_birth(&store, &[birth(10, 6)], SimTime(6), &BTreeMap::new())
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    let mut facts = References {
        people: [PersonId(1), PersonId(2), PersonId(3), PersonId(10)]
            .into_iter()
            .map(|person| PersonReference {
                person,
                born: if person == PersonId(10) {
                    SimTime(6)
                } else {
                    SimTime(0)
                },
                initial_age: if person == PersonId(10) {
                    SimDuration(0)
                } else {
                    SimDuration(10)
                },
                parent_count: 2,
                alive: person != PersonId(1),
            })
            .collect(),
        deaths: vec![(PersonId(1), SimTime(5))],
    };
    runtime
        .validate_references(&store, SimTime(6), 1, PersonId(10), &facts)
        .unwrap();
    facts.deaths.clear();
    assert!(
        runtime
            .validate_references(&store, SimTime(6), 1, PersonId(10), &facts)
            .is_err()
    );
    facts.deaths.push((PersonId(1), SimTime(5)));
    facts.people.push(PersonReference {
        person: PersonId(9),
        born: SimTime(6),
        initial_age: SimDuration(0),
        parent_count: 2,
        alive: true,
    });
    assert!(
        runtime
            .validate_references(&store, SimTime(6), 1, PersonId(10), &facts)
            .is_err()
    );
}

#[test]
fn canonical_succession_and_character_roles_publish_in_one_batch() {
    let (runtime, mut store) = fixture();
    for (actor, role) in [
        (1, Role::Ruler),
        (2, Role::Guardian),
        (3, Role::Guardian),
        (10, Role::Citizen),
        (11, Role::Citizen),
    ] {
        game_characters::people::insert_person(
            &mut store,
            game_characters::people::Person {
                id: PersonId(actor),
                role,
                inventory: InventoryId(actor + 200),
                work_units: 17,
                work_remainder: 0,
                work_rate: 1_000,
                hunger_exempt: role == Role::Guardian,
            },
        )
        .unwrap();
    }
    runtime
        .prepare_death(&store, false, SimTime(5), &BTreeMap::new())
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    let before = store.snapshot().unwrap();
    for reject in [true, false] {
        let prepared = runtime
            .prepare_birth(&store, &[birth(10, 6)], SimTime(6), &BTreeMap::new())
            .unwrap();
        let Succession::Bound { .. } = *prepared.output() else {
            panic!("actual post-death successor");
        };
        let roles = game_characters::people::prepare_roles(
            &store,
            prepared
                .output()
                .role_changes()
                .into_iter()
                .map(|(actor, role)| game_characters::people::RoleChange { actor, role }),
        )
        .unwrap();
        let mut batch = Batch::new();
        let lineage = prepared.stage_into(&runtime, &store, &mut batch).unwrap();
        let roles = roles.stage_into(&store, &mut batch).unwrap();
        if reject {
            batch.insert(StableId(10), 11_u64);
        }
        lineage.validate(&runtime, &store, &batch).unwrap();
        roles.validate(&store, &batch).unwrap();
        if reject {
            assert!(store.apply_batch(batch).is_err());
            assert_eq!(store.snapshot().unwrap(), before);
            assert_eq!(
                game_characters::people::person(&store, PersonId(1))
                    .unwrap()
                    .role,
                Role::Ruler
            );
            assert_eq!(runtime.current(&store), None);
        } else {
            store.apply_batch(batch).unwrap();
            assert_eq!(
                lineage.finish(),
                Succession::Bound {
                    predecessor: PersonId(1),
                    successor: PersonId(10)
                }
            );
        }
    }
    assert_eq!(runtime.current(&store), Some(PersonId(10)));
    assert_eq!(
        game_characters::people::person(&store, PersonId(1))
            .unwrap()
            .role,
        Role::Citizen
    );
    let child = game_characters::people::person(&store, PersonId(10)).unwrap();
    assert_eq!(child.role, Role::Ruler);
    assert_eq!((child.inventory, child.work_units), (InventoryId(210), 17));
    let resumed = Store::restore(store.schema().clone(), store.snapshot().unwrap()).unwrap();
    assert_eq!(store, resumed);
    assert_eq!(runtime.current(&resumed), Some(PersonId(10)));
}

fn participant(actor: u64) -> ParticipantFacts {
    ParticipantFacts {
        person: PersonId(actor),
        location: Location::AtArea(AreaId(7)),
        alive: true,
        health: 100,
        idle: true,
        independent: true,
        in_combat: false,
        restricted: false,
        in_care: false,
    }
}
fn teacher(actor: u64) -> TeacherFacts {
    TeacherFacts {
        participant: participant(actor),
        craft_level: 1,
        craft_interest: 0,
        knows_routes: true,
        signal_urgency: 0,
        stocks: vec![StockKnowledge {
            inventory: InventoryId(30),
            good: Good::Food,
            quantity: Quantity(1_000),
            accessible: true,
            area: AreaId(7),
            observed_at: SimTime(0),
            learned_at: SimTime(0),
            source: KnowledgeSource::Observed(PersonId(actor)),
        }],
        destinations: vec![(AreaId(8), true), (AreaId(7), true)],
        sites: vec![SiteObservation {
            site: SiteId(40),
            area: AreaId(7),
            access: true,
        }],
    }
}
#[test]
fn canonical_witness_selection_and_available_commands_are_owner_policy() {
    let (runtime, mut store) = fixture();
    for (conduct, at) in [
        (WitnessConduct::Meal, 1),
        (WitnessConduct::Production, 2),
        (WitnessConduct::Care, 3),
    ] {
        let observers = [
            WitnessFacts {
                actor: PersonId(2),
                location: Location::AtArea(AreaId(7)),
                health: 100,
                traits: bold(),
                signal_urgency: 0,
            },
            WitnessFacts {
                actor: PersonId(3),
                location: Location::AtArea(AreaId(8)),
                health: 100,
                traits: bold(),
                signal_urgency: 0,
            },
        ];
        runtime
            .prepare_witness(
                &store,
                WitnessEvent {
                    actor: PersonId(1),
                    conduct,
                    departure: None,
                },
                &Location::AtArea(AreaId(7)),
                LifeStage::Adult,
                SimTime(at),
                observers,
            )
            .unwrap()
            .commit(&runtime, &mut store)
            .unwrap();
    }
    assert!(guardian(&store, PersonId(2)).unwrap().assessments[&PersonId(1)].ready);
    assert!(
        guardian(&store, PersonId(3))
            .unwrap()
            .assessments
            .is_empty()
    );
    let known = std::collections::BTreeSet::from([InventoryId(30)]);
    let forward =
        runtime.available_commands(&store, &participant(1), &known, [teacher(2), teacher(3)]);
    let mut reversed = teacher(2);
    reversed.destinations.reverse();
    reversed.stocks.reverse();
    reversed.sites.reverse();
    assert_eq!(
        forward,
        runtime.available_commands(&store, &participant(1), &known, [teacher(3), reversed])
    );
    assert!(forward.contains(&AvailableCommand::Request {
        guardian: PersonId(2),
        request: GuardianRequest::Move {
            destination: AreaId(8)
        }
    }));
    assert!(!forward.contains(&AvailableCommand::Request {
        guardian: PersonId(3),
        request: GuardianRequest::Move {
            destination: AreaId(8)
        }
    }));
    let mut facts = participant(1);
    facts.health = 0;
    assert!(
        runtime
            .available_commands(&store, &facts, &known, [teacher(2)])
            .is_empty()
    );
}
#[test]
fn canonical_lesson_start_and_finish_use_the_owned_rules_and_actual_participants() {
    let (runtime, mut store) = fixture();
    let mut facts = teacher(2);
    facts.signal_urgency = 20_000;
    let refused = runtime
        .prepare_lesson_start(
            &store,
            &participant(1),
            &facts,
            LessonTopic::Invocation,
            SimTime(1),
        )
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert!(matches!(refused, LessonDecision::Refused(_)));
    facts.signal_urgency = 0;
    let prepared = runtime
        .prepare_lesson_start(
            &store,
            &participant(1),
            &facts,
            LessonTopic::Invocation,
            SimTime(2),
        )
        .unwrap();
    let expected = SimTime(2 + runtime.rules.lesson_duration.0);
    assert_eq!(
        prepared.output(),
        &LessonDecision::Begin {
            teacher: PersonId(2),
            student: PersonId(1),
            topic: LessonTopic::Invocation,
            completes: expected
        }
    );
    prepared.commit(&runtime, &mut store).unwrap();
    let lesson = LessonProgress {
        teacher: PersonId(2),
        student: PersonId(1),
        topic: LessonTopic::Invocation,
        completes: expected,
    };
    assert!(runtime.lesson_can_finish(lesson, &participant(2), &participant(1), true, expected));
    assert!(!runtime.lesson_can_finish(lesson, &participant(2), &participant(1), false, expected));
    assert!(!runtime.lesson_can_finish(lesson, &participant(3), &participant(1), true, expected));
    assert!(!runtime.lesson_can_finish(
        lesson,
        &participant(2),
        &participant(1),
        true,
        SimTime(expected.0 - 1)
    ));
}

#[test]
fn canonical_guardian_view_preserves_waiting_spectator_visibility() {
    let (runtime, mut store) = fixture();
    let local = std::collections::BTreeSet::from([PersonId(2)]);
    assert_eq!(
        runtime
            .visible_guardians(&store, &local)
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        vec![PersonId(2)]
    );
    runtime
        .prepare_death(&store, false, SimTime(5), &BTreeMap::new())
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert_eq!(
        runtime
            .visible_guardians(&store, &local)
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        vec![PersonId(2), PersonId(3)]
    );
}

#[test]
fn reserved_successor_requires_same_batch_child_admission_and_survives_restore() {
    let (runtime, mut store) = fixture();
    runtime
        .prepare_death(&store, false, SimTime(5), &BTreeMap::new())
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    let before = store.snapshot().unwrap();
    let prepared = runtime
        .prepare_reserved_birth(&store, &[birth(12, 6)], SimTime(6), &BTreeMap::new())
        .unwrap();
    assert!(prepared.commit(&runtime, &mut store).is_err());
    assert_eq!(store.snapshot().unwrap(), before);

    for reject in [true, false] {
        let prepared = runtime
            .prepare_reserved_birth(&store, &[birth(12, 6)], SimTime(6), &BTreeMap::new())
            .unwrap();
        let roles = prepared.output().role_changes();
        let child_role = roles
            .iter()
            .find(|(actor, _)| *actor == PersonId(12))
            .unwrap()
            .1;
        let child =
            game_characters::people::prepare_newborn_person(&store, PersonId(12), InventoryId(212))
                .unwrap()
                .with_role(child_role);
        let mut batch = Batch::new();
        let lineage = prepared.stage_into(&runtime, &store, &mut batch).unwrap();
        assert!(lineage.validate(&runtime, &store, &batch).is_err());
        let child = child.stage_into(&store, &mut batch).unwrap();
        lineage.validate(&runtime, &store, &batch).unwrap();
        child.validate(&store, &batch).unwrap();
        if reject {
            batch.insert(StableId(12), 11_u64);
        }
        if reject {
            assert!(store.apply_batch(batch).is_err());
            assert_eq!(store.snapshot().unwrap(), before);
        } else {
            store.apply_batch(batch).unwrap();
            assert_eq!(
                lineage.finish(),
                Succession::Bound {
                    predecessor: PersonId(1),
                    successor: PersonId(12)
                }
            );
        }
    }
    assert_eq!(runtime.current(&store), Some(PersonId(12)));
    assert_eq!(
        game_characters::people::person(&store, PersonId(12))
            .unwrap()
            .role,
        Role::Ruler
    );
    let restored = Store::restore(store.schema().clone(), store.snapshot().unwrap()).unwrap();
    assert_eq!(restored, store);
    assert_eq!(runtime.current(&restored), Some(PersonId(12)));
    assert!(guardians(&restored).all(|(_, guardian)| guardian.target == Some(PersonId(12))));
}

#[test]
fn ordered_production_evidence_then_death_publish_once_and_roll_back_together() {
    let (runtime, mut store) = fixture();
    let before = store.snapshot().unwrap();
    for reject in [true, false] {
        let witness = runtime
            .prepare_witness(
                &store,
                WitnessEvent {
                    actor: PersonId(1),
                    conduct: WitnessConduct::Production,
                    departure: None,
                },
                &Location::AtArea(AreaId(7)),
                LifeStage::Adult,
                SimTime(5),
                [WitnessFacts {
                    actor: PersonId(2),
                    location: Location::AtArea(AreaId(7)),
                    health: 100,
                    traits: bold(),
                    signal_urgency: 0,
                }],
            )
            .unwrap();
        let combined = witness
            .with_death(&runtime, &store, false, SimTime(5), &BTreeMap::new())
            .unwrap();
        assert_eq!(
            *combined.output(),
            Succession::Died {
                predecessor: PersonId(1)
            }
        );
        let mut batch = Batch::new();
        let staged = combined.stage_into(&runtime, &store, &mut batch).unwrap();
        staged.validate(&runtime, &store, &batch).unwrap();
        if reject {
            batch.insert(StableId(1), 11_u64);
        }
        if reject {
            assert!(store.apply_batch(batch).is_err());
            assert_eq!(store.snapshot().unwrap(), before);
        } else {
            store.apply_batch(batch).unwrap();
            staged.finish();
        }
    }
    let row = guardian(&store, PersonId(2)).unwrap();
    assert_eq!(row.assessments[&PersonId(1)].competence, 100);
    assert_eq!(row.target, None);
    assert_eq!(row.goal, GuardianGoal::Waiting);
    assert_eq!(runtime.current(&store), None);
    assert_eq!(runtime.waiting(&store).unwrap().died_at, SimTime(5));
}

#[test]
fn campaign_founder_anchor_preserves_person_rows_identity_stream_and_save_continuation() {
    let (_, template) = fixture();
    let mut store = Store::with_schema(template.schema().clone());
    let person = game_characters::people::Person {
        id: PersonId(1),
        role: Role::Ruler,
        inventory: InventoryId(201),
        work_units: 17,
        work_remainder: 0,
        work_rate: 1000,
        hunger_exempt: false,
    };
    game_characters::people::insert_person(&mut store, person.clone()).unwrap();
    for id in [2, 3] {
        store.spawn(StableId(id)).unwrap();
    }
    let before_ids = store.ids().collect::<Vec<_>>();
    let history = History {
        incarnations: vec![Incarnation {
            person: PersonId(1),
            bound_at: SimTime(0),
            died_at: None,
        }],
    };
    let before = store.snapshot().unwrap();
    assert!(
        Runtime::initialize(
            &mut store,
            StableId(2),
            MentorshipRules::default(),
            history.clone(),
            [(PersonId(2), founder())]
        )
        .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
    let runtime = Runtime::initialize(
        &mut store,
        StableId(1),
        MentorshipRules::default(),
        history,
        [(PersonId(2), founder())],
    )
    .unwrap();
    assert_eq!(store.ids().collect::<Vec<_>>(), before_ids);
    assert_eq!(
        game_characters::people::person(&store, PersonId(1)),
        Some(&person)
    );
    runtime
        .prepare_death(&store, false, SimTime(5), &BTreeMap::new())
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    runtime
        .prepare_birth(&store, &[birth(3, 6)], SimTime(6), &BTreeMap::new())
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert_eq!(runtime.history, StableId(1));
    assert_eq!(runtime.current(&store), Some(PersonId(3)));
    assert_eq!(
        game_characters::people::person(&store, PersonId(1)),
        Some(&person)
    );
    let restored = Store::restore(store.schema().clone(), store.snapshot().unwrap()).unwrap();
    runtime.validate(&restored, SimTime(6)).unwrap();
    assert_eq!(runtime.current(&restored), Some(PersonId(3)));
    let mut wrong_anchor = restored.snapshot().unwrap();
    wrong_anchor.columns.get_mut("lineage/history").unwrap()[0].entity = 2;
    let moved = Store::restore(restored.schema().clone(), wrong_anchor).unwrap();
    let wrong_runtime = Runtime {
        history: StableId(2),
        ..runtime
    };
    assert!(wrong_runtime.validate(&moved, SimTime(6)).is_err());
}

#[test]
fn canonical_guardian_review_owns_activity_eligibility_for_rest_company_and_waiting() {
    for activity in [
        GuardianActivity::Idle,
        GuardianActivity::Rest,
        GuardianActivity::Work(SiteId(20)),
        GuardianActivity::Other,
    ] {
        for company in [false, true] {
            let (runtime, mut store) = fixture();
            let location = Location::AtArea(AreaId(7));
            let facts = ReviewFacts {
                now: SimTime(10),
                location: &location,
                activity,
                confined: false,
                incapacitated: false,
                combat_restricted: false,
                in_combat: false,
                serving_objective: false,
            };
            let need = company.then_some(NeedObservation {
                location: location.clone(),
                observed_at: SimTime(10),
                urgency: 0,
                hydration_urgency: 0,
                preference: Some(ChildhoodInterest::Company),
            });
            let plan = runtime
                .prepare_review(
                    &store,
                    PersonId(2),
                    facts,
                    Some(location.clone()),
                    false,
                    Some(AssistanceFacts {
                        target_location: &location,
                        need,
                        provision: None,
                        transport_rescue: false,
                        force_rescue: false,
                    }),
                )
                .unwrap();
            let expected = match (activity, company) {
                (GuardianActivity::Other, _) => None,
                (_, true) => Some(ReviewAction::Physical(GuardianAction::Comfort {
                    child: PersonId(1),
                })),
                (GuardianActivity::Idle | GuardianActivity::Work(_), false) => {
                    Some(ReviewAction::Physical(GuardianAction::Rest))
                }
                (GuardianActivity::Rest, false) => None,
            };
            assert_eq!(plan.output().action, expected);
            plan.commit(&runtime, &mut store).unwrap();
            runtime
                .prepare_death(&store, false, SimTime(11), &BTreeMap::new())
                .unwrap()
                .commit(&runtime, &mut store)
                .unwrap();
            let waiting = runtime
                .prepare_review(
                    &store,
                    PersonId(2),
                    ReviewFacts {
                        now: SimTime(12),
                        ..facts
                    },
                    None,
                    false,
                    None,
                )
                .unwrap();
            let rest = matches!(activity, GuardianActivity::Idle | GuardianActivity::Work(_));
            assert_eq!(waiting.output().step, ReviewStep::Wait { rest });
            assert_eq!(
                waiting.output().action,
                rest.then_some(ReviewAction::Physical(GuardianAction::Rest))
            );
            waiting.commit(&runtime, &mut store).unwrap();
            assert_eq!(
                guardian(&store, PersonId(2)).unwrap().goal,
                GuardianGoal::Waiting
            );
        }
    }
}
