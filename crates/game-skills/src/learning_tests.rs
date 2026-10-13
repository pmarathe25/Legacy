use super::*;
use game_ecs::{Schema, StableId};

fn store() -> Store {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    for actor in 1..=10 {
        store.spawn(StableId(actor)).unwrap();
    }
    store
}
fn actor(id: u64, activity: ActivityFacts) -> ActorFacts {
    ActorFacts {
        actor: PersonId(id),
        alive: true,
        health: 100,
        at_area: true,
        equipped: true,
        activity,
        workshop_job: false,
        care_work: None,
        care_can_progress: false,
    }
}
#[test]
fn dying_actor_credit_is_prepared_with_receivers_and_remainder_keeps_opening_pupil_cap() {
    let runtime = Runtime::default();
    let mut store = store();
    runtime
        .credit(&mut store, PersonId(1), Skill::Craft, 30_000, None)
        .unwrap();
    let pupil = |id| {
        actor(
            id,
            ActivityFacts::Lesson {
                teacher: PersonId(1),
                student: PersonId(id),
                topic: Lesson::Craft,
            },
        )
    };
    let mut evaluation = runtime
        .evaluate_learning(
            &store,
            SimTime(0),
            SimTime(60_000),
            [actor(1, ActivityFacts::Construct), pupil(2), pupil(3)],
            [],
        )
        .unwrap();
    let prepared = runtime
        .prepare_actors_learning(&store, &evaluation, [PersonId(1), PersonId(2)])
        .unwrap();
    let mut batch = game_ecs::Batch::new();
    let receipt = prepared.stage_into(&runtime, &store, &mut batch).unwrap();
    let acknowledgement = evaluation
        .prepare_acknowledgement(&runtime, &store, &batch, &receipt)
        .unwrap();
    receipt.validate(&runtime, &store, &batch).unwrap();
    store.apply_batch(batch).unwrap();
    acknowledgement.finish();
    assert_eq!(runtime.practice(&store, PersonId(1), Skill::Craft), 90_000);
    assert_eq!(runtime.practice(&store, PersonId(2), Skill::Craft), 30_000);
    assert_eq!(evaluation.deltas().len(), 1);
    assert_eq!(evaluation.deltas()[0].actor, PersonId(3));
    assert_eq!(evaluation.deltas()[0].mastery_cap, Some(30_000));
    evaluation.validate(&runtime, &store).unwrap();
    runtime
        .prepare_learning(&store, evaluation)
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert_eq!(runtime.practice(&store, PersonId(1), Skill::Craft), 90_000);
    assert_eq!(runtime.practice(&store, PersonId(2), Skill::Craft), 30_000);
    assert_eq!(runtime.practice(&store, PersonId(3), Skill::Craft), 30_000);
}
#[test]
fn rejected_receiving_origin_keeps_learning_evaluation_and_credits_unchanged() {
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct BodyOrigin;
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    schema
        .register::<BodyOrigin>("fixture/body", |_| Err(Error::Unavailable))
        .unwrap();
    let mut store = Store::with_schema(schema);
    store.spawn(StableId(1)).unwrap();
    let runtime = Runtime::default();
    let mut evaluation = runtime
        .evaluate_learning(
            &store,
            SimTime(0),
            SimTime(60_000),
            [actor(1, ActivityFacts::Construct)],
            [],
        )
        .unwrap();
    let before = evaluation.clone();
    let rows = store.snapshot().unwrap();
    let prepared = runtime
        .prepare_actor_learning(&store, &evaluation, PersonId(1))
        .unwrap();
    let mut batch = game_ecs::Batch::new();
    let receipt = prepared.stage_into(&runtime, &store, &mut batch).unwrap();
    batch.insert(StableId(1), BodyOrigin);
    let acknowledgement = evaluation
        .prepare_acknowledgement(&runtime, &store, &batch, &receipt)
        .unwrap();
    assert!(store.apply_batch(batch).is_err());
    drop(acknowledgement);
    assert_eq!(evaluation, before);
    assert_eq!(store.snapshot().unwrap(), rows);
    let mut batch = game_ecs::Batch::new();
    let generic = runtime
        .prepare_practice(&store, evaluation.deltas().iter().cloned())
        .unwrap()
        .stage_into(&runtime, &store, &mut batch)
        .unwrap();
    assert!(
        evaluation
            .prepare_acknowledgement(&runtime, &store, &batch, &generic)
            .is_err()
    );
    assert_eq!(evaluation, before);
}
#[test]
fn prepared_early_learning_retains_teacher_read_after_actor_staging() {
    let runtime = Runtime::default();
    let mut store = store();
    runtime
        .credit(&mut store, PersonId(1), Skill::Craft, 30_000, None)
        .unwrap();
    let evaluation = runtime
        .evaluate_learning(
            &store,
            SimTime(0),
            SimTime(60_000),
            [actor(
                2,
                ActivityFacts::Lesson {
                    teacher: PersonId(1),
                    student: PersonId(2),
                    topic: Lesson::Craft,
                },
            )],
            [],
        )
        .unwrap();
    let prepared = runtime
        .prepare_actor_learning(&store, &evaluation, PersonId(2))
        .unwrap();
    let mut batch = game_ecs::Batch::new();
    let receipt = prepared.stage_into(&runtime, &store, &mut batch).unwrap();
    runtime
        .credit(&mut store, PersonId(1), Skill::Craft, 1, None)
        .unwrap();
    let before = store.snapshot().unwrap();
    assert!(receipt.validate(&runtime, &store, &batch).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
}

#[test]
fn production_learning_entry_selects_skills_and_live_teacher_cap() {
    let runtime = Runtime::default();
    let mut store = store();
    runtime
        .credit(&mut store, PersonId(1), Skill::Craft, 70_000, None)
        .unwrap();
    let mut work = actor(4, ActivityFacts::Workshop);
    work.workshop_job = true;
    let mut care = actor(5, ActivityFacts::Other);
    care.care_work = Some(CareWork::Treat);
    care.care_can_progress = true;
    let facts = [
        actor(
            1,
            ActivityFacts::Lesson {
                teacher: PersonId(1),
                student: PersonId(2),
                topic: Lesson::Craft,
            },
        ),
        actor(
            2,
            ActivityFacts::Lesson {
                teacher: PersonId(1),
                student: PersonId(2),
                topic: Lesson::Craft,
            },
        ),
        actor(3, ActivityFacts::Cultivate),
        work,
        care,
        actor(
            6,
            ActivityFacts::Play {
                interest: ChildhoodInterest::Craft,
            },
        ),
    ];
    let deltas = runtime
        .evaluate_learning(&store, SimTime(0), SimTime(90_000), facts, [])
        .unwrap();
    assert_eq!(
        runtime
            .practice_boundary(&store, SimTime(0), SimTime(90_000), deltas.deltas())
            .unwrap(),
        SimTime(60_000)
    );
    runtime
        .prepare_learning(&store, deltas)
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert_eq!(
        runtime.practice(&store, PersonId(1), Skill::Leadership),
        90_000
    );
    assert_eq!(runtime.practice(&store, PersonId(2), Skill::Craft), 70_000);
    assert_eq!(
        runtime.practice(&store, PersonId(3), Skill::Farming),
        90_000
    );
    assert_eq!(runtime.practice(&store, PersonId(4), Skill::Craft), 90_000);
    assert_eq!(
        runtime.practice(&store, PersonId(5), Skill::Medicine),
        90_000
    );
    assert_eq!(runtime.view(&store, PersonId(2))[1].practice_ms, 70_000);
    assert_eq!(runtime.practice(&store, PersonId(6), Skill::Craft), 90_000);
    assert!(
        runtime
            .work_rate(&store, PersonId(2), Skill::Craft)
            .numerator
            > 1_000
    );
}

#[test]
fn blocked_idle_travel_and_drinking_do_not_train() {
    let runtime = Runtime::default();
    let store = store();
    let mut dead = actor(1, ActivityFacts::Construct);
    dead.alive = false;
    let mut injured = actor(2, ActivityFacts::Construct);
    injured.health = 0;
    let mut moving = actor(
        3,
        ActivityFacts::Play {
            interest: ChildhoodInterest::Craft,
        },
    );
    moving.at_area = false;
    let mut unequipped = actor(4, ActivityFacts::Gather);
    unequipped.equipped = false;
    let mut drinking = actor(5, ActivityFacts::Other);
    drinking.care_work = Some(CareWork::Drink);
    drinking.care_can_progress = true;
    let travel = actor(
        6,
        ActivityFacts::Lesson {
            teacher: PersonId(1),
            student: PersonId(6),
            topic: Lesson::Travel,
        },
    );
    let facts = [
        dead,
        injured,
        moving,
        unequipped,
        drinking,
        travel,
        actor(7, ActivityFacts::Other),
        actor(
            8,
            ActivityFacts::Play {
                interest: ChildhoodInterest::Company,
            },
        ),
    ];
    assert!(
        runtime
            .evaluate_learning(&store, SimTime(10), SimTime(1_010), facts, [])
            .unwrap()
            .deltas()
            .is_empty()
    );
}

#[test]
fn social_policy_credits_only_valid_elapsed_intervals_and_caps_students() {
    let runtime = Runtime::default();
    let mut store = store();
    let facts = [
        SocialFacts {
            interaction: InteractionFacts::Talk {
                first: PersonId(1),
                second: PersonId(2),
            },
            can_progress: true,
            completes: SimTime(20_000),
        },
        SocialFacts {
            interaction: InteractionFacts::Instruction {
                teacher: PersonId(3),
                student: PersonId(4),
                skill: Some(Skill::Craft),
                mastery_cap: 5_000,
            },
            can_progress: true,
            completes: SimTime(90_000),
        },
        SocialFacts {
            interaction: InteractionFacts::Instruction {
                teacher: PersonId(5),
                student: PersonId(6),
                skill: None,
                mastery_cap: 0,
            },
            can_progress: true,
            completes: SimTime(90_000),
        },
        SocialFacts {
            interaction: InteractionFacts::Practice {
                actor: PersonId(7),
                skill: Skill::Combat,
            },
            can_progress: false,
            completes: SimTime(90_000),
        },
    ];
    let deltas = runtime
        .evaluate_learning(&store, SimTime(10_000), SimTime(40_000), [], facts)
        .unwrap();
    runtime
        .prepare_learning(&store, deltas)
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert_eq!(
        runtime.practice(&store, PersonId(1), Skill::Negotiation),
        10_000
    );
    assert_eq!(
        runtime.practice(&store, PersonId(2), Skill::Negotiation),
        10_000
    );
    assert_eq!(
        runtime.practice(&store, PersonId(3), Skill::Leadership),
        30_000
    );
    assert_eq!(runtime.practice(&store, PersonId(4), Skill::Craft), 5_000);
    assert_eq!(
        runtime.practice(&store, PersonId(5), Skill::Leadership),
        30_000
    );
    assert!(skills(&store, PersonId(6)).is_none());
    assert!(skills(&store, PersonId(7)).is_none());
}

#[test]
fn production_event_reaction_is_atomic_bounded_and_initializes_newborns() {
    let runtime = Runtime::default();
    let mut store = store();
    let event = LearningEvent::AgreementActivated {
        accepted: vec![PersonId(1), PersonId(2), PersonId(1)],
    };
    runtime
        .prepare_event(&store, event.clone(), SimTime(0))
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    let before = store.snapshot().unwrap();
    runtime
        .prepare_event(&store, event, SimTime(0))
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert_eq!(before, store.snapshot().unwrap());
    assert_eq!(
        runtime.practice(&store, PersonId(1), Skill::Negotiation),
        6_000
    );
    runtime
        .prepare_event(
            &store,
            LearningEvent::AgreementAmended {
                proposer: PersonId(1),
            },
            SimTime(MINUTE.0),
        )
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert_eq!(
        runtime.practice(&store, PersonId(1), Skill::Negotiation),
        12_000
    );
    runtime
        .prepare_event(
            &store,
            LearningEvent::Born { child: PersonId(3) },
            SimTime(0),
        )
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert_eq!(skills(&store, PersonId(3)), Some(&Skills::default()));
    let before = store.snapshot().unwrap();
    assert!(
        runtime
            .prepare_event(
                &store,
                LearningEvent::AgreementActivated {
                    accepted: vec![PersonId(4), PersonId(99)]
                },
                SimTime(0)
            )
            .is_err()
    );
    assert_eq!(before, store.snapshot().unwrap());
}

#[test]
fn composed_strike_damage_and_save_validation_use_owner_interfaces() {
    let runtime = Runtime::default();
    let mut store = store();
    runtime
        .credit(&mut store, PersonId(1), Skill::Combat, MINUTE.0, None)
        .unwrap();
    assert_eq!(
        runtime
            .combat_damage(&store, PersonId(1), 1_000, 1_200)
            .unwrap(),
        1_206
    );
    assert_eq!(
        validate_world(Some(&runtime), &store, SimTime(0), true, |actor| actor.0
            <= 10),
        Ok(())
    );
    assert!(validate_world(None, &store, SimTime(0), true, |_| true).is_err());
    assert!(validate_world(Some(&runtime), &store, SimTime(0), false, |_| true).is_err());
    assert!(validate_world(Some(&runtime), &store, SimTime(0), true, |_| false).is_err());
}

#[test]
fn live_teacher_cap_is_guarded_without_rejecting_unrelated_same_frame_credit() {
    let runtime = Runtime::default();
    let mut store = store();
    runtime
        .credit(&mut store, PersonId(1), Skill::Craft, 70_000, None)
        .unwrap();
    let pupil = actor(
        2,
        ActivityFacts::Lesson {
            teacher: PersonId(1),
            student: PersonId(2),
            topic: Lesson::Craft,
        },
    );
    let evaluation = runtime
        .evaluate_learning(&store, SimTime(0), SimTime(90_000), [pupil], [])
        .unwrap();
    runtime
        .credit(&mut store, PersonId(1), Skill::Craft, 1, None)
        .unwrap();
    let before = store.snapshot().unwrap();
    assert!(evaluation.validate(&runtime, &store).is_err());
    assert!(runtime.prepare_learning(&store, evaluation).is_err());
    assert_eq!(before, store.snapshot().unwrap());

    let evaluation = runtime
        .evaluate_learning(&store, SimTime(0), SimTime(90_000), [pupil], [])
        .unwrap();
    runtime
        .prepare_event(
            &store,
            LearningEvent::AgreementAmended {
                proposer: PersonId(1),
            },
            SimTime(0),
        )
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    evaluation.validate(&runtime, &store).unwrap();
    let prepared = runtime.prepare_learning(&store, evaluation).unwrap();
    let mut batch = game_ecs::Batch::new();
    let guard = prepared.stage_into(&runtime, &store, &mut batch).unwrap();
    batch.remove::<Skills>(StableId(1));
    assert!(guard.validate(&runtime, &store, &batch).is_err());

    let absent_teacher = actor(
        4,
        ActivityFacts::Lesson {
            teacher: PersonId(3),
            student: PersonId(4),
            topic: Lesson::Craft,
        },
    );
    let evaluation = runtime
        .evaluate_learning(&store, SimTime(0), SimTime(90_000), [absent_teacher], [])
        .unwrap();
    assert!(skills(&store, PersonId(3)).is_none());
    runtime
        .credit_action(
            &mut store,
            PersonId(3),
            Skill::Negotiation,
            6_000,
            SimTime(0),
        )
        .unwrap();
    evaluation.validate(&runtime, &store).unwrap();
    runtime
        .prepare_learning(&store, evaluation)
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert!(skills(&store, PersonId(4)).is_none());
}
