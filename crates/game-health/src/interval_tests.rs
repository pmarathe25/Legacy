use crate::*;
use game_core::*;
use game_ecs::{Schema, StableId, Store};
use serde::{Deserialize, Serialize};

fn medical_job(actor: u64) -> Job {
    Job {
        actor: PersonId(actor),
        area: AreaId(10),
        completes: SimTime(30_000),
        work: Work::Treat {
            patient: PersonId(2),
            skill_ms: 0,
            elapsed: 0,
        },
    }
}

fn input<'a>(runtime: &'a Runtime, state: &'a Clinical) -> ClinicalInput<'a> {
    ClinicalInput {
        rules: &runtime.rules,
        state,
        now: SimTime(0),
        next: SimTime(1_000),
        alive: true,
        thirst_enabled: true,
        resting: false,
        exposure: ExposureInput::default(),
        physiology: clinical_physiology(&runtime.rules, 1_000, 0).unwrap(),
    }
}

fn prepare(runtime: &Runtime, store: &Store, can_progress: bool) -> PreparedInterval {
    let state = clinical(store, PersonId(1)).cloned().unwrap_or_default();
    runtime
        .prepare_interval(
            store,
            SimTime(0),
            SimTime(1_000),
            &[(PersonId(1), input(runtime, &state))],
            &[(
                PersonId(1),
                JobInput {
                    job: job(store, PersonId(1)).unwrap(),
                    can_progress,
                    medical_level: 20,
                },
            )],
        )
        .unwrap()
}

#[test]
fn sparse_interval_delegates_exact_calculations_and_preserves_unrelated_rows() {
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct ForeignTag(u64);
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    schema
        .register::<ForeignTag>("fixture/tag", |_| Ok(()))
        .unwrap();
    let mut store = Store::with_schema(schema);
    let runtime = Runtime::default();
    insert_job(&mut store, medical_job(1)).unwrap();
    store.insert(StableId(1), ForeignTag(9)).unwrap();
    let opening = Clinical::default();
    let expected_clinical = advance_clinical(input(&runtime, &opening)).unwrap();
    let expected_job = advance_job(&medical_job(1), 1_000, true, 20).unwrap();
    let prepared = prepare(&runtime, &store, true);
    assert_eq!(
        prepared.changed_jobs().collect::<Vec<_>>(),
        vec![(PersonId(1), &expected_job)]
    );
    insert_clinical(
        &mut store,
        PersonId(99),
        Clinical {
            thirst: 77,
            ..Clinical::default()
        },
    )
    .unwrap();
    insert_job(&mut store, medical_job(99)).unwrap();
    prepared.validate(&runtime, &store).unwrap();
    prepared.commit(&runtime, &mut store, |_| true).unwrap();
    assert_eq!(clinical(&store, PersonId(1)), Some(&expected_clinical));
    assert_eq!(job(&store, PersonId(1)), Some(&expected_job));
    assert_eq!(clinical(&store, PersonId(99)).unwrap().thirst, 77);
    assert_eq!(job(&store, PersonId(99)), Some(&medical_job(99)));
    assert_eq!(store.get::<ForeignTag>(StableId(1)), Some(&ForeignTag(9)));
}

