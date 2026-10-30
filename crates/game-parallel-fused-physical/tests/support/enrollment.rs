use game_parallel::World;
use game_parallel_blocks::Engine;
use game_parallel_fused_physical::Kernel;
#[test]
fn actual_births_join_social_and_knowledge_and_wait_until_adulthood_for_autonomy() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/enrollment.toml")).unwrap();
    let first_newborn = world.families.as_ref().unwrap().next_id;
    let mut block = Engine::compile(world.clone()).unwrap();
    let mut fused = Kernel::compile(world.clone()).unwrap();
    for _ in 0..8 {
        world.step().unwrap();
        block.step().unwrap();
        fused.step().unwrap();
        assert_eq!(block.export(), world);
        assert_eq!(fused.export(), world);
    }
    let families = world.families.as_ref().unwrap();
    let children: Vec<_> = families
        .members
        .iter()
        .filter(|member| member.id >= first_newborn)
        .collect();
    assert!(!children.is_empty());
    for child in children {
        assert!(
            world
                .social
                .as_ref()
                .unwrap()
                .participants
                .iter()
                .any(|person| person.person == child.id)
        );
        assert!(
            world
                .knowledge
                .as_ref()
                .unwrap()
                .minds
                .iter()
                .any(|mind| mind.person == child.id)
        );
        let actor = world
            .autonomy
            .as_ref()
            .unwrap()
            .actors
            .iter()
            .find(|actor| actor.person == child.id)
            .unwrap();
        assert_eq!(
            actor.next_review,
            child.born.unwrap() + families.rules.adult_age
        );
        assert!(
            !world
                .travelers
                .iter()
                .any(|traveler| traveler.person.id == child.id)
        );
    }
    let saved = serde_json::to_string(&fused.export()).unwrap();
    assert_eq!(
        Kernel::compile(World::from_json(&saved).unwrap())
            .unwrap()
            .export(),
        world
    );
}

#[test]
fn birth_enrollment_is_deterministic_across_workers_and_reload() {
    let world =
        World::from_toml(include_str!("../../../../content/parallel/enrollment.toml")).unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                for tick in 1..=12 {
                    kernel.step().unwrap();
                    if tick == 6 {
                        let save = serde_json::to_string(&kernel.export()).unwrap();
                        kernel = Kernel::compile(World::from_json(&save).unwrap()).unwrap();
                    }
                }
                kernel.export()
            })
    };
    let serial = run(1);
    let parallel = run(4);
    assert_eq!(serial, parallel);
    parallel.validate().unwrap();
}

#[test]
fn newborns_start_seeking_food_only_after_the_configured_age() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    world.cargo = World::from_toml(include_str!(
        "../../../../content/parallel/personal_food.toml"
    ))
    .unwrap()
    .cargo;
    let cargo = world.cargo.as_mut().unwrap();
    cargo.holdings.clear();
    cargo.acquire_when_hungry.clear();
    cargo.auto_seek_age = Some(2);
    let first_newborn = world.families.as_ref().unwrap().next_id;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                let mut saw_newborn = false;
                for tick in 1..=8 {
                    kernel.step().unwrap();
                    let state = kernel.export();
                    let cargo = state.cargo.as_ref().unwrap();
                    for child in state
                        .families
                        .as_ref()
                        .unwrap()
                        .members
                        .iter()
                        .filter(|member| member.id >= first_newborn)
                    {
                        saw_newborn = true;
                        let due = child.born.unwrap() + 2;
                        if due <= tick {
                            assert!(cargo.acquire_when_hungry.contains(&child.id));
                            assert!(
                                cargo
                                    .food_enrollments
                                    .iter()
                                    .all(|entry| entry.person != child.id)
                            );
                        } else {
                            assert!(!cargo.acquire_when_hungry.contains(&child.id));
                            assert!(
                                cargo
                                    .food_enrollments
                                    .iter()
                                    .any(|entry| entry.person == child.id && entry.at == due)
                            );
                        }
                    }
                    if tick == 4 {
                        kernel = Kernel::compile(
                            World::from_json(&serde_json::to_string(&state).unwrap()).unwrap(),
                        )
                        .unwrap();
                    }
                }
                assert!(saw_newborn);
                kernel.export()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn an_explicit_food_choice_cancels_a_scheduled_enrollment() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/caregiving.toml")).unwrap();
    world.cargo = World::from_toml(include_str!(
        "../../../../content/parallel/personal_food.toml"
    ))
    .unwrap()
    .cargo;
    let cargo = world.cargo.as_mut().unwrap();
    cargo.holdings.clear();
    cargo.acquire_when_hungry.clear();
    cargo.auto_seek_age = Some(2);
    cargo
        .food_enrollments
        .push(game_parallel::FoodEnrollment { person: 9, at: 2 });
    let mut kernel = Kernel::compile(world).unwrap();
    kernel.set_food_seeking(9, false).unwrap();
    kernel.step().unwrap();
    kernel.step().unwrap();
    let state = kernel.export();
    assert!(
        state
            .cargo
            .as_ref()
            .unwrap()
            .food_enrollments
            .iter()
            .all(|entry| entry.person != 9)
    );
    assert!(
        !state
            .cargo
            .as_ref()
            .unwrap()
            .acquire_when_hungry
            .contains(&9)
    );
}

