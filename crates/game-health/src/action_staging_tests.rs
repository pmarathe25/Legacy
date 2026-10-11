use super::*;
fn setup() -> (Runtime, Store) {
    let runtime = Runtime::default();
    let mut store = crate::component_store();
    insert_job(
        &mut store,
        Job {
            actor: PersonId(1),
            area: AreaId(1),
            completes: SimTime(10),
            work: Work::Treat {
                patient: PersonId(2),
                skill_ms: 0,
                elapsed: 0,
            },
        },
    )
    .unwrap();
    insert_job(
        &mut store,
        Job {
            actor: PersonId(2),
            area: AreaId(1),
            completes: SimTime(11),
            work: Work::Drink {
                source: InventoryId(10),
                quantity: Quantity(1),
            },
        },
    )
    .unwrap();
    (runtime, store)
}
#[test]
fn cancelling_shared_patient_and_giver_stages_once_and_preserves_first_actor_order() {
    let (runtime, mut store) = setup();
    let prepared = runtime.prepare_cancel_many(&store, &[PersonId(2), PersonId(1), PersonId(2)]);
    let groups: Vec<_> = prepared
        .effects()
        .map(|(actor, effects)| (actor, effects.len()))
        .collect();
    assert_eq!(groups, vec![(PersonId(2), 2), (PersonId(1), 0)]);
    let before = store.snapshot().unwrap();
    let mut batch = Batch::new();
    let finish = prepared.stage_into(&runtime, &store, &mut batch).unwrap();
    assert_eq!(store.snapshot().unwrap(), before);
    store.apply_batch(batch).unwrap();
    let effects = finish.finish();
    assert_eq!(
        effects[0].1,
        vec![
            HealthEffect::ClearWork {
                actor: PersonId(1),
                completes: SimTime(10)
            },
            HealthEffect::ClearWork {
                actor: PersonId(2),
                completes: SimTime(11)
            }
        ]
    );
    assert!(jobs(&store).next().is_none());
}
#[test]
fn cancellation_and_foreign_invalid_write_reject_before_any_job_clear() {
    let (runtime, mut store) = setup();
    let before = store.snapshot().unwrap();
    let mut batch = Batch::new();
    runtime
        .prepare_cancel_many(&store, &[PersonId(2)])
        .stage_into(&runtime, &store, &mut batch)
        .unwrap();
    batch.insert(
        StableId(3),
        Clinical {
            cold_remainder: 1000,
            ..Clinical::default()
        },
    );
    assert!(store.apply_batch(batch).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
    let prepared = runtime.prepare_cancel_many(&store, &[PersonId(2)]);
    insert_job(
        &mut store,
        Job {
            actor: PersonId(4),
            area: AreaId(1),
            completes: SimTime(12),
            work: Work::Treat {
                patient: PersonId(2),
                skill_ms: 0,
                elapsed: 0,
            },
        },
    )
    .unwrap();
    let before = store.snapshot().unwrap();
    assert!(
        prepared
            .stage_into(&runtime, &store, &mut Batch::new())
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
}
