use crate::*;
use game_core::*;
use game_ecs::{Schema, StableId, Store};
use serde::{Deserialize, Serialize};

fn drink_job(actor: u64) -> Job {
    Job {
        actor: PersonId(actor),
        area: AreaId(10),
        completes: SimTime(30_000),
        work: Work::Drink {
            source: InventoryId(20),
            quantity: Quantity(1_000),
        },
    }
}

fn actor(id: u64) -> ActorFacts {
    ActorFacts {
        actor: PersonId(id),
        now: SimTime(0),
        area: Some(AreaId(10)),
        present: true,
        capable: true,
        at_area: true,
        carried: false,
        tactical: false,
    }
}

fn water() -> StockFacts {
    StockFacts {
        id: InventoryId(20),
        good: Good::Water,
        quantity: Quantity(1_000),
        available: Quantity(1_000),
        usable: true,
        tool_available: false,
    }
}

fn prepare_drink(runtime: &Runtime, store: &Store) -> PreparedHealth {
    runtime
        .prepare_action(
            store,
            actor(1),
            &HealthAction::Drink { source: water().id },
            ActionFacts::Drink {
                water: water(),
                physiology: clinical_physiology(&runtime.rules, 1_000, 0).unwrap(),
            },
        )
        .unwrap()
}

#[test]
fn shared_clinical_and_job_rows_roundtrip_and_preserve_other_components() {
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct ForeignTag(u64);
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    assert_eq!(
        schema.keys().collect::<Vec<_>>(),
        vec![
            "health/clinical",
            "health/job",
            "health/knowledge",
            "health/occupancy",
            "health/offers",
            "health/shelter",
            "health/water"
        ]
    );
    schema
        .register::<ForeignTag>("fixture/tag", |_| Ok(()))
        .unwrap();
    let mut store = Store::with_schema(schema.clone());
    assert!(clinical(&store, PersonId(2)).is_none());
    assert!(clinical_mut(&mut store, PersonId(2)).is_none());
    assert!(job(&store, PersonId(2)).is_none());
    assert!(job_mut(&mut store, PersonId(2)).is_none());
    assert!(remove_clinical(&mut store, PersonId(2)).unwrap().is_none());
    assert!(remove_job(&mut store, PersonId(2)).unwrap().is_none());
    let row = Clinical {
        thirst: 12_345,
        cold_remainder: 7,
        ..Clinical::default()
    };
    insert_clinical(&mut store, PersonId(2), row.clone()).unwrap();
    insert_job(&mut store, drink_job(2)).unwrap();
    store.insert(StableId(2), ForeignTag(9)).unwrap();
    let bytes = serde_json::to_vec(&store.snapshot().unwrap()).unwrap();
    let mut restored = Store::restore(schema, serde_json::from_slice(&bytes).unwrap()).unwrap();
    assert_eq!(
        clinicals(&restored).map(|(id, _)| id).collect::<Vec<_>>(),
        vec![PersonId(2)]
    );
    assert_eq!(
        jobs(&restored).map(|(id, _)| id).collect::<Vec<_>>(),
        vec![PersonId(2)]
    );
    assert_eq!(
        remove_job(&mut restored, PersonId(2)).unwrap(),
        Some(drink_job(2))
    );
    assert_eq!(clinical(&restored, PersonId(2)), Some(&row));
    assert_eq!(
        remove_clinical(&mut restored, PersonId(2)).unwrap(),
        Some(row)
    );
    assert_eq!(
        restored.get::<ForeignTag>(StableId(2)),
        Some(&ForeignTag(9))
    );
    assert!(restored.contains(StableId(2)));
}

