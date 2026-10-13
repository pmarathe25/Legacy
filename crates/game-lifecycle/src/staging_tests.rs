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

#[test]
fn attached_external_fact_reads_reject_changed_inputs_and_late_foreign_batch_edits() {
    let (runtime, mut store) = setup();
    let actor = PersonId(1);
    insert_life(
        &mut store,
        Life {
            id: actor,
            born: SimTime(0),
            initial_age: SimDuration(0),
            stage: LifeStage::Infant,
            reproduction: Reproduction::Neither,
            parents: vec![],
            caregivers: BTreeSet::new(),
            preference: None,
        },
    )
    .unwrap();
    let mut reads = game_ecs::ReadSet::new();
    reads.read::<Life>(&store, StableId(2));
    let prepared = runtime
        .prepare_age(&store, actor, SimTime(1), true, 0, true)
        .unwrap()
        .with_factual_reads(reads);
    let mut batch = Batch::new();
    let finalized = prepared.stage_into(&runtime, &store, &mut batch).unwrap();
    batch.insert(
        StableId(2),
        Life {
            id: PersonId(2),
            born: SimTime(0),
            initial_age: SimDuration(0),
            stage: LifeStage::Infant,
            reproduction: Reproduction::Neither,
            parents: vec![],
            caregivers: BTreeSet::new(),
            preference: None,
        },
    );
    let before = store.snapshot().unwrap();
    assert_eq!(finalized.validate(&store, &batch), Err(Error::Unavailable));
    assert_eq!(store.snapshot().unwrap(), before);
}

#[test]
fn active_gestation_death_selection_excludes_history_and_retains_chosen_membership() {
    let (runtime, mut store) = setup();
    for (id, state) in [
        (11, PregnancyState::Gestating { due: SimTime(200) }),
        (12, PregnancyState::Born(PersonId(13))),
    ] {
        insert_pregnancy(
            &mut store,
            Pregnancy {
                id: PregnancyId(id),
                gestator: PersonId(2),
                partner: PersonId(1),
                state,
            },
        )
        .unwrap();
    }
    assert_eq!(
        active_pregnancies(&store, PersonId(2))
            .iter()
            .map(|row| row.id)
            .collect::<Vec<_>>(),
        vec![PregnancyId(10), PregnancyId(11)]
    );
    let plan = runtime.prepare_gestation_death(&store, PersonId(2));
    assert!(
        matches!(plan.effects(), [Effect::PregnancyEnded(effect)] if effect.pregnancy == PregnancyId(11) && !effect.was_conceiving)
    );
    let before = store.snapshot().unwrap();
    let mut batch = Batch::new();
    let receipt = plan.stage_into(&runtime, &store, &mut batch).unwrap();
    receipt.validate(&store, &batch).unwrap();
    assert_eq!(store.snapshot().unwrap(), before);
    store.apply_batch(batch).unwrap();
    assert!(matches!(
        pregnancy(&store, PregnancyId(10)).unwrap().state,
        PregnancyState::Conceiving { .. }
    ));
    assert_eq!(
        pregnancy(&store, PregnancyId(11)).unwrap().state,
        PregnancyState::Ended
    );
    assert_eq!(
        pregnancy(&store, PregnancyId(12)).unwrap().state,
        PregnancyState::Born(PersonId(13))
    );
    let plan = runtime.prepare_gestation_death(&store, PersonId(2));
    insert_pregnancy(
        &mut store,
        Pregnancy {
            id: PregnancyId(14),
            gestator: PersonId(2),
            partner: PersonId(1),
            state: PregnancyState::Gestating { due: SimTime(300) },
        },
    )
    .unwrap();
    assert!(
        plan.stage_into(&runtime, &store, &mut Batch::new())
            .is_err()
    );
}

#[test]
fn interruption_request_excludes_only_the_same_owner_completed_attempt() {
    let (runtime, store) = setup();
    for valid in [false, true] {
        let bodies = [PersonId(2), PersonId(1)].map(|actor| BodyFacts {
            actor,
            alive: valid,
            health: if valid { 10 } else { 0 },
            position: game_core::Location::AtArea(game_core::AreaId(1)),
            action_restricted: false,
            attempt: Some(Attempt {
                pregnancy: PregnancyId(10),
                completes: SimTime(100),
            }),
        });
        let completed = runtime
            .prepare_complete_conception(
                &store,
                PregnancyId(10),
                SimTime(100),
                [&bodies[0], &bodies[1]],
            )
            .unwrap();
        let request =
            InterruptionRequest::for_attempt(PersonId(1), Some(PregnancyId(10)), Some(&completed));
        assert!(request.affected.is_empty());
        assert!(!request.died);
        assert_eq!(
            InterruptionRequest::for_attempt(PersonId(1), Some(PregnancyId(11)), Some(&completed))
                .affected,
            vec![PregnancyId(11)]
        );
    }
    assert_eq!(
        InterruptionRequest::for_attempt(PersonId(1), Some(PregnancyId(10)), None).affected,
        vec![PregnancyId(10)]
    );
    assert!(
        InterruptionRequest::for_attempt(PersonId(1), None, None)
            .affected
            .is_empty()
    );
}
