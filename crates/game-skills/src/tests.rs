use super::*;

fn store() -> Store {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    Store::with_schema(schema)
}

#[test]
fn removed_runtime_maps_and_unknown_component_fields_are_rejected() {
    let mut runtime = serde_json::to_value(Runtime::default()).unwrap();
    runtime["records"] = serde_json::json!({});
    assert!(serde_json::from_value::<Runtime>(runtime).is_err());
    let mut row = serde_json::to_value(Skills::default()).unwrap();
    row["semantic_tick"] = serde_json::json!({});
    assert!(serde_json::from_value::<Skills>(row).is_err());
}

#[test]
fn personal_components_roundtrip_with_semantic_limits_and_foreign_rows() {
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct ForeignTag(u64);
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    assert_eq!(schema.keys().collect::<Vec<_>>(), vec!["skills/personal"]);
    schema
        .register::<ForeignTag>("fixture/tag", |_| Ok(()))
        .unwrap();
    let mut store = Store::with_schema(schema.clone());
    let runtime = Runtime::default();
    let actor = PersonId(3);
    assert!(skills(&store, actor).is_none());
    assert!(skills_mut(&mut store, actor).is_none());
    assert!(remove_skills(&mut store, actor).unwrap().is_none());
    runtime
        .credit(&mut store, actor, Skill::Craft, 70_003, None)
        .unwrap();
    runtime
        .credit_action(
            &mut store,
            actor,
            Skill::Negotiation,
            1_000,
            SimTime(60_000),
        )
        .unwrap();
    store
        .insert(game_ecs::StableId(actor.0), ForeignTag(7))
        .unwrap();
    insert_skills(&mut store, PersonId(1), Skills::default()).unwrap();
    let bytes = serde_json::to_vec(&store.snapshot().unwrap()).unwrap();
    let mut restored = Store::restore(schema, serde_json::from_slice(&bytes).unwrap()).unwrap();
    assert_eq!(
        all_skills(&restored).map(|(id, _)| id).collect::<Vec<_>>(),
        vec![PersonId(1), actor]
    );
    assert_eq!(skills(&store, actor), skills(&restored, actor));
    runtime.validate(&restored, SimTime(60_000)).unwrap();
    let before = restored.snapshot().unwrap();
    runtime
        .credit_action(
            &mut restored,
            actor,
            Skill::Negotiation,
            1_000,
            SimTime(60_000),
        )
        .unwrap();
    assert_eq!(restored.snapshot().unwrap(), before);
    skills_mut(&mut restored, actor)
        .unwrap()
        .semantic_ticks
        .insert(Skill::Leadership, 1);
    assert!(remove_skills(&mut restored, actor).unwrap().is_some());
    assert_eq!(
        restored.get::<ForeignTag>(game_ecs::StableId(actor.0)),
        Some(&ForeignTag(7))
    );
    assert!(restored.contains(game_ecs::StableId(actor.0)));
    assert!(skills(&restored, actor).is_none());
    assert_eq!(runtime.practice(&restored, PersonId(1), Skill::Craft), 0);
}