#[test]
fn interval_admission_rejects_duplicate_keys_and_mismatched_opening_inputs() {
    let runtime = Runtime::default();
    let mut store = component_store();
    insert_job(&mut store, medical_job(1)).unwrap();
    let state = Clinical::default();
    let correct = input(&runtime, &state);
    let work = JobInput {
        job: job(&store, PersonId(1)).unwrap(),
        can_progress: true,
        medical_level: 20,
    };
    let before = store.snapshot().unwrap();
    assert!(
        runtime
            .prepare_interval(
                &store,
                SimTime(0),
                SimTime(1_000),
                &[(PersonId(1), correct), (PersonId(1), correct)],
                &[]
            )
            .is_err()
    );
    assert!(
        runtime
            .prepare_interval(
                &store,
                SimTime(0),
                SimTime(1_000),
                &[],
                &[(PersonId(1), work), (PersonId(1), work)]
            )
            .is_err()
    );
    let wrong_rules = Rules {
        drinking_time: SimDuration(1),
        ..runtime.rules.clone()
    };
    let wrong_state = Clinical {
        thirst: 1,
        ..Clinical::default()
    };
    for mismatched in [
        ClinicalInput {
            rules: &wrong_rules,
            ..correct
        },
        ClinicalInput {
            state: &wrong_state,
            ..correct
        },
        ClinicalInput {
            now: SimTime(1),
            ..correct
        },
        ClinicalInput {
            next: SimTime(999),
            ..correct
        },
    ] {
        assert!(
            runtime
                .prepare_interval(
                    &store,
                    SimTime(0),
                    SimTime(1_000),
                    &[(PersonId(1), mismatched)],
                    &[]
                )
                .is_err()
        );
    }
    assert!(
        runtime
            .prepare_interval(
                &store,
                SimTime(0),
                SimTime(1_000),
                &[(PersonId(0), correct)],
                &[]
            )
            .is_err()
    );
    assert!(
        runtime
            .prepare_interval(
                &store,
                SimTime(0),
                SimTime(1_000),
                &[],
                &[(PersonId(2), work)]
            )
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
}

#[test]
fn stale_clinical_jobs_rules_or_phantom_rows_reject_the_entire_interval() {
    for change in 0..4 {
        let mut runtime = Runtime::default();
        let mut store = component_store();
        insert_job(&mut store, medical_job(1)).unwrap();
        if change != 3 {
            insert_clinical(&mut store, PersonId(1), Clinical::default()).unwrap();
        }
        let prepared = prepare(&runtime, &store, false);
        match change {
            0 => clinical_mut(&mut store, PersonId(1)).unwrap().thirst = 1,
            1 => job_mut(&mut store, PersonId(1)).unwrap().completes.0 += 1,
            2 => runtime.rules.drinking_time = SimDuration(1),
            3 => {
                insert_clinical(&mut store, PersonId(1), Clinical::default()).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(prepared.validate(&runtime, &store).is_err());
        let before = store.snapshot().unwrap();
        assert!(prepared.commit(&runtime, &mut store, |_| true).is_err());
        assert_eq!(store.snapshot().unwrap(), before);
    }
}

#[test]
fn intentional_cancellation_is_preserved_and_replacement_work_is_never_accepted() {
    for change in 0..4 {
        let runtime = Runtime::default();
        let mut store = component_store();
        insert_job(&mut store, medical_job(1)).unwrap();
        let prepared = prepare(&runtime, &store, true);
        prepared.validate(&runtime, &store).unwrap();
        if change != 0 {
            remove_job(&mut store, PersonId(1)).unwrap();
        }
        if change == 3 {
            let mut replacement = medical_job(1);
            replacement.completes = SimTime(60_000);
            insert_job(&mut store, replacement).unwrap();
        }
        let before = store.snapshot().unwrap();
        let result = prepared.commit(&runtime, &mut store, |_| change == 2);
        if change <= 1 {
            result.unwrap();
            assert!(job(&store, PersonId(1)).is_none());
            assert_eq!(clinical(&store, PersonId(1)).unwrap().thirst, 1_000);
        } else {
            assert!(result.is_err());
            assert_eq!(store.snapshot().unwrap(), before);
        }
    }
}

#[test]
fn interval_component_validator_rejection_does_not_publish_any_other_row() {
    let mut schema = Schema::new();
    schema.register::<Job>("health/job", Job::validate).unwrap();
    schema
        .register::<Clinical>("health/clinical", |row| {
            row.validate()?;
            if row.thirst > 500 {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    let mut store = Store::with_schema(schema);
    let runtime = Runtime::default();
    insert_clinical(&mut store, PersonId(1), Clinical::default()).unwrap();
    insert_job(&mut store, medical_job(1)).unwrap();
    let prepared = prepare(&runtime, &store, false);
    let before = store.snapshot().unwrap();
    assert!(prepared.commit(&runtime, &mut store, |_| true).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
}

#[test]
fn unchanged_inputs_remain_guarded_without_creating_default_components() {
    let runtime = Runtime::default();
    let mut store = component_store();
    let state = Clinical::default();
    let before = store.snapshot().unwrap();
    let prepared = runtime
        .prepare_interval(
            &store,
            SimTime(0),
            SimTime(1_000),
            &[(
                PersonId(1),
                ClinicalInput {
                    alive: false,
                    ..input(&runtime, &state)
                },
            )],
            &[],
        )
        .unwrap();
    assert_eq!(prepared.changed_jobs().count(), 0);
    prepared.commit(&runtime, &mut store, |_| true).unwrap();
    assert_eq!(store.snapshot().unwrap(), before);
}

/// A plain receiving component proves composition without importing assembly
/// or its public Activity representation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum ReceiverActivity {
    HealthWork {
        kind: HealthWorkKind,
        completes: SimTime,
    },
    Idle,
}
fn activity_store(reject_deadline: bool) -> Store {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    if reject_deadline {
        schema.register::<ReceiverActivity>("test/activity", |row| {
            if matches!(row, ReceiverActivity::HealthWork { completes, .. } if completes.0 > 30000) { return Err(Error::Unavailable); }
            Ok(())
        }).unwrap();
    } else {
        schema
            .register::<ReceiverActivity>("test/activity", |_| Ok(()))
            .unwrap();
    }
    let mut store = Store::with_schema(schema);
    insert_job(&mut store, medical_job(1)).unwrap();
    insert_clinical(&mut store, PersonId(1), Clinical::default()).unwrap();
    store
        .insert(
            StableId(1),
            ReceiverActivity::HealthWork {
                kind: HealthWorkKind::Treat,
                completes: SimTime(30000),
            },
        )
        .unwrap();
    store
}
fn stage_received_deadlines(
    prepared: PreparedInterval,
    runtime: &Runtime,
    store: &Store,
    keep: bool,
    batch: &mut game_ecs::Batch,
) -> (IntervalFinalization, game_ecs::ReadGuard) {
    let mut reads = game_ecs::ReadSet::new();
    let finish = prepared
        .stage_into(
            runtime,
            store,
            |_| keep,
            |actor| match reads.read::<ReceiverActivity>(store, StableId(actor.0)) {
                Some(ReceiverActivity::HealthWork { kind, .. }) => Some(*kind),
                _ => None,
            },
            batch,
        )
        .unwrap();
    let mut admitted = game_ecs::ReadFootprint::new();
    for deadline in finish.deadlines() {
        batch.insert(
            StableId(deadline.actor.0),
            ReceiverActivity::HealthWork {
                kind: deadline.kind,
                completes: deadline.completes,
            },
        );
        admitted.protect::<ReceiverActivity>(StableId(deadline.actor.0));
    }
    let guard = reads.bind(store, batch, &admitted).unwrap();
    (finish, guard)
}
#[test]
fn interval_and_received_activity_deadline_publish_atomically_under_the_actual_owner_entrypoint() {
    for rejects in [true, false] {
        let runtime = Runtime::default();
        let mut store = activity_store(rejects);
        let before = store.clone();
        let prepared = prepare(&runtime, &store, false);
        let mut batch = game_ecs::Batch::new();
        let (finish, activity_guard) =
            stage_received_deadlines(prepared, &runtime, &store, true, &mut batch);
        assert_eq!(
            finish.deadlines(),
            &[WorkDeadlineEffect {
                actor: PersonId(1),
                kind: HealthWorkKind::Treat,
                completes: SimTime(31000)
            }]
        );
        finish.validate_batch(&runtime, &store, &batch).unwrap();
        activity_guard.validate(&store, &batch).unwrap();
        let result = store.apply_batch(batch);
        if rejects {
            assert_eq!(result, Err(Error::Unavailable));
            assert_eq!(store, before);
        } else {
            result.unwrap();
            assert_eq!(job(&store, PersonId(1)).unwrap().completes, SimTime(31000));
            assert_eq!(clinical(&store, PersonId(1)).unwrap().thirst, 1000);
            assert_eq!(
                store.get::<ReceiverActivity>(StableId(1)),
                Some(&ReceiverActivity::HealthWork {
                    kind: HealthWorkKind::Treat,
                    completes: SimTime(31000)
                })
            );
        }
    }
}
#[test]
fn deadline_receiver_guards_concrete_activity_facts_and_later_same_batch_edits() {
    for canonical in [true, false] {
        let runtime = Runtime::default();
        let mut store = activity_store(false);
        let prepared = prepare(&runtime, &store, false);
        let mut batch = game_ecs::Batch::new();
        let (finish, activity_guard) =
            stage_received_deadlines(prepared, &runtime, &store, true, &mut batch);
        if canonical {
            store.insert(StableId(1), ReceiverActivity::Idle).unwrap();
        } else {
            batch.insert(StableId(1), ReceiverActivity::Idle);
        }
        let before = store.clone();
        assert_eq!(
            activity_guard.validate(&store, &batch),
            Err(Error::Unavailable)
        );
        finish.validate_batch(&runtime, &store, &batch).unwrap();
        assert_eq!(store, before);
    }
}
#[test]
fn closing_cancellation_never_recreates_a_job_or_stages_its_activity_deadline() {
    let runtime = Runtime::default();
    let mut store = activity_store(false);
    let prepared = prepare(&runtime, &store, false);
    remove_job(&mut store, PersonId(1)).unwrap();
    store.insert(StableId(1), ReceiverActivity::Idle).unwrap();
    let mut batch = game_ecs::Batch::new();
    let (finish, activity_guard) =
        stage_received_deadlines(prepared, &runtime, &store, false, &mut batch);
    assert!(finish.deadlines().is_empty());
    finish.validate_batch(&runtime, &store, &batch).unwrap();
    activity_guard.validate(&store, &batch).unwrap();
    store.apply_batch(batch).unwrap();
    assert!(job(&store, PersonId(1)).is_none());
    assert_eq!(
        store.get::<ReceiverActivity>(StableId(1)),
        Some(&ReceiverActivity::Idle)
    );
    assert_eq!(clinical(&store, PersonId(1)).unwrap().thirst, 1000);
}
#[test]
fn closing_deadlines_include_untracked_new_work_and_guard_new_matching_members() {
    let runtime = Runtime::default();
    let mut store = activity_store(false);
    let prepared = prepare(&runtime, &store, false);
    insert_job(&mut store, medical_job(99)).unwrap();
    store
        .insert(
            StableId(99),
            ReceiverActivity::HealthWork {
                kind: HealthWorkKind::Drink,
                completes: SimTime(1),
            },
        )
        .unwrap();
    let mut batch = game_ecs::Batch::new();
    let (finish, activity_guard) =
        stage_received_deadlines(prepared, &runtime, &store, true, &mut batch);
    assert_eq!(
        finish
            .deadlines()
            .iter()
            .map(|deadline| (deadline.actor, deadline.kind, deadline.completes))
            .collect::<Vec<_>>(),
        vec![
            (PersonId(1), HealthWorkKind::Treat, SimTime(31000)),
            (PersonId(99), HealthWorkKind::Drink, SimTime(30000))
        ]
    );
    activity_guard.validate(&store, &batch).unwrap();
    batch.insert(StableId(100), medical_job(100));
    assert_eq!(
        finish.validate_batch(&runtime, &store, &batch),
        Err(Error::Unavailable)
    );
}
