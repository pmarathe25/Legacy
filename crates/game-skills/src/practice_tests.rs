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