#[test]
fn newborns_can_inherit_individual_care_preferences_without_automatic_care() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    world.caregiving =
        World::from_toml(include_str!("../../../../content/parallel/caregiving.toml"))
            .unwrap()
            .caregiving;
    let care = world.caregiving.as_mut().unwrap();
    care.inherit_births = true;
    care.policies[1].willing = false;
    let first_newborn = world.families.as_ref().unwrap().next_id;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                for _ in 0..8 {
                    kernel.step().unwrap();
                }
                let result = kernel.export();
                let children: Vec<_> = result
                    .families
                    .as_ref()
                    .unwrap()
                    .members
                    .iter()
                    .filter(|member| member.id >= first_newborn)
                    .collect();
                assert!(!children.is_empty());
                for child in children {
                    let parent = child.parents[child.id as usize % child.parents.len()];
                    let expected = world
                        .caregiving
                        .as_ref()
                        .unwrap()
                        .policies
                        .iter()
                        .find(|p| p.person == parent)
                        .is_some_and(|p| p.willing);
                    let actual = result
                        .caregiving
                        .as_ref()
                        .unwrap()
                        .policies
                        .iter()
                        .find(|p| p.person == child.id)
                        .unwrap();
                    assert_eq!(actual.willing, expected);
                }
                let restored = World::from_json(&serde_json::to_string(&result).unwrap()).unwrap();
                assert_eq!(Kernel::compile(restored).unwrap().export(), result);
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn newborns_can_inherit_a_personal_sharing_inclination() {
    use game_parallel::FoodSharingPolicy;
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    world.cargo = World::from_toml(include_str!("../../../../content/parallel/food_aid.toml"))
        .unwrap()
        .cargo;
    let cargo = world.cargo.as_mut().unwrap();
    cargo.food_aid.clear();
    cargo.holdings.clear();
    cargo.acquire_when_hungry.clear();
    cargo.inherit_food_sharing = true;
    cargo.food_sharing = world
        .families
        .as_ref()
        .unwrap()
        .members
        .iter()
        .map(|person| FoodSharingPolicy {
            person: person.id,
            amount: 2,
            retain: 1,
            threshold: 1 + person.id % 3,
            interval: 2,
            enabled: false,
        })
        .collect();
    let first_newborn = world.families.as_ref().unwrap().next_id;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                for _ in 0..8 {
                    kernel.step().unwrap();
                }
                let result = kernel.export();
                let children: Vec<_> = result
                    .families
                    .as_ref()
                    .unwrap()
                    .members
                    .iter()
                    .filter(|member| member.id >= first_newborn)
                    .collect();
                assert!(!children.is_empty());
                for child in children {
                    let parent = child.parents[child.id as usize % child.parents.len()];
                    let expected = world
                        .cargo
                        .as_ref()
                        .unwrap()
                        .food_sharing
                        .iter()
                        .find(|p| p.person == parent)
                        .unwrap();
                    let actual = result
                        .cargo
                        .as_ref()
                        .unwrap()
                        .food_sharing
                        .iter()
                        .find(|p| p.person == child.id)
                        .unwrap();
                    assert_eq!(
                        (actual.threshold, actual.enabled),
                        (expected.threshold, false)
                    );
                }
                let restored = World::from_json(&serde_json::to_string(&result).unwrap()).unwrap();
                assert_eq!(Kernel::compile(restored).unwrap().export(), result);
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn newborns_inherit_a_changeable_building_inclination_without_inheriting_a_claim() {
    use game_parallel::FoodStoreBuilder;
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let building = World::from_toml(include_str!(
        "../../../../content/parallel/store_building.toml"
    ))
    .unwrap();
    world.production = building.production;
    world.goods_cargo = building.goods_cargo;
    world.cargo = building.cargo;
    let cargo = world.cargo.as_mut().unwrap();
    cargo.acquire_when_hungry.clear();
    cargo.inherit_store_building = true;
    cargo.food_store_builders = world
        .families
        .as_ref()
        .unwrap()
        .members
        .iter()
        .map(|member| FoodStoreBuilder {
            person: member.id,
            blueprint: 7,
            remaining: 0,
            interval: member.id + 1,
            object_to_taking: false,
            use_after_build: true,
            deposit_after_build: true,
            enabled: false,
        })
        .collect();
    let first_newborn = world.families.as_ref().unwrap().next_id;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                for _ in 0..8 {
                    kernel.step().unwrap();
                }
                let result = kernel.export();
                let cargo = result.cargo.as_ref().unwrap();
                assert!(cargo.food_stores.is_empty());
                let children: Vec<_> = result
                    .families
                    .as_ref()
                    .unwrap()
                    .members
                    .iter()
                    .filter(|member| member.id >= first_newborn)
                    .collect();
                assert!(!children.is_empty());
                for child in children {
                    let parent = child.parents[child.id as usize % child.parents.len()];
                    let actual = cargo
                        .food_store_builders
                        .iter()
                        .find(|row| row.person == child.id)
                        .unwrap();
                    assert_eq!(
                        (actual.interval, actual.remaining, actual.enabled),
                        (parent + 1, 1, false)
                    );
                }
                let restored = World::from_json(&serde_json::to_string(&result).unwrap()).unwrap();
                assert_eq!(Kernel::compile(restored).unwrap().export(), result);
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn newborns_can_inherit_a_pressure_preference_but_not_an_opinion() {
    use game_parallel::{SocialParticipant, SocialPressurePolicy};
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state =
                    World::from_toml(include_str!("../../../../content/parallel/families.toml"))
                        .unwrap();
                state.families.as_mut().unwrap().rules.enroll_births = true;
                let first_child = state.families.as_ref().unwrap().next_id;
                let mut social = World::from_toml(include_str!(
                    "../../../../content/parallel/social_needs.toml"
                ))
                .unwrap()
                .social
                .unwrap();
                social.participants = (1..first_child)
                    .map(|person| SocialParticipant {
                        person,
                        willing: true,
                    })
                    .collect();
                social.pressures = (1..first_child)
                    .map(|person| SocialPressurePolicy {
                        person,
                        fear_at_least: Some(person + 1),
                        obligation_at_least: None,
                        enabled: true,
                    })
                    .collect();
                state.social = Some(social);
                let mut opinions =
                    World::from_toml(include_str!("../../../../content/parallel/campaign.toml"))
                        .unwrap()
                        .opinions
                        .unwrap();
                opinions.rules.encounter_gain = 0;
                opinions.records.clear();
                state.opinions = Some(opinions);
                let mut kernel = Kernel::compile(state).unwrap();
                for _ in 0..8 {
                    kernel.step().unwrap();
                }
                let result = kernel.export();
                let children: Vec<_> = result
                    .families
                    .as_ref()
                    .unwrap()
                    .members
                    .iter()
                    .filter(|m| m.id >= first_child)
                    .collect();
                assert!(!children.is_empty());
                for child in children {
                    let selected_parent = child.parents[child.id as usize % child.parents.len()];
                    let preference = result
                        .social
                        .as_ref()
                        .unwrap()
                        .pressures
                        .iter()
                        .find(|p| p.person == child.id)
                        .unwrap();
                    assert_eq!(preference.fear_at_least, Some(selected_parent + 1));
                    assert!(
                        !result
                            .opinions
                            .as_ref()
                            .unwrap()
                            .records
                            .iter()
                            .any(|o| o.observer == child.id)
                    );
                }
                result.validate().unwrap();
                let restored = World::from_json(&serde_json::to_string(&result).unwrap()).unwrap();
                assert_eq!(Kernel::compile(restored).unwrap().export(), result);
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn disabling_birth_enrollment_does_not_create_invalid_social_preferences() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    state.families.as_mut().unwrap().rules.enroll_births = false;
    let mut social = World::from_toml(include_str!(
        "../../../../content/parallel/social_needs.toml"
    ))
    .unwrap()
    .social
    .unwrap();
    social.pressures = vec![game_parallel::SocialPressurePolicy {
        person: 1,
        fear_at_least: Some(1),
        obligation_at_least: None,
        enabled: true,
    }];
    state.social = Some(social);
    let mut kernel = Kernel::compile(state).unwrap();
    for _ in 0..8 {
        kernel.step().unwrap();
    }
    let result = kernel.export();
    assert!(result.families.as_ref().unwrap().totals.births > 0);
    assert_eq!(result.social.as_ref().unwrap().pressures.len(), 1);
    result.validate().unwrap();
}