#[test]
fn intrinsic_clinical_corruption_cannot_replace_rows_create_entities_or_restore() {
    let mut store = component_store();
    insert_clinical(&mut store, PersonId(1), Clinical::default()).unwrap();
    let before = store.snapshot().unwrap();
    for change in 0..7 {
        let mut invalid = Clinical::default();
        match change {
            0 => invalid.cold_remainder = 1_000,
            1 => {
                invalid.doses.insert(
                    IllnessKind::Enteric,
                    Dose {
                        amount: 0,
                        remainder: 1_000,
                    },
                );
            }
            2..=4 => {
                let mut infection = Infection {
                    kind: IllnessKind::Enteric,
                    acquired: SimTime(1),
                    onset: SimTime(2),
                    recovers: SimTime(3),
                    symptomatic: false,
                };
                match change {
                    2 => infection.kind = IllnessKind::Respiratory,
                    3 => infection.acquired = SimTime(3),
                    4 => infection.recovers = SimTime(2),
                    _ => unreachable!(),
                }
                invalid.illnesses.insert(IllnessKind::Enteric, infection);
            }
            5 => {
                invalid.treatment = Some(Course {
                    until: SimTime(1),
                    healing: 11_001,
                })
            }
            6 => invalid.last_drink = Some((InventoryId(0), SimTime(0))),
            _ => unreachable!(),
        }
        assert!(insert_clinical(&mut store, PersonId(1), invalid.clone()).is_err());
        assert!(insert_clinical(&mut store, PersonId(99), invalid.clone()).is_err());
        assert_eq!(store.snapshot().unwrap(), before);
        let mut corrupted = before.clone();
        corrupted.columns.get_mut("health/clinical").unwrap()[0].value =
            serde_json::value::to_raw_value(&invalid).unwrap();
        assert!(Store::restore(store.schema().clone(), corrupted).is_err());
    }
    assert!(insert_clinical(&mut store, PersonId(0), Clinical::default()).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
}

#[test]
fn intrinsic_health_job_checks_cover_each_work_payload_without_host_facts() {
    let mut store = component_store();
    insert_job(&mut store, drink_job(1)).unwrap();
    let before = store.snapshot().unwrap();
    for change in 0..12 {
        let mut invalid = drink_job(1);
        match change {
            0 => invalid.actor = PersonId(0),
            1 => invalid.area = AreaId(0),
            2 => invalid.completes = SimTime(0),
            3 => {
                invalid.work = Work::Drink {
                    source: InventoryId(0),
                    quantity: Quantity(1_000),
                }
            }
            4 => {
                invalid.work = Work::Drink {
                    source: InventoryId(20),
                    quantity: Quantity(0),
                }
            }
            5 => {
                invalid.work = Work::Boil {
                    source: InventoryId(20),
                    quantity: Quantity(1_000),
                    tool: InventoryId(0),
                }
            }
            6..=9 => {
                invalid.work = Work::Shelter {
                    timber_used: Quantity(if change == 6 { 0 } else { 1_000 }),
                    tool: InventoryId(21),
                    capacity: if change == 7 { 0 } else { 1 },
                    insulation: if change == 8 { 0 } else { 500 },
                    started: SimTime(if change == 9 { invalid.completes.0 } else { 0 }),
                };
            }
            10 => {
                invalid.work = Work::Treat {
                    patient: PersonId(0),
                    skill_ms: 0,
                    elapsed: 0,
                }
            }
            11 => {
                invalid.work = Work::Treat {
                    patient: PersonId(2),
                    skill_ms: 65_536,
                    elapsed: 1,
                }
            }
            _ => unreachable!(),
        }
        assert!(insert_job(&mut store, invalid.clone()).is_err());
        assert_eq!(store.snapshot().unwrap(), before);
        let mut corrupted = before.clone();
        corrupted.columns.get_mut("health/job").unwrap()[0].value =
            serde_json::value::to_raw_value(&invalid).unwrap();
        assert!(Store::restore(store.schema().clone(), corrupted).is_err());
        if change > 0 {
            invalid.actor = PersonId(99);
            assert!(insert_job(&mut store, invalid).is_err());
            assert_eq!(store.snapshot().unwrap(), before);
        }
    }
}

#[test]
fn prepared_cancellation_rejects_new_related_jobs_but_preserves_unrelated_work() {
    let mut runtime = Runtime::default();
    let mut store = component_store();
    insert_job(&mut store, drink_job(1)).unwrap();
    let change = runtime.prepare_cancel(&store, PersonId(1));
    insert_job(&mut store, drink_job(99)).unwrap();
    change.commit(&mut runtime, &mut store).unwrap();
    assert!(job(&store, PersonId(1)).is_none());
    assert_eq!(job(&store, PersonId(99)), Some(&drink_job(99)));
    let change = runtime.prepare_cancel(&store, PersonId(1));
    let mut related = drink_job(2);
    related.work = Work::Treat {
        patient: PersonId(1),
        skill_ms: 0,
        elapsed: 0,
    };
    insert_job(&mut store, related).unwrap();
    let before = (runtime.clone(), store.snapshot().unwrap());
    assert!(change.commit(&mut runtime, &mut store).is_err());
    assert_eq!((runtime, store.snapshot().unwrap()), before);
}

#[test]
fn component_validator_rejection_keeps_jobs_clinical_runtime_and_effects_atomic() {
    let mut schema = Schema::new();
    schema.register::<Job>("health/job", Job::validate).unwrap();
    schema
        .register::<Clinical>("health/clinical", |row| {
            row.validate()?;
            if row.last_drink.is_some() {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    let mut store = Store::with_schema(schema);
    let mut runtime = Runtime {
        rules: Rules::fixture(),
        ..Runtime::default()
    };
    insert_clinical(
        &mut store,
        PersonId(1),
        Clinical {
            thirst: 90_000,
            ..Clinical::default()
        },
    )
    .unwrap();
    prepare_drink(&runtime, &store)
        .commit(&mut runtime, &mut store)
        .unwrap();
    let completes = job(&store, PersonId(1)).unwrap().completes;
    let change = runtime
        .prepare_finish(
            &store,
            PersonId(1),
            completes,
            true,
            FinishFacts::Drink {
                water: water(),
                physiology: clinical_physiology(&runtime.rules, 1_000, 0).unwrap(),
            },
        )
        .unwrap();
    let before = (runtime.clone(), store.snapshot().unwrap());
    assert!(change.commit(&mut runtime, &mut store).is_err());
    assert_eq!((runtime, store.snapshot().unwrap()), before);

    // Starting work withdraws an existing offer. A rejected job schema must
    // leave the offer component and entity creation unpublished as well.
    for missing_schema in [false, true] {
        let mut schema = Schema::new();
        schema
            .register::<Clinical>("health/clinical", Clinical::validate)
            .unwrap();
        schema
            .register::<Offers>("health/offers", Offers::validate)
            .unwrap();
        if !missing_schema {
            schema
                .register::<Job>("health/job", |_| Err(Error::Unavailable))
                .unwrap();
        }
        let mut store = Store::with_schema(schema);
        let mut runtime = Runtime {
            rules: Rules::fixture(),
            ..Runtime::default()
        };
        insert_offer(
            &mut store,
            Offer {
                caregiver: PersonId(1),
                patient: PersonId(2),
                herbs: InventoryId(21),
                water: InventoryId(20),
                expires: SimTime(180_000),
                accepted: true,
            },
        )
        .unwrap();
        let change = prepare_drink(&runtime, &store);
        let before = (runtime.clone(), store.snapshot().unwrap());
        assert!(change.commit(&mut runtime, &mut store).is_err());
        assert_eq!((runtime, store.snapshot().unwrap()), before);
    }
}

#[test]
fn stale_clinical_or_job_rows_reject_completion_without_clearing_work() {
    for change in 0..2 {
        let mut runtime = Runtime {
            rules: Rules::fixture(),
            ..Runtime::default()
        };
        let mut store = component_store();
        insert_clinical(&mut store, PersonId(1), Clinical::default()).unwrap();
        prepare_drink(&runtime, &store)
            .commit(&mut runtime, &mut store)
            .unwrap();
        let completes = job(&store, PersonId(1)).unwrap().completes;
        let prepared = runtime
            .prepare_finish(
                &store,
                PersonId(1),
                completes,
                true,
                FinishFacts::Drink {
                    water: water(),
                    physiology: clinical_physiology(&runtime.rules, 1_000, 0).unwrap(),
                },
            )
            .unwrap();
        if change == 0 {
            clinical_mut(&mut store, PersonId(1)).unwrap().thirst = 1;
        } else {
            job_mut(&mut store, PersonId(1)).unwrap().completes.0 += 1;
        }
        let before = (runtime.clone(), store.snapshot().unwrap());
        assert!(prepared.commit(&mut runtime, &mut store).is_err());
        assert_eq!((runtime, store.snapshot().unwrap()), before);
    }
}

#[test]
fn removed_runtime_maps_and_unknown_clinical_fields_are_rejected() {
    for field in [
        "clinical",
        "jobs",
        "water",
        "shelters",
        "occupancy",
        "offers",
        "known",
    ] {
        let mut runtime = serde_json::to_value(Runtime::default()).unwrap();
        runtime[field] = serde_json::json!({});
        assert!(serde_json::from_value::<Runtime>(runtime).is_err());
    }
    let mut row = serde_json::to_value(Clinical::default()).unwrap();
    row["cold_fraction"] = serde_json::json!(0);
    assert!(serde_json::from_value::<Clinical>(row).is_err());
}
