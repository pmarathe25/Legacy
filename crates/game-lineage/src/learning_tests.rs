use super::*;
use crate::{History, Incarnation, MentorshipRules, Runtime};

fn store() -> Store {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    store.spawn(StableId(1)).unwrap();
    store.spawn(StableId(2)).unwrap();
    store
}

#[test]
fn invocation_learning_is_person_owned_bounded_and_resettable() {
    let mut store = store();
    assert_eq!(invocation_learning(&store, PersonId(1)), 0);
    assert_eq!(
        increase_invocation_learning(&mut store, PersonId(1), 700),
        Ok(700)
    );
    assert_eq!(
        increase_invocation_learning(&mut store, PersonId(1), 500),
        Ok(300)
    );
    assert_eq!(
        invocation_learning(&store, PersonId(1)),
        INVOCATION_LEARNING_LIMIT
    );
    assert_eq!(invocation_learning(&store, PersonId(2)), 0);
    reset_invocation_learning(&mut store, PersonId(1)).unwrap();
    reset_invocation_learning(&mut store, PersonId(1)).unwrap();
    assert_eq!(invocation_learning(&store, PersonId(1)), 0);
}

#[test]
fn invocation_learning_rejects_unknown_people_and_validates_contextual_membership() {
    let mut store = store();
    assert_eq!(
        increase_invocation_learning(&mut store, PersonId(3), 100),
        Err(Error::Unavailable)
    );
    increase_invocation_learning(&mut store, PersonId(1), 100).unwrap();
    assert!(validate_invocation_learning(&store, |person| person == PersonId(1)).is_ok());
    assert!(matches!(
        validate_invocation_learning(&store, |person| person == PersonId(2)),
        Err(Error::InvalidSave(_))
    ));
}

#[test]
fn lesson_judgment_refuses_unknown_topics_before_observed_immediate_care() {
    assert_eq!(
        judge_lesson(LessonJudgmentFacts {
            teacher_knows_topic: false,
            immediate_care: false,
        }),
        Err("I cannot teach knowledge I do not have")
    );
    assert_eq!(
        judge_lesson(LessonJudgmentFacts {
            teacher_knows_topic: true,
            immediate_care: true,
        }),
        Err("The pupil needs immediate physical care before a lesson")
    );
    assert_eq!(
        judge_lesson(LessonJudgmentFacts {
            teacher_knows_topic: true,
            immediate_care: false,
        }),
        Ok(())
    );
}

#[test]
fn invocation_readiness_uses_the_current_person_and_owner_threshold() {
    let mut store = store();
    let world = Runtime::initialize(
        &mut store,
        StableId(1),
        MentorshipRules::default(),
        History {
            incarnations: vec![Incarnation {
                person: PersonId(1),
                bound_at: game_core::SimTime(0),
                died_at: None,
            }],
        },
        [],
    )
    .unwrap();
    assert!(!world.invocation_ready(&store, PersonId(1)));
    increase_invocation_learning(&mut store, PersonId(1), 199).unwrap();
    assert!(!world.invocation_ready(&store, PersonId(1)));
    increase_invocation_learning(&mut store, PersonId(1), 1).unwrap();
    assert!(world.invocation_ready(&store, PersonId(1)));
    assert!(!world.invocation_ready(&store, PersonId(2)));
    world
        .prepare_death(&store, false, game_core::SimTime(20), &Default::default())
        .unwrap()
        .commit(&world, &mut store)
        .unwrap();
    assert!(!world.invocation_ready(&store, PersonId(1)));
}