#[test]
fn intrinsic_schema_rejects_bad_rows_without_capturing_rules_or_clock() {
    let mut store = store();
    let runtime = Runtime::default();
    runtime
        .credit(&mut store, PersonId(1), Skill::Craft, 90_003, None)
        .unwrap();
    let before = store.snapshot().unwrap();
    for change in 0..3 {
        let mut invalid = skills(&store, PersonId(1)).unwrap().clone();
        match change {
            0 => invalid.records.get_mut(&Skill::Craft).unwrap().level = 0,
            1 => invalid.records.get_mut(&Skill::Craft).unwrap().progress_ms = 90_003,
            2 => {
                invalid.semantic_ticks.insert(Skill::Combat, 0);
            }
            _ => unreachable!(),
        }
        assert!(insert_skills(&mut store, PersonId(1), invalid.clone()).is_err());
        assert!(insert_skills(&mut store, PersonId(99), invalid.clone()).is_err());
        assert_eq!(store.snapshot().unwrap(), before);
        let mut corrupted = before.clone();
        corrupted.columns.get_mut("skills/personal").unwrap()[0].value =
            serde_json::value::to_raw_value(&invalid).unwrap();
        assert!(Store::restore(store.schema().clone(), corrupted).is_err());
    }
    assert!(insert_skills(&mut store, PersonId(0), Skills::default()).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
    let mut contextual = skills(&store, PersonId(1)).unwrap().clone();
    contextual.records.get_mut(&Skill::Craft).unwrap().level = 2;
    contextual
        .semantic_ticks
        .insert(Skill::Leadership, u64::MAX);
    insert_skills(&mut store, PersonId(1), contextual).unwrap();
    let restored = Store::restore(store.schema().clone(), store.snapshot().unwrap()).unwrap();
    assert!(runtime.validate(&restored, SimTime(60_000)).is_err());
}

#[test]
fn contextual_validation_rejects_progress_caps_and_future_semantic_ticks() {
    let runtime = Runtime::default();
    for change in 0..4 {
        let mut store = store();
        let mut row = Skills::default();
        let mut record = progress(90_003, &runtime.rules).unwrap();
        match change {
            0 => record.level += 1,
            1 => record.progress_ms += 1,
            2 => record.practice_ms = u64::MAX,
            3 => {
                row.semantic_ticks.insert(Skill::Leadership, 2);
            }
            _ => unreachable!(),
        }
        row.records.insert(Skill::Craft, record);
        insert_skills(&mut store, PersonId(1), row).unwrap();
        assert!(runtime.validate(&store, SimTime(60_000)).is_err());
        if change != 3 {
            let before = store.snapshot().unwrap();
            assert!(
                runtime
                    .credit(&mut store, PersonId(1), Skill::Craft, 1_000, None)
                    .is_err()
            );
            assert_eq!(store.snapshot().unwrap(), before);
        }
    }
}

#[test]
fn invalid_rules_and_schema_failures_cannot_publish_learning_or_semantic_ticks() {
    for change in 0..8 {
        let mut rules = Rules::default();
        match change {
            0 => rules.per_level = SimDuration(0),
            1 => rules.max_level = 0,
            2 => rules.max_level = 1_001,
            3 => rules.work_bonus_per_level = 11,
            4 => rules.damage_bonus_per_level = 11,
            5 => rules.action_practice_ms = 0,
            6 => rules.action_practice_ms = u64::MAX,
            7 => rules.per_level = SimDuration(u64::MAX),
            _ => unreachable!(),
        }
        assert!(Runtime::new(rules.clone()).is_err());
        assert!(progress(1, &rules).is_err());
        let runtime = Runtime { rules };
        let mut store = store();
        let before = store.snapshot().unwrap();
        assert!(
            runtime
                .credit(&mut store, PersonId(1), Skill::Craft, 1_000, None)
                .is_err()
        );
        assert!(
            runtime
                .credit_action(
                    &mut store,
                    PersonId(1),
                    Skill::Leadership,
                    1_000,
                    SimTime(0)
                )
                .is_err()
        );
        assert_eq!(store.snapshot().unwrap(), before);
    }
    let runtime = Runtime::default();
    let mut missing = Store::with_schema(game_ecs::Schema::new());
    let before = missing.snapshot().unwrap();
    assert!(
        runtime
            .credit(&mut missing, PersonId(1), Skill::Craft, 1_000, None)
            .is_err()
    );
    assert!(
        runtime
            .credit_action(
                &mut missing,
                PersonId(1),
                Skill::Leadership,
                1_000,
                SimTime(0)
            )
            .is_err()
    );
    assert_eq!(missing.snapshot().unwrap(), before);
    let mut schema = game_ecs::Schema::new();
    schema
        .register::<Skills>("skills/personal", |row| {
            row.validate()?;
            if row.records.values().any(|record| record.practice_ms > 500) {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    let mut stricter = Store::with_schema(schema);
    runtime
        .credit(&mut stricter, PersonId(1), Skill::Craft, 500, None)
        .unwrap();
    let before = stricter.snapshot().unwrap();
    assert!(
        runtime
            .credit_action(
                &mut stricter,
                PersonId(1),
                Skill::Leadership,
                1_000,
                SimTime(0)
            )
            .is_err()
    );
    assert!(
        runtime
            .credit(&mut stricter, PersonId(99), Skill::Craft, 1_000, None)
            .is_err()
    );
    assert_eq!(stricter.snapshot().unwrap(), before);
}

#[test]
fn zero_practice_and_newborn_components_never_copy_existing_mastery() {
    let runtime = Runtime::default();
    let mut store = store();
    let before = store.snapshot().unwrap();
    runtime
        .credit(&mut store, PersonId(99), Skill::Craft, 0, None)
        .unwrap();
    runtime
        .credit_action(&mut store, PersonId(99), Skill::Negotiation, 0, SimTime(0))
        .unwrap();
    runtime
        .credit(&mut store, PersonId(99), Skill::Craft, 1_000, Some(0))
        .unwrap();
    assert_eq!(store.snapshot().unwrap(), before);
    runtime
        .credit(&mut store, PersonId(1), Skill::Craft, 300_000, None)
        .unwrap();
    insert_skills(&mut store, PersonId(2), Skills::default()).unwrap();
    assert_eq!(runtime.level(&store, PersonId(1), Skill::Craft), 5);
    assert_eq!(runtime.level(&store, PersonId(2), Skill::Craft), 0);
    assert_eq!(runtime.practice(&store, PersonId(2), Skill::Craft), 0);
    assert!(
        skills(&store, PersonId(2))
            .unwrap()
            .semantic_ticks
            .is_empty()
    );
}

#[test]
fn semantic_learning_limits_duplicates_without_discarding_distinct_physical_actions() {
    let world = Runtime::default();
    let mut store = store();
    let actor = PersonId(1);
    world
        .credit_action(
            &mut store,
            actor,
            Skill::Negotiation,
            1_000,
            SimTime(59_999),
        )
        .unwrap();
    world
        .credit_action(
            &mut store,
            actor,
            Skill::Negotiation,
            1_000,
            SimTime(59_999),
        )
        .unwrap();
    world
        .credit_action(&mut store, actor, Skill::Leadership, 1_000, SimTime(59_999))
        .unwrap();
    world
        .credit_action(
            &mut store,
            PersonId(2),
            Skill::Negotiation,
            1_000,
            SimTime(59_999),
        )
        .unwrap();
    world
        .credit_action(
            &mut store,
            actor,
            Skill::Negotiation,
            1_000,
            SimTime(60_000),
        )
        .unwrap();
    let after = store.snapshot().unwrap();
    world
        .credit_action(
            &mut store,
            actor,
            Skill::Negotiation,
            1_000,
            SimTime(59_999),
        )
        .unwrap();
    assert_eq!(store.snapshot().unwrap(), after);
    assert_eq!(world.practice(&store, actor, Skill::Negotiation), 2_000);
    assert_eq!(world.practice(&store, actor, Skill::Leadership), 1_000);
    assert_eq!(
        world.practice(&store, PersonId(2), Skill::Negotiation),
        1_000
    );
    for _ in 0..2 {
        world
            .credit_action(&mut store, actor, Skill::Combat, 1_000, SimTime(60_000))
            .unwrap();
    }
    assert_eq!(world.practice(&store, actor, Skill::Combat), 2_000);
    assert!(
        !skills(&store, actor)
            .unwrap()
            .semantic_ticks
            .contains_key(&Skill::Combat)
    );
}

#[test]
fn independent_practice_effects_preserve_elapsed_learning_and_a_guides_limit() {
    let world = Runtime::default();
    let mut store = store();
    for elapsed in [20_000, 30_000, 40_000] {
        world
            .apply_practice(
                &mut store,
                PracticeDelta {
                    actor: PersonId(1),
                    skill: Skill::Craft,
                    milliseconds: elapsed,
                    mastery_cap: Some(70_000),
                },
            )
            .unwrap();
    }
    assert_eq!(world.practice(&store, PersonId(1), Skill::Craft), 70_000);
    assert_eq!(world.level(&store, PersonId(1), Skill::Craft), 1);
    world
        .apply_practice(
            &mut store,
            PracticeDelta {
                actor: PersonId(1),
                skill: Skill::Craft,
                milliseconds: 20_000,
                mastery_cap: None,
            },
        )
        .unwrap();
    assert_eq!(world.practice(&store, PersonId(1), Skill::Craft), 90_000);
}
#[test]
fn partial_training_is_fractional_bounded_and_slice_invariant() {
    let whole = Runtime::default();
    let sliced = whole.clone();
    let mut whole_store = store();
    let mut sliced_store = store();
    whole
        .credit(&mut whole_store, PersonId(1), Skill::Craft, 170_003, None)
        .unwrap();
    for elapsed in [1, 59_999, 10_003, 40_000, 60_000] {
        sliced
            .credit(&mut sliced_store, PersonId(1), Skill::Craft, elapsed, None)
            .unwrap();
    }
    assert_eq!(
        whole_store.snapshot().unwrap(),
        sliced_store.snapshot().unwrap()
    );
    assert_eq!(whole.level(&whole_store, PersonId(1), Skill::Craft), 2);
    assert_eq!(
        skills(&whole_store, PersonId(1)).unwrap().records[&Skill::Craft].progress_ms,
        50_003
    );
    assert_eq!(whole.level(&whole_store, PersonId(1), Skill::Farming), 0);
    whole
        .credit(&mut whole_store, PersonId(1), Skill::Craft, u64::MAX, None)
        .unwrap();
    assert_eq!(whole.level(&whole_store, PersonId(1), Skill::Craft), 100);
    assert_eq!(
        skills(&whole_store, PersonId(1)).unwrap().records[&Skill::Craft].progress_ms,
        0
    );
    assert_eq!(whole.level(&whole_store, PersonId(2), Skill::Craft), 0);
    let saturated = whole_store.snapshot().unwrap();
    whole
        .credit(&mut whole_store, PersonId(1), Skill::Craft, 1_000, None)
        .unwrap();
    assert_eq!(whole_store.snapshot().unwrap(), saturated);
}

#[test]
fn rational_work_progress_and_completion_share_the_exact_remainder() {
    let rate = SkillRate {
        numerator: 1_370,
        denominator: 1_000,
    };
    let (whole, remainder) = advance_work(70_013, rate, 19).unwrap();
    let (mut total, mut carry) = (0, 19);
    for elapsed in [1, 9, 60_003, 10_000] {
        let (work, next) = advance_work(elapsed, rate, carry).unwrap();
        total += work;
        carry = next;
    }
    assert_eq!((whole, remainder), (total, carry));
    let duration = work_duration(60_000, rate, 19).unwrap();
    assert!(advance_work(duration - 1, rate, 19).unwrap().0 < 60_000);
    assert!(advance_work(duration, rate, 19).unwrap().0 >= 60_000);
    assert_eq!(
        work_duration(60_000, SkillRate::default(), 0).unwrap(),
        60_000
    );
    assert!(advance_work(u64::MAX, rate, 0).is_err());
}

#[test]
fn guidance_cannot_copy_a_teachers_full_mastery_and_borrowing_is_read_only() {
    let world = Runtime::default();
    let mut store = store();
    world
        .credit(&mut store, PersonId(1), Skill::Craft, 300_000, None)
        .unwrap();
    let before = store.snapshot().unwrap();
    let borrowed = rate_for_level(world.level(&store, PersonId(1), Skill::Craft), &world.rules);
    assert!(borrowed.numerator > 1_000);
    assert_eq!(store.snapshot().unwrap(), before);
    let teacher = world.practice(&store, PersonId(1), Skill::Craft);
    world
        .credit(&mut store, PersonId(2), Skill::Craft, 30_000, Some(teacher))
        .unwrap();
    assert_eq!(world.level(&store, PersonId(2), Skill::Craft), 0);
    world
        .credit(
            &mut store,
            PersonId(2),
            Skill::Craft,
            600_000,
            Some(teacher),
        )
        .unwrap();
    assert_eq!(world.level(&store, PersonId(2), Skill::Craft), 5);
    world
        .credit(&mut store, PersonId(2), Skill::Craft, 60_000, None)
        .unwrap();
    assert_eq!(world.level(&store, PersonId(2), Skill::Craft), 6);
    world
        .credit(&mut store, PersonId(2), Skill::Craft, 60_000, Some(teacher))
        .unwrap();
    assert_eq!(world.level(&store, PersonId(2), Skill::Craft), 6);
    assert_eq!(damage_for_level(45_000, 100, &world.rules).unwrap(), 67_500);
}
#[test]
fn live_mastery_changes_have_exact_boundaries_and_respect_a_guides_cap() {
    let mut world = Runtime::default();
    let mut store = store();
    let actor = PersonId(1);
    world
        .credit(&mut store, actor, Skill::Medicine, 90_000, None)
        .unwrap();
    assert_eq!(
        world
            .until_next_level(&store, actor, Skill::Medicine, None)
            .unwrap(),
        Some(30_000)
    );
    assert_eq!(
        world
            .until_next_level(&store, actor, Skill::Medicine, Some(119_999))
            .unwrap(),
        None
    );
    assert_eq!(
        world
            .until_next_level(&store, actor, Skill::Medicine, Some(120_000))
            .unwrap(),
        Some(30_000)
    );
    world
        .credit(&mut store, actor, Skill::Medicine, u64::MAX, None)
        .unwrap();
    assert_eq!(
        world
            .until_next_level(&store, actor, Skill::Medicine, None)
            .unwrap(),
        None
    );
    world.rules.per_level = SimDuration(0);
    assert!(
        world
            .until_next_level(&store, actor, Skill::Medicine, None)
            .is_err()
    );
}

#[test]
fn borrowed_bonus_facts_follow_current_owner_rules_and_owner_defaults() {
    assert_eq!(
        bonus_rates(None),
        BonusRates {
            work_bonus_per_level: 10,
            damage_bonus_per_level: 5
        }
    );
    let runtime = Runtime::new(Rules {
        work_bonus_per_level: 7,
        damage_bonus_per_level: 3,
        ..Rules::default()
    })
    .unwrap();
    assert_eq!(
        bonus_rates(Some(&runtime)),
        BonusRates {
            work_bonus_per_level: 7,
            damage_bonus_per_level: 3
        }
    );
}
