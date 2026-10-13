use super::*;
use game_ecs::{Schema, StableId, Store};
fn fixture() -> Store {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    for id in 1..=3 {
        insert_skills(&mut store, PersonId(id), Skills::default()).unwrap();
    }
    store
}
fn delta(actor: u64, skill: Skill, milliseconds: u64, mastery_cap: Option<u64>) -> PracticeDelta {
    PracticeDelta {
        actor: PersonId(actor),
        skill,
        milliseconds,
        mastery_cap,
    }
}
#[test]
fn admitted_agreement_events_share_exact_semantic_ticks_and_reject_origin_schema_failure() {
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct AgreementOrigin(bool);
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    schema
        .register::<AgreementOrigin>("fixture/agreement", |row| {
            if row.0 {
                Ok(())
            } else {
                Err(Error::Unavailable)
            }
        })
        .unwrap();
    let mut store = Store::with_schema(schema);
    for id in 1..=2 {
        insert_skills(&mut store, PersonId(id), Skills::default()).unwrap();
    }
    let runtime = Runtime::default();
    let prepare = |store: &Store| {
        runtime
            .prepare_events(
                store,
                [
                    LearningEvent::AgreementActivated {
                        accepted: vec![PersonId(1), PersonId(2), PersonId(1)],
                    },
                    LearningEvent::AgreementAmended {
                        proposer: PersonId(1),
                    },
                ],
                SimTime(MINUTE.0),
            )
            .unwrap()
    };
    let before = store.snapshot().unwrap();
    let mut rejected = game_ecs::Batch::new();
    rejected.insert(StableId(10), AgreementOrigin(false));
    let guard = prepare(&store)
        .stage_into(&runtime, &store, &mut rejected)
        .unwrap();
    guard.validate(&runtime, &store, &rejected).unwrap();
    assert!(store.apply_batch(rejected).is_err());
    assert_eq!(before, store.snapshot().unwrap());
    let mut batch = game_ecs::Batch::new();
    batch.insert(StableId(10), AgreementOrigin(true));
    let guard = prepare(&store)
        .stage_into(&runtime, &store, &mut batch)
        .unwrap();
    guard.validate(&runtime, &store, &batch).unwrap();
    store.apply_batch(batch).unwrap();
    for actor in [PersonId(1), PersonId(2)] {
        assert_eq!(
            runtime.practice(&store, actor, Skill::Negotiation),
            runtime.rules.action_practice_ms
        );
        assert_eq!(
            skills(&store, actor).unwrap().semantic_ticks[&Skill::Negotiation],
            1
        );
    }
    prepare(&store).commit(&runtime, &mut store).unwrap();
    assert_eq!(
        runtime.practice(&store, PersonId(1), Skill::Negotiation),
        runtime.rules.action_practice_ms
    );
}

