use crate::*;
use game_core::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Foreign(u64);
fn schema() -> game_ecs::Schema {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    schema
        .register::<Foreign>("test/foreign", |row| {
            if row.0 == 99 {
                Err(Error::InvalidConfiguration(
                    "foreign admission rejected".into(),
                ))
            } else {
                Ok(())
            }
        })
        .unwrap();
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
    store.insert_or_spawn(StableId(1), Foreign(7)).unwrap();
    (Runtime::default(), store)
}
fn consume(runtime: &Runtime, store: &Store) -> PreparedProperty {
    runtime
        .prepare_receipts(
            store,
            &[PropertyReceipt {
                event: 1,
                change: ReceiptChange::Consume {
                    inventory: InventoryId(10),
                    quantity: Quantity(500),
                },
            }],
            &BTreeMap::from([(InventoryId(10), Quantity(500))]),
            1,
        )
        .unwrap()
}
#[test]
fn owner_schema_has_no_header_entity_maps_and_invalid_admission_is_inert() {
    let (runtime, mut store) = fixture();
    let before = store.snapshot().unwrap();
    assert_eq!(
        serde_json::to_value(&runtime).unwrap(),
        serde_json::json!({"last_event":0})
    );
    assert!(
        serde_json::from_value::<Runtime>(serde_json::json!({"last_event":0,"claims":{}})).is_err()
    );
    assert!(
        insert_attitude(
            &mut store,
            PersonId(1),
            Attitude {
                exclusive_claims: 0,
                sharing: 1_001,
                willingness_to_defend: 0
            }
        )
        .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
    assert_eq!(store.get::<Foreign>(StableId(1)), Some(&Foreign(7)));
}
#[test]
fn staged_native_rows_and_foreign_admission_share_one_atomic_batch() {
    let (mut runtime, mut store) = fixture();
    let before = store.snapshot().unwrap();
    let mut batch = Batch::new();
    let finish = consume(&runtime, &store)
        .stage_into(&runtime, &store, &mut batch)
        .unwrap();
    batch.insert(StableId(1), Foreign(99));
    assert!(store.apply_batch(batch).is_err());
    drop(finish);
    assert_eq!(runtime.last_event, 0);
    assert_eq!(store.snapshot().unwrap(), before);
    let mut batch = Batch::new();
    let finish = consume(&runtime, &store)
        .stage_into(&runtime, &store, &mut batch)
        .unwrap();
    batch.insert(StableId(1), Foreign(8));
    store.apply_batch(batch).unwrap();
    finish.finish(&mut runtime);
    assert_eq!(runtime.last_event, 1);
    assert_eq!(
        portions(&store, InventoryId(10)).unwrap().get(&ClaimId(0)),
        Some(&500)
    );
    assert_eq!(store.get::<Foreign>(StableId(1)), Some(&Foreign(8)));
}
#[test]
fn stale_cursor_or_partition_never_publishes_rows_or_effects() {
    let (mut runtime, mut store) = fixture();
    let prepared = consume(&runtime, &store);
    runtime.last_event = 2;
    let before = store.snapshot().unwrap();
    assert_eq!(
        prepared.commit(&mut runtime, &mut store),
        Err(Error::Unavailable)
    );
    assert_eq!(store.snapshot().unwrap(), before);
    runtime.last_event = 0;
    let prepared = consume(&runtime, &store);
    provenance_mut(&mut store, InventoryId(10))
        .unwrap()
        .portions
        .insert(ClaimId(0), 999);
    let before = store.snapshot().unwrap();
    assert_eq!(
        prepared.commit(&mut runtime, &mut store),
        Err(Error::Unavailable)
    );
    assert_eq!(runtime.last_event, 0);
    assert_eq!(store.snapshot().unwrap(), before);
}
#[test]
fn roundtrip_retains_personal_values_and_checks_native_stock_provenance() {
    let (runtime, mut store) = fixture();
    insert_attitude(
        &mut store,
        PersonId(1),
        Attitude {
            exclusive_claims: 12,
            sharing: 34,
            willingness_to_defend: 56,
        },
    )
    .unwrap();
    let restored = Store::restore(schema(), store.snapshot().unwrap()).unwrap();
    assert_eq!(restored.snapshot().unwrap(), store.snapshot().unwrap());
    runtime
        .validate_with_quantities(&restored, SimTime(0), 1, |id| {
            (id == InventoryId(10)).then_some(Quantity(1_000))
        })
        .unwrap();
    assert!(
        runtime
            .validate_with_quantities(&restored, SimTime(0), 2, |_| Some(Quantity(1_000)))
            .is_err()
    );
    assert!(
        runtime
            .validate_with_quantities(&restored, SimTime(0), 1, |_| Some(Quantity(999)))
            .is_err()
    );
    assert_eq!(attitude(&restored, PersonId(1)).unwrap().sharing, 34);
    assert_eq!(restored.get::<Foreign>(StableId(1)), Some(&Foreign(7)));
}
#[test]
fn cancellation_guard_catches_new_report_participant_membership() {
    let (runtime, mut store) = fixture();
    let prepared = runtime.prepare_cancel(&store, PersonId(1), SimTime(1));
    let incident = Incident {
        id: IncidentId(30),
        actor: PersonId(2),
        source: InventoryId(10),
        destination: InventoryId(11),
        quantity: Quantity(1),
        area: AreaId(3),
        started_at: SimTime(0),
        completes: SimTime(2),
        changed_at: SimTime(0),
        outcome: IncidentOutcome::Attempt,
        portions: BTreeMap::from([(ClaimId(0), 1)]),
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
                snapshot: IncidentKnowledge {
                    incident,
                    learned_at: SimTime(0),
                    source: IncidentSource::Observed,
                },
            }),
        },
    )
    .unwrap();
    assert_eq!(prepared.validate(&runtime, &store), Err(Error::Unavailable));
}

