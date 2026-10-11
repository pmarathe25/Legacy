use super::*;
fn setup() -> (Runtime, Store) {
    let runtime = Runtime {
        rules: Rules::human(),
    };
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    insert_pregnancy(
        &mut store,
        Pregnancy {
            id: PregnancyId(10),
            gestator: PersonId(2),
            partner: PersonId(1),
            state: PregnancyState::Conceiving {
                completes: SimTime(100),
            },
        },
    )
    .unwrap();
    (runtime, store)
}
#[test]
fn shared_conception_ends_once_for_the_first_interrupting_parent() {
    let (runtime, mut store) = setup();
    let prepared = runtime.prepare_interruptions(
        &store,
        &[
            InterruptionRequest {
                actor: PersonId(1),
                died: false,
                affected: vec![PregnancyId(10)],
            },
            InterruptionRequest {
                actor: PersonId(2),
                died: false,
                affected: vec![PregnancyId(10)],
            },
        ],
    );
    assert_eq!(
        prepared
            .effects()
            .map(|(actor, effects)| (actor, effects.len()))
            .collect::<Vec<_>>(),
        vec![(PersonId(1), 1), (PersonId(2), 0)]
    );
    let before = store.snapshot().unwrap();
    let mut batch = Batch::new();
    let finish = prepared.stage_into(&runtime, &store, &mut batch).unwrap();
    assert_eq!(store.snapshot().unwrap(), before);
    store.apply_batch(batch).unwrap();
    assert_eq!(
        pregnancy(&store, PregnancyId(10)).unwrap().state,
        PregnancyState::Ended
    );
    assert!(matches!(finish.finish()[0].1[0], Effect::PregnancyEnded(_)));
}
#[test]
fn lifecycle_cancellation_is_atomic_with_foreign_schema_failure_and_stale_record() {
    let (runtime, mut store) = setup();
    let requests = [InterruptionRequest {
        actor: PersonId(1),
        died: false,
        affected: vec![PregnancyId(10)],
    }];
    let before = store.snapshot().unwrap();
    let mut batch = Batch::new();
    runtime
        .prepare_interruptions(&store, &requests)
        .stage_into(&runtime, &store, &mut batch)
        .unwrap();
    batch.insert(
        StableId(11),
        Pregnancy {
            id: PregnancyId(0),
            gestator: PersonId(1),
            partner: PersonId(2),
            state: PregnancyState::Ended,
        },
    );
    assert!(store.apply_batch(batch).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
    let prepared = runtime.prepare_interruptions(&store, &requests);
    pregnancy_mut(&mut store, PregnancyId(10)).unwrap().state =
        PregnancyState::Gestating { due: SimTime(200) };
    let before = store.snapshot().unwrap();
    assert!(
        prepared
            .stage_into(&runtime, &store, &mut Batch::new())
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
}
