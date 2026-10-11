use crate::*;
use game_core::*;
use std::collections::{BTreeMap, BTreeSet};

fn schema() -> game_ecs::Schema {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    schema
}
fn fixture() -> (Runtime, Store) {
    let mut store = Store::with_schema(schema());
    insert_provenance(
        &mut store,
        InventoryId(10),
        Provenance {
            portions: BTreeMap::from([(ClaimId(0), 1_000)]),
        },
    )
    .unwrap();
    insert_provenance(&mut store, InventoryId(11), Provenance::default()).unwrap();
    insert_attitude(
        &mut store,
        PersonId(1),
        Attitude {
            exclusive_claims: 900,
            sharing: 100,
            willingness_to_defend: 900,
        },
    )
    .unwrap();
    insert_attitude(
        &mut store,
        PersonId(2),
        Attitude {
            exclusive_claims: 900,
            sharing: 900,
            willingness_to_defend: 900,
        },
    )
    .unwrap();
    (Runtime::default(), store)
}
fn snapshot(runtime: &Runtime, store: &Store) -> (Runtime, game_ecs::Snapshot) {
    (runtime.clone(), store.snapshot().unwrap())
}
fn actor(id: u64, now: u64) -> ActorFacts {
    ActorFacts {
        actor: PersonId(id),
        now: SimTime(now),
        area: Some(AreaId(5)),
        capable: true,
        exertion_ready: true,
    }
}
fn source() -> StockFacts {
    StockFacts {
        id: InventoryId(10),
        area: Some(AreaId(5)),
        good: Good::Timber,
        quantity: Quantity(1_000),
        room: Quantity(1_000),
        carried_by: None,
        actor_allowed: false,
        custodian: Some(PersonId(2)),
    }
}
fn destination() -> StockFacts {
    StockFacts {
        id: InventoryId(11),
        quantity: Quantity(0),
        room: Quantity(3_000),
        carried_by: Some(PersonId(1)),
        actor_allowed: true,
        custodian: Some(PersonId(1)),
        ..source()
    }
}
fn contact(id: u64) -> ContactFacts {
    ContactFacts {
        actor: PersonId(id),
        area: Some(AreaId(5)),
        present: true,
        capable: true,
        available: true,
        autonomous: true,
    }
}
fn taking(world: &mut Runtime, store: &mut Store) {
    world
        .prepare_action(
            store,
            actor(1, 0),
            &PropertyAction::Take {
                source: InventoryId(10),
                destination: InventoryId(11),
                quantity: Quantity(500),
            },
            ActionFacts::Take {
                source: source(),
                destination: destination(),
                id: IncidentId(30),
                carrying_room: Quantity(3_000),
                duration: SimDuration(100),
            },
            None,
        )
        .unwrap()
        .commit(world, store)
        .unwrap();
}
#[test]
fn competing_assertions_and_personal_recognition_never_transfer_goods_or_custody() {
    let (mut world, mut store) = fixture();
    let rights = BTreeSet::from([ClaimRight::Transfer, ClaimRight::GrantAccess]);
    for (person, id) in [(1, 20), (2, 21)] {
        world
            .prepare_action(
                &store,
                actor(person, 0),
                &PropertyAction::Claim {
                    inventory: InventoryId(10),
                    rights: rights.clone(),
                    evidence: vec![],
                },
                ActionFacts::Claim {
                    stock: source(),
                    id: ClaimId(id),
                    witnesses: vec![PersonId(1), PersonId(2)],
                },
                None,
            )
            .unwrap()
            .commit(&mut world, &mut store)
            .unwrap();
    }
    assert_eq!(
        claim(&store, ClaimId(20)).unwrap().portions,
        claim(&store, ClaimId(21)).unwrap().portions
    );
    let effects = world
        .prepare_action(
            &store,
            actor(1, 1),
            &PropertyAction::Recognize { claim: ClaimId(21) },
            ActionFacts::Recognition {
                witnesses: vec![PersonId(1)],
            },
            None,
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::Permission { .. } | Effect::Transfer { .. }))
    );
    assert_eq!(
        portions(&store, InventoryId(10))
            .unwrap()
            .values()
            .sum::<u64>(),
        1_000
    );
    assert!(
        world
            .prepare_action(
                &store,
                actor(1, 2),
                &PropertyAction::Share {
                    inventory: InventoryId(10),
                    recipient: PersonId(1),
                    allowed: true
                },
                ActionFacts::Share {
                    stock: source(),
                    recipient: contact(1)
                },
                None
            )
            .is_err()
    );
}
#[test]
fn a_real_custodian_can_refuse_or_share_without_a_claim_becoming_global_law() {
    let (mut world, mut store) = fixture();
    let action = PropertyAction::RequestAccess {
        inventory: InventoryId(10),
    };
    let denied = world
        .prepare_action(
            &store,
            actor(1, 0),
            &action,
            ActionFacts::Request {
                stock: source(),
                custodian: contact(2),
                trust: 0,
                reserve: Quantity(2_000),
            },
            None,
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    assert!(denied.contains(&Effect::Refused {
        actor: PersonId(1),
        custodian: PersonId(2),
        inventory: InventoryId(10)
    }));
    let shared = world
        .prepare_action(
            &store,
            actor(1, 1),
            &action,
            ActionFacts::Request {
                stock: source(),
                custodian: contact(2),
                trust: 0,
                reserve: Quantity(1_000),
            },
            None,
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    assert!(shared.contains(&Effect::Permission {
        actor: PersonId(2),
        inventory: InventoryId(10),
        recipient: PersonId(1),
        allowed: true
    }));
    assert_eq!(
        portions(&store, InventoryId(10)).unwrap()[&ClaimId(0)],
        1_000
    );
}
#[test]
fn taking_requires_real_time_and_returns_one_transfer_while_receipt_moves_provenance_once() {
    let (mut world, mut store) = fixture();
    taking(&mut world, &mut store);
    assert!(
        world
            .prepare_finish_take(
                &store,
                actor(1, 99),
                source(),
                destination(),
                Quantity(3_000),
                None
            )
            .is_err()
    );
    let effects = world
        .prepare_finish_take(
            &store,
            actor(1, 100),
            source(),
            destination(),
            Quantity(3_000),
            None,
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    assert_eq!(
        effects
            .iter()
            .filter(|e| matches!(e, Effect::Transfer { .. }))
            .count(),
        1
    );
    assert_eq!(
        incident(&store, IncidentId(30)).unwrap().outcome,
        IncidentOutcome::Taken
    );
    assert_eq!(
        portions(&store, InventoryId(10)).unwrap()[&ClaimId(0)],
        1_000
    );
    let balances = BTreeMap::from([
        (InventoryId(10), Quantity(500)),
        (InventoryId(11), Quantity(500)),
    ]);
    world
        .prepare_receipts(
            &store,
            &[PropertyReceipt {
                event: 1,
                change: ReceiptChange::Transfer {
                    source: InventoryId(10),
                    destination: InventoryId(11),
                    quantity: Quantity(500),
                },
            }],
            &balances,
            2,
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    assert_eq!(portions(&store, InventoryId(10)).unwrap()[&ClaimId(0)], 500);
    assert_eq!(portions(&store, InventoryId(11)).unwrap()[&ClaimId(0)], 500);
    world.validate_owned(&store, SimTime(100)).unwrap();
}
#[test]
fn insufficient_or_stale_provenance_rejects_the_whole_receipt_batch() {
    let (world, mut store) = fixture();
    let before = snapshot(&world, &store);
    let receipts = vec![
        PropertyReceipt {
            event: 1,
            change: ReceiptChange::Consume {
                inventory: InventoryId(10),
                quantity: Quantity(500),
            },
        },
        PropertyReceipt {
            event: 2,
            change: ReceiptChange::Consume {
                inventory: InventoryId(10),
                quantity: Quantity(600),
            },
        },
    ];
    assert!(
        world
            .prepare_receipts(
                &store,
                &receipts,
                &BTreeMap::from([(InventoryId(10), Quantity(0))]),
                2
            )
            .is_err()
    );
    assert_eq!(snapshot(&world, &store), before);
    let prepared = world
        .prepare_receipts(&store, &[], &BTreeMap::new(), 1)
        .unwrap();
    let mut changed = world;
    changed.last_event = 2;
    let before = snapshot(&changed, &store);
    assert!(prepared.commit(&mut changed, &mut store).is_err());
    assert_eq!(snapshot(&changed, &store), before);
}
#[test]
fn new_cargo_opening_is_reconstructed_and_production_never_inherits_a_title() {
    let (mut world, mut store) = fixture();
    let mut balances = BTreeMap::from([
        (InventoryId(10), Quantity(500)),
        (InventoryId(12), Quantity(500)),
    ]);
    world
        .prepare_receipts(
            &store,
            &[PropertyReceipt {
                event: 1,
                change: ReceiptChange::Transfer {
                    source: InventoryId(10),
                    destination: InventoryId(12),
                    quantity: Quantity(500),
                },
            }],
            &balances,
            1,
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    assert_eq!(portions(&store, InventoryId(12)).unwrap()[&ClaimId(0)], 500);
    balances.insert(InventoryId(12), Quantity(600));
    world
        .prepare_receipts(
            &store,
            &[PropertyReceipt {
                event: 2,
                change: ReceiptChange::Produce {
                    outputs: vec![(InventoryId(12), Quantity(100))],
                },
            }],
            &balances,
            2,
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    assert_eq!(portions(&store, InventoryId(12)).unwrap()[&ClaimId(0)], 600);
}
#[test]
fn observer_choices_require_personal_claims_or_custody_and_defense_is_an_external_force_intent() {
    let (mut world, mut store) = fixture();
    taking(&mut world, &mut store);
    let observer = ObserverFacts {
        actor: PersonId(2),
        personal_custody: true,
        personally_allowed: false,
        can_defend: true,
    };
    world
        .prepare_witness(&store, IncidentId(30), SimTime(0), &[observer])
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    assert_eq!(
        world.resistance(&store, PersonId(1), &[observer]),
        Some(Effect::Engage {
            defender: PersonId(2),
            thief: PersonId(1)
        })
    );
    world
        .prepare_finish_take(
            &store,
            actor(1, 100),
            source(),
            destination(),
            Quantity(3_000),
            None,
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    let effects = world
        .prepare_witness(&store, IncidentId(30), SimTime(100), &[observer])
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    assert!(effects.contains(&Effect::Relationship {
        observer: PersonId(2),
        target: PersonId(1),
        trust: -100
    }));
    let effects = world
        .prepare_witness(&store, IncidentId(30), SimTime(101), &[observer])
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::Relationship { .. }))
    );
}
#[test]
fn report_content_is_frozen_and_cannot_refresh_itself_from_later_sender_truth() {
    let (mut world, mut store) = fixture();
    taking(&mut world, &mut store);
    let witness = ObserverFacts {
        actor: PersonId(2),
        personal_custody: false,
        personally_allowed: false,
        can_defend: false,
    };
    world
        .prepare_witness(&store, IncidentId(30), SimTime(1), &[witness])
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    world
        .prepare_action(
            &store,
            actor(2, 1),
            &PropertyAction::ReportIncident {
                recipient: PersonId(3),
                incident: IncidentId(30),
            },
            ActionFacts::Report {
                recipient: contact(3),
                duration: SimDuration(200),
            },
            None,
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    world
        .prepare_finish_take(
            &store,
            actor(1, 100),
            source(),
            destination(),
            Quantity(3_000),
            None,
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    world
        .prepare_witness(&store, IncidentId(30), SimTime(100), &[witness])
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    assert_eq!(
        known_incidents(&store, PersonId(2)).unwrap()[&IncidentId(30)]
            .incident
            .outcome,
        IncidentOutcome::Taken
    );
    world
        .prepare_finish_report(
            &store,
            actor(2, 201),
            contact(3),
            ObserverFacts {
                actor: PersonId(3),
                ..witness
            },
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    assert_eq!(
        known_incidents(&store, PersonId(3)).unwrap()[&IncidentId(30)]
            .incident
            .outcome,
        IncidentOutcome::Attempt
    );
    assert_eq!(
        known_incidents(&store, PersonId(3)).unwrap()[&IncidentId(30)].source,
        IncidentSource::Reported {
            sender: PersonId(2)
        }
    );
}
#[test]
fn listener_interruption_cancels_the_speakers_frozen_report_and_releases_occupancy() {
    let (mut world, mut store) = fixture();
    taking(&mut world, &mut store);
    let witness = ObserverFacts {
        actor: PersonId(2),
        personal_custody: false,
        personally_allowed: false,
        can_defend: false,
    };
    world
        .prepare_witness(&store, IncidentId(30), SimTime(1), &[witness])
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    world
        .prepare_action(
            &store,
            actor(2, 1),
            &PropertyAction::ReportIncident {
                recipient: PersonId(3),
                incident: IncidentId(30),
            },
            ActionFacts::Report {
                recipient: contact(3),
                duration: SimDuration(100),
            },
            None,
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    assert!(world.actor_talking(&store, PersonId(3)));
    let effects = world
        .prepare_cancel(&store, PersonId(3), SimTime(2))
        .commit(&mut world, &mut store)
        .unwrap();
    assert!(effects.contains(&Effect::ClearReport {
        sender: PersonId(2),
        completes: SimTime(101),
        interrupted: true,
    }));
    assert!(reports(&store).next().is_none());
}
#[test]
fn acquisition_offers_include_permission_and_taking_without_choosing_personal_policy() {
    let (world, store) = fixture();
    let opportunity = NeedOpportunity {
        known: StockKnowledge {
            inventory: InventoryId(10),
            area: AreaId(5),
            good: Good::Timber,
            quantity: Quantity(1_000),
            accessible: false,
            observed_at: SimTime(0),
            learned_at: SimTime(0),
            source: KnowledgeSource::Observed(PersonId(1)),
        },
        local_custodian: true,
        can_take: true,
    };
    let need = AcquisitionFacts {
        actor: actor(1, 0),
        good: Good::Timber,
        destination: Some(InventoryId(11)),
        quantity: Quantity(500),
    };
    assert_eq!(
        world.acquisition_opportunities(&store, need, &[opportunity]),
        vec![
            PropertyAction::RequestAccess {
                inventory: InventoryId(10)
            },
            PropertyAction::Take {
                source: InventoryId(10),
                destination: InventoryId(11),
                quantity: Quantity(500),
            }
        ]
    );
}
#[test]
fn acquisition_offers_require_known_local_stock_capacity_and_a_capable_actor() {
    let (world, store) = fixture();
    for good in [Good::Food, Good::Water, Good::Timber, Good::Tool] {
        let mut opportunity = NeedOpportunity {
            known: StockKnowledge {
                inventory: InventoryId(10),
                area: AreaId(5),
                good,
                quantity: Quantity(1_000),
                accessible: false,
                observed_at: SimTime(0),
                learned_at: SimTime(0),
                source: KnowledgeSource::Observed(PersonId(1)),
            },
            local_custodian: true,
            can_take: false,
        };
        let facts = || AcquisitionFacts {
            actor: actor(1, 0),
            good,
            destination: Some(InventoryId(11)),
            quantity: Quantity(500),
        };
        assert_eq!(
            world.acquisition_opportunities(&store, facts(), std::slice::from_ref(&opportunity)),
            vec![PropertyAction::RequestAccess {
                inventory: InventoryId(10)
            }]
        );
        opportunity.known.area = AreaId(99);
        assert!(
            world
                .acquisition_opportunities(&store, facts(), &[opportunity])
                .is_empty()
        );
        assert!(
            world
                .acquisition_opportunities(&store, facts(), &[])
                .is_empty()
        );
        let mut incapable = facts();
        incapable.actor.capable = false;
        assert!(
            world
                .acquisition_opportunities(&store, incapable, &[])
                .is_empty()
        );
    }
}
#[test]
fn receipt_prefix_and_claim_commit_are_atomic_and_current_restore_keeps_pending_reports() {
    let (mut world, mut store) = fixture();
    let balances = BTreeMap::from([(InventoryId(10), Quantity(1_000))]);
    let prefix = world.prepare_receipts(&store, &[], &balances, 1).unwrap();
    let before = snapshot(&world, &store);
    assert!(
        world
            .prepare_action(
                &store,
                actor(1, 0),
                &PropertyAction::Claim {
                    inventory: InventoryId(10),
                    rights: BTreeSet::new(),
                    evidence: vec![]
                },
                ActionFacts::Claim {
                    stock: source(),
                    id: ClaimId(20),
                    witnesses: vec![]
                },
                Some(prefix)
            )
            .is_err()
    );
    assert_eq!(snapshot(&world, &store), before);
    taking(&mut world, &mut store);
    let witness = ObserverFacts {
        actor: PersonId(2),
        personal_custody: false,
        personally_allowed: false,
        can_defend: false,
    };
    world
        .prepare_witness(&store, IncidentId(30), SimTime(1), &[witness])
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    world
        .prepare_action(
            &store,
            actor(2, 1),
            &PropertyAction::ReportIncident {
                recipient: PersonId(3),
                incident: IncidentId(30),
            },
            ActionFacts::Report {
                recipient: contact(3),
                duration: SimDuration(100),
            },
            None,
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    let loaded: (Runtime, game_ecs::Snapshot) =
        serde_json::from_slice(&serde_json::to_vec(&snapshot(&world, &store)).unwrap()).unwrap();
    let restored = Store::restore(schema(), loaded.1).unwrap();
    assert_eq!(snapshot(&loaded.0, &restored), snapshot(&world, &store));
    loaded.0.validate_owned(&restored, SimTime(1)).unwrap();
}

#[test]
fn timed_actions_reject_zero_duration_and_an_occupied_listener_without_mutation() {
    let (mut world, mut store) = fixture();
    let before = snapshot(&world, &store);
    assert!(
        world
            .prepare_action(
                &store,
                actor(1, 0),
                &PropertyAction::Take {
                    source: InventoryId(10),
                    destination: InventoryId(11),
                    quantity: Quantity(500),
                },
                ActionFacts::Take {
                    source: source(),
                    destination: destination(),
                    id: IncidentId(30),
                    carrying_room: Quantity(3_000),
                    duration: SimDuration(0),
                },
                None,
            )
            .is_err()
    );
    assert_eq!(snapshot(&world, &store), before);
    taking(&mut world, &mut store);
    for person in [2, 4] {
        world
            .prepare_witness(
                &store,
                IncidentId(30),
                SimTime(1),
                &[ObserverFacts {
                    actor: PersonId(person),
                    personal_custody: false,
                    personally_allowed: false,
                    can_defend: false,
                }],
            )
            .unwrap()
            .commit(&mut world, &mut store)
            .unwrap();
    }
    let report = PropertyAction::ReportIncident {
        recipient: PersonId(3),
        incident: IncidentId(30),
    };
    assert!(
        world
            .prepare_action(
                &store,
                actor(2, 1),
                &report,
                ActionFacts::Report {
                    recipient: contact(3),
                    duration: SimDuration(0),
                },
                None
            )
            .is_err()
    );
    world
        .prepare_action(
            &store,
            actor(2, 1),
            &report,
            ActionFacts::Report {
                recipient: contact(3),
                duration: SimDuration(100),
            },
            None,
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    let before = snapshot(&world, &store);
    // External availability is insufficient while the owner has an active report.
    assert!(
        world
            .prepare_action(
                &store,
                actor(4, 1),
                &report,
                ActionFacts::Report {
                    recipient: contact(3),
                    duration: SimDuration(100),
                },
                None
            )
            .is_err()
    );
    assert_eq!(snapshot(&world, &store), before);
}

#[test]
fn taking_completion_preflights_actual_balances_and_stale_opening_partitions() {
    let (mut world, mut store) = fixture();
    taking(&mut world, &mut store);
    let before = snapshot(&world, &store);
    let wrong_source = StockFacts {
        quantity: Quantity(999),
        ..source()
    };
    assert!(
        world
            .prepare_finish_take(
                &store,
                actor(1, 100),
                wrong_source,
                destination(),
                Quantity(3_000),
                None
            )
            .is_err()
    );
    assert_eq!(snapshot(&world, &store), before);
    let prepared = world
        .prepare_finish_take(
            &store,
            actor(1, 100),
            source(),
            destination(),
            Quantity(3_000),
            None,
        )
        .unwrap();
    // An unrelated real consumption after preparation invalidates the opening
    // provenance, even though the incident and its pending deadline are unchanged.
    provenance_mut(&mut store, InventoryId(10))
        .unwrap()
        .portions
        .insert(ClaimId(0), 999);
    let before = snapshot(&world, &store);
    assert_eq!(
        prepared.commit(&mut world, &mut store),
        Err(Error::Unavailable)
    );
    assert_eq!(snapshot(&world, &store), before);
}

#[test]
fn permission_preparation_needs_no_cargo_yield_and_never_offers_a_zero_quantity_taking() {
    let (world, store) = fixture();
    let before = snapshot(&world, &store);
    let offers = world.acquisition_opportunities(
        &store,
        AcquisitionFacts {
            actor: actor(1, 0),
            good: Good::Water,
            destination: Some(InventoryId(11)),
            quantity: Quantity(0),
        },
        &[NeedOpportunity {
            known: StockKnowledge {
                inventory: InventoryId(10),
                area: AreaId(5),
                good: Good::Water,
                quantity: Quantity(500),
                accessible: false,
                observed_at: SimTime(0),
                learned_at: SimTime(0),
                source: KnowledgeSource::Observed(PersonId(1)),
            },
            local_custodian: true,
            can_take: true,
        }],
    );
    assert_eq!(
        offers,
        vec![PropertyAction::RequestAccess {
            inventory: InventoryId(10)
        }]
    );
    assert_eq!(snapshot(&world, &store), before);
}