#[test]
fn actual_cargo_and_provenance_owners_stage_conserved_transfer_together() {
    let mut schema = schema();
    game_cargo::register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    let mut runtime = Runtime::default();
    for (id, quantity, location) in [
        (
            InventoryId(10),
            Quantity(1_000),
            game_cargo::InventoryLocation::Area(AreaId(4)),
        ),
        (
            InventoryId(11),
            Quantity(0),
            game_cargo::InventoryLocation::Carried(PersonId(3)),
        ),
    ] {
        game_cargo::insert_inventory(
            &mut store,
            game_cargo::Inventory {
                id,
                location,
                good: Good::Timber,
                quantity,
                capacity: Quantity(3_000),
                access: std::collections::BTreeSet::from([PersonId(3)]),
                last_granted: None,
                spoils_at: None,
            },
        )
        .unwrap();
        insert_provenance(
            &mut store,
            id,
            Provenance {
                portions: if quantity.0 == 0 {
                    BTreeMap::new()
                } else {
                    BTreeMap::from([(ClaimId(0), quantity.0)])
                },
            },
        )
        .unwrap();
    }
    let before = store.snapshot().unwrap();
    for reject in [true, false] {
        let transfer = game_cargo::prepare_transfers(
            &store,
            None,
            vec![game_cargo::TransferRequest {
                intent: game_cargo::TransferIntent {
                    actor: PersonId(3),
                    source: InventoryId(10),
                    destination: InventoryId(11),
                    quantity: Quantity(500),
                },
                facts: game_cargo::ClosingFacts {
                    alive: true,
                    health: 100,
                    source_contact: true,
                    source_access: true,
                    receiver_contact: true,
                    receiver_access: true,
                    receiver_accepted: false,
                    record_permission: true,
                },
            }],
            None,
            |_| true,
        )
        .unwrap();
        let receipt = runtime
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
                &BTreeMap::from([
                    (InventoryId(10), Quantity(500)),
                    (InventoryId(11), Quantity(500)),
                ]),
                1,
            )
            .unwrap();
        let mut batch = Batch::new();
        transfer.stage_into(&store, &mut batch).unwrap();
        let finish = receipt.stage_into(&runtime, &store, &mut batch).unwrap();
        batch.insert(StableId(3), Foreign(if reject { 99 } else { 8 }));
        if reject {
            assert!(store.apply_batch(batch).is_err());
            drop(finish);
            assert_eq!(runtime.last_event, 0);
            assert_eq!(store.snapshot().unwrap(), before);
        } else {
            store.apply_batch(batch).unwrap();
            finish.finish(&mut runtime);
        }
    }
    runtime
        .validate_with_quantities(&store, SimTime(0), 2, |id| {
            game_cargo::inventory(&store, id).map(|row| row.quantity)
        })
        .unwrap();
    assert_eq!(runtime.last_event, 1);
    assert_eq!(
        game_cargo::inventory(&store, InventoryId(10))
            .unwrap()
            .quantity,
        Quantity(500)
    );
    assert_eq!(
        portions(&store, InventoryId(10)).unwrap().get(&ClaimId(0)),
        Some(&500)
    );
    assert_eq!(
        portions(&store, InventoryId(11)).unwrap().get(&ClaimId(0)),
        Some(&500)
    );
}

#[test]
fn staged_receipt_rejects_cursor_advance_before_publication_without_rewinding() {
    let (mut runtime, store) = fixture();
    let before = store.snapshot().unwrap();
    let mut batch = Batch::new();
    let finish = consume(&runtime, &store)
        .stage_into(&runtime, &store, &mut batch)
        .unwrap();
    finish.validate_runtime(&runtime).unwrap();
    runtime.last_event = 2;
    assert_eq!(finish.validate_runtime(&runtime), Err(Error::Unavailable));
    assert_eq!(store.snapshot().unwrap(), before);
    assert_eq!(runtime.last_event, 2);
}

