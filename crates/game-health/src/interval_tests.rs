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