#[test]
fn newborn_staging_allows_sibling_components_but_retains_own_projection_and_absence() {
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Life(bool);
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    schema
        .register::<Life>("fixture/life", |row| {
            if row.0 {
                Ok(())
            } else {
                Err(Error::Unavailable)
            }
        })
        .unwrap();
    let mut store = Store::with_schema(schema);
    let runtime = Runtime::default();
    let prepared = runtime.prepare_newborn(&store, PersonId(1)).unwrap();
    let before = store.snapshot().unwrap();
    let mut rejected = game_ecs::Batch::new();
    rejected.insert(StableId(1), Life(false));
    let guard = prepared
        .clone()
        .stage_into(&runtime, &store, &mut rejected)
        .unwrap();
    guard.validate(&runtime, &store, &rejected).unwrap();
    assert!(store.apply_batch(rejected).is_err());
    assert_eq!(before, store.snapshot().unwrap());
    let mut batch = game_ecs::Batch::new();
    batch.insert(StableId(1), Life(true));
    let guard = prepared.stage_into(&runtime, &store, &mut batch).unwrap();
    guard.validate(&runtime, &store, &batch).unwrap();
    store.apply_batch(batch).unwrap();
    assert_eq!(skills(&store, PersonId(1)), Some(&Skills::default()));
    assert!(runtime.prepare_newborn(&store, PersonId(1)).is_err());
    let prepared = runtime.prepare_newborn(&store, PersonId(2)).unwrap();
    store.insert_or_spawn(StableId(2), Life(true)).unwrap();
    assert!(
        prepared
            .stage_into(&runtime, &store, &mut game_ecs::Batch::new())
            .is_err()
    );
}
#[test]
fn sparse_practice_matches_sequential_credits_and_preserves_semantic_events() {
    let runtime = Runtime::default();
    let mut batch = fixture();
    runtime
        .credit_action(
            &mut batch,
            PersonId(1),
            Skill::Leadership,
            6000,
            SimTime(MINUTE.0),
        )
        .unwrap();
    let mut scalar = batch.clone();
    let deltas = vec![
        delta(2, Skill::Craft, 90000, None),
        delta(1, Skill::Leadership, 60000, None),
        delta(2, Skill::Craft, 60000, Some(100000)),
        delta(1, Skill::Craft, 120000, None),
    ];
    let prepared = runtime.prepare_practice(&batch, deltas.clone()).unwrap();
    for row in deltas {
        runtime.apply_practice(&mut scalar, row).unwrap();
    }
    prepared.commit(&runtime, &mut batch).unwrap();
    assert_eq!(batch.snapshot().unwrap(), scalar.snapshot().unwrap());
    assert_eq!(
        skills(&batch, PersonId(1)).unwrap().semantic_ticks[&Skill::Leadership],
        1
    );
    assert_eq!(runtime.practice(&batch, PersonId(2), Skill::Craft), 100000);
}
#[test]
fn stale_touched_rows_and_rules_fail_without_partial_changes() {
    let mut runtime = Runtime::default();
    let mut store = fixture();
    let prepared = runtime
        .prepare_practice(
            &store,
            vec![
                delta(1, Skill::Craft, 60000, None),
                delta(2, Skill::Craft, 60000, None),
            ],
        )
        .unwrap();
    runtime
        .credit(&mut store, PersonId(2), Skill::Craft, 1, None)
        .unwrap();
    let before = store.snapshot().unwrap();
    assert!(prepared.commit(&runtime, &mut store).is_err());
    assert_eq!(before, store.snapshot().unwrap());
    let prepared = runtime
        .prepare_practice(&store, vec![delta(1, Skill::Craft, 60000, None)])
        .unwrap();
    runtime.rules.per_level = SimDuration(120000);
    let before = store.snapshot().unwrap();
    assert!(prepared.commit(&runtime, &mut store).is_err());
    assert_eq!(before, store.snapshot().unwrap());
}
#[test]
fn unrelated_learning_remains_independent_and_zero_effect_does_not_attach_rows() {
    let runtime = Runtime::default();
    let mut store = fixture();
    let prepared = runtime
        .prepare_practice(&store, vec![delta(1, Skill::Craft, 60000, None)])
        .unwrap();
    runtime
        .credit(&mut store, PersonId(3), Skill::Farming, 100, None)
        .unwrap();
    prepared.commit(&runtime, &mut store).unwrap();
    assert_eq!(runtime.practice(&store, PersonId(3), Skill::Farming), 100);
    store.spawn(StableId(4)).unwrap();
    let before = store.snapshot().unwrap();
    runtime
        .prepare_practice(&store, vec![delta(4, Skill::Craft, 10, Some(0))])
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert_eq!(before, store.snapshot().unwrap());
}
#[test]
fn schema_rejection_is_atomic_and_boundary_is_owned_by_skills() {
    let mut schema = Schema::new();
    schema
        .register::<Skills>("test/skills", |row| {
            if row.practice(Skill::Craft) > 60000 {
                Err(Error::Unavailable)
            } else {
                row.validate()
            }
        })
        .unwrap();
    let mut store = Store::with_schema(schema);
    for id in 1..=2 {
        insert_skills(&mut store, PersonId(id), Skills::default()).unwrap();
    }
    let runtime = Runtime::default();
    let before = store.snapshot().unwrap();
    let deltas = vec![
        delta(1, Skill::Craft, 60000, None),
        delta(2, Skill::Craft, 120000, None),
    ];
    assert_eq!(
        runtime
            .practice_boundary(&store, SimTime(10), SimTime(120010), &deltas)
            .unwrap(),
        SimTime(60010)
    );
    assert!(
        runtime
            .prepare_practice(&store, deltas)
            .unwrap()
            .commit(&runtime, &mut store)
            .is_err()
    );
    assert_eq!(before, store.snapshot().unwrap());
}