#[test]
fn taking_judgment_and_character_trust_reaction_share_one_admitted_publication() {
    use game_characters::decisions::{Aspiration, Mind, Personality, Relationship, TrustDelta};
    for existing in [false, true] {
        let mut schema = schema();
        game_characters::decisions::register_minds::<u8>(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        let mut runtime = Runtime::default();
        insert_incident(
            &mut store,
            IncidentId(30),
            Incident {
                id: IncidentId(30),
                actor: PersonId(1),
                source: InventoryId(10),
                destination: InventoryId(11),
                area: AreaId(5),
                quantity: Quantity(500),
                started_at: SimTime(0),
                completes: SimTime(100),
                changed_at: SimTime(100),
                outcome: IncidentOutcome::Taken,
                portions: BTreeMap::from([(ClaimId(7), 500)]),
            },
        )
        .unwrap();
        insert_attitude(
            &mut store,
            PersonId(2),
            Attitude {
                exclusive_claims: 900,
                sharing: 100,
                willingness_to_defend: 900,
            },
        )
        .unwrap();
        let mut mind = Mind::<u8>::new(Personality::industrious(), Aspiration::Comfort).unwrap();
        if existing {
            mind.relationships.insert(
                PersonId(1),
                Relationship {
                    trust: -950,
                    affection: 200,
                },
            );
        }
        game_characters::decisions::insert_mind(&mut store, PersonId(2), mind).unwrap();
        store.insert_or_spawn(StableId(2), Foreign(7)).unwrap();
        let observer = observer_facts(
            PersonId(2),
            ObserverContext {
                local: true,
                source_custodian: Some(PersonId(2)),
                taking_actor_allowed: false,
                player_controlled: false,
                capable: true,
            },
        );
        let before = store.snapshot().unwrap();
        let original_attitude = attitude(&store, PersonId(2)).unwrap().clone();
        let published = store.snapshot().unwrap();
        let mut batch = Batch::new();
        let read_guard = runtime
            .prepare_witness(&store, IncidentId(30), SimTime(100), &[observer])
            .unwrap()
            .stage_into(&runtime, &store, &mut batch)
            .unwrap();
        batch.remove::<Attitude>(StableId(2));
        assert_eq!(
            read_guard.validate_batch(&store, &batch),
            Err(Error::Unavailable)
        );
        assert_eq!(store.snapshot().unwrap(), published);
        let pending = runtime
            .prepare_witness(&store, IncidentId(30), SimTime(100), &[observer])
            .unwrap();
        attitude_mut(&mut store, PersonId(2)).unwrap().sharing = 700;
        let changed = store.snapshot().unwrap();
        assert!(pending.commit(&mut runtime, &mut store).is_err());
        assert_eq!(store.snapshot().unwrap(), changed);
        insert_attitude(&mut store, PersonId(2), original_attitude).unwrap();
        for rejected in [true, false] {
            let property = runtime
                .prepare_witness(&store, IncidentId(30), SimTime(100), &[observer])
                .unwrap();
            let trust = game_characters::receivers::prepare_trust_reactions::<u8>(
                &store,
                property.effects().iter().filter_map(|effect| match effect {
                    Effect::Relationship {
                        observer,
                        target,
                        trust,
                    } => Some(TrustDelta {
                        observer: *observer,
                        subject: *target,
                        delta: *trust,
                    }),
                    _ => None,
                }),
            )
            .unwrap();
            let mut batch = Batch::new();
            let finish = property.stage_into(&runtime, &store, &mut batch).unwrap();
            let trust_guard = trust.stage_into(&store, &mut batch).unwrap();
            finish.validate_runtime(&runtime).unwrap();
            finish.validate_batch(&store, &batch).unwrap();
            trust_guard.validate(&store, &batch).unwrap();
            if rejected {
                batch.insert(StableId(2), Foreign(99));
                assert!(store.apply_batch(batch).is_err());
                assert_eq!(store.snapshot().unwrap(), before);
                assert!(known_incidents(&store, PersonId(2)).is_none());
            } else {
                store.apply_batch(batch).unwrap();
                finish.finish(&mut runtime);
            }
        }
        assert!(
            known_incidents(&store, PersonId(2))
                .unwrap()
                .contains_key(&IncidentId(30))
        );
        let relation = &game_characters::decisions::mind::<u8>(&store, PersonId(2))
            .unwrap()
            .relationships[&PersonId(1)];
        assert_eq!(relation.trust, if existing { -1000 } else { -100 });
        assert_eq!(relation.affection, if existing { 200 } else { 0 });
        assert!(
            !runtime
                .prepare_witness(&store, IncidentId(30), SimTime(100), &[observer])
                .unwrap()
                .effects()
                .iter()
                .any(|effect| matches!(effect, Effect::Relationship { .. }))
        );
    }
}