#[test]
fn final_guard_rejects_foreign_overlap_wrong_batch_and_stale_readonly_rows() {
    let runtime = Runtime::default();
    let store = fixture();
    let before = store.snapshot().unwrap();
    let prepared = runtime
        .prepare_practice(&store, [delta(1, Skill::Craft, 60_000, None)])
        .unwrap();
    let mut prefix = game_ecs::Batch::new();
    prefix.remove::<Skills>(StableId(1));
    assert!(
        prepared
            .clone()
            .stage_into(&runtime, &store, &mut prefix)
            .is_err()
    );
    let mut batch = game_ecs::Batch::new();
    let staged = prepared.stage_into(&runtime, &store, &mut batch).unwrap();
    assert!(
        staged
            .validate(&runtime, &store, &game_ecs::Batch::new())
            .is_err()
    );
    staged.validate(&runtime, &store, &batch).unwrap();
    batch.remove::<Skills>(StableId(1));
    assert!(staged.validate(&runtime, &store, &batch).is_err());
    assert_eq!(store.snapshot().unwrap(), before);

    let mut store = store;
    let prepared = runtime
        .prepare_practice(&store, [delta(1, Skill::Craft, 10, Some(0))])
        .unwrap();
    runtime
        .credit(&mut store, PersonId(1), Skill::Leadership, 10, None)
        .unwrap();
    let before = store.snapshot().unwrap();
    assert!(prepared.commit(&runtime, &mut store).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
}

#[test]
fn owner_staging_composes_with_foreign_rows_and_independent_people_atomically() {
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Foreign(u64);
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    schema
        .register::<Foreign>("fixture/foreign", |row| {
            if row.0 > 10 {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    let mut store = Store::with_schema(schema);
    for id in 1..=2 {
        insert_skills(&mut store, PersonId(id), Skills::default()).unwrap();
    }
    let runtime = Runtime::default();
    let prepared = runtime
        .prepare_practice(&store, [delta(1, Skill::Craft, 60_000, None)])
        .unwrap();
    let before = store.snapshot().unwrap();
    let mut rejected = game_ecs::Batch::new();
    let guard = prepared
        .clone()
        .stage_into(&runtime, &store, &mut rejected)
        .unwrap();
    rejected.insert(StableId(1), Foreign(11));
    guard.validate(&runtime, &store, &rejected).unwrap();
    assert!(store.apply_batch(rejected).is_err());
    assert_eq!(before, store.snapshot().unwrap());
    let mut batch = game_ecs::Batch::new();
    let guard = prepared.stage_into(&runtime, &store, &mut batch).unwrap();
    batch.insert(StableId(1), Foreign(7));
    let independent = runtime
        .prepare_practice(&store, [delta(2, Skill::Farming, 30_000, None)])
        .unwrap()
        .stage_into(&runtime, &store, &mut batch)
        .unwrap();
    guard.validate(&runtime, &store, &batch).unwrap();
    independent.validate(&runtime, &store, &batch).unwrap();
    store.apply_batch(batch).unwrap();
    assert_eq!(runtime.practice(&store, PersonId(1), Skill::Craft), 60_000);
    assert_eq!(
        runtime.practice(&store, PersonId(2), Skill::Farming),
        30_000
    );
    assert_eq!(store.get::<Foreign>(StableId(1)), Some(&Foreign(7)));
}
