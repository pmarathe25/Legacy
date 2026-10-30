use game_parallel::{Caregiving, CaregivingPolicy, CaregivingRules, Person, World};
use game_parallel_fused_physical::Kernel;
fn world() -> World {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let content =
        World::from_toml(include_str!("../../../../content/parallel/goals.toml")).unwrap();
    world.production = content.production;
    world.autowork = content.autowork;
    world.goals = content.goals;
    world.regions[0].people.push(Person {
        id: 9,
        health: 100,
        hunger: 0,
    });
    let families = world.families.as_mut().unwrap();
    let mut child = families.members[0].clone();
    child.id = 9;
    child.age = 0;
    child.born = Some(0);
    child.parents = vec![1, 2].into();
    child.partner = None;
    child.pregnancy = None;
    child.next_conception = 0;
    child.reproduction_allowed = false;
    families.members.push(child);
    families.next_id = 10;
    world.caregiving = Some(Caregiving {
        inherit_births: false,
        rules: CaregivingRules {
            dependent_until: 5,
            max_caregiver_hunger: 10,
            neglect_damage: 3,
            feed_from_cargo: false,
            fetch_local_food: false,
            pressure_threshold: None,
        },
        policies: vec![CaregivingPolicy {
            person: 1,
            willing: true,
        }],
        events: vec![],
    });
    world
}
#[test]
fn caregiver_spends_real_time_and_neglect_harms_unattended_child() {
    let mut kernel = Kernel::compile(world()).unwrap();
    kernel.step().unwrap();
    let present = kernel.export();
    assert_eq!(
        present.caregiving.as_ref().unwrap().events[0].caregiver,
        Some(1)
    );
    assert_eq!(
        present.regions[0]
            .people
            .iter()
            .find(|p| p.id == 9)
            .unwrap()
            .health,
        100
    );
    assert!(present.production.as_ref().unwrap().jobs.is_empty());
    let mut unattended = world();
    unattended.caregiving.as_mut().unwrap().policies[0].willing = false;
    let mut kernel = Kernel::compile(unattended).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(result.caregiving.unwrap().events[0].caregiver, None);
    assert_eq!(
        result.regions[0]
            .people
            .iter()
            .find(|p| p.id == 9)
            .unwrap()
            .health,
        97
    );
    assert_eq!(result.production.unwrap().jobs[0].worker, 1);
}
#[test]
fn care_and_neglect_roll_back_if_lifecycle_fails() {
    let mut state = world();
    state.caregiving.as_mut().unwrap().policies[0].willing = false;
    state.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut kernel = Kernel::compile(state).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}
#[test]
fn authored_caregiving_fixture_runs_and_survives_reload() {
    let state =
        World::from_toml(include_str!("../../../../content/parallel/caregiving.toml")).unwrap();
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let saved = kernel.export();
    assert!(
        saved
            .caregiving
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|event| event.child == 9 && event.caregiver.is_some())
    );
    assert_eq!(
        Kernel::compile(World::from_json(&serde_json::to_string(&saved).unwrap()).unwrap())
            .unwrap()
            .export(),
        saved
    );
}
#[test]
fn child_can_be_cared_for_by_a_parent_on_the_same_journey() {
    use game_parallel::Move;
    let mut together = Kernel::compile(world()).unwrap();
    together
        .move_people(&[
            Move {
                person: 1,
                destination: 2,
            },
            Move {
                person: 9,
                destination: 2,
            },
        ])
        .unwrap();
    together.step().unwrap();
    let state = together.export();
    assert_eq!(state.caregiving.unwrap().events[0].caregiver, Some(1));
    assert_eq!(
        state
            .travelers
            .iter()
            .find(|t| t.person.id == 9)
            .unwrap()
            .person
            .health,
        100
    );
    let mut alone = Kernel::compile(world()).unwrap();
    alone
        .move_people(&[Move {
            person: 9,
            destination: 2,
        }])
        .unwrap();
    alone.step().unwrap();
    let state = alone.export();
    assert_eq!(state.caregiving.unwrap().events[0].caregiver, None);
    assert_eq!(
        state
            .travelers
            .iter()
            .find(|t| t.person.id == 9)
            .unwrap()
            .person
            .health,
        97
    );
}
#[test]
fn caregiver_can_change_preference_and_invalidate_prepared_tick() {
    use game_parallel::CaregivingPolicy;
    let mut kernel = Kernel::compile(world()).unwrap();
    let draft = kernel.prepare().unwrap();
    kernel
        .set_caregiver_policy(CaregivingPolicy {
            person: 1,
            willing: false,
        })
        .unwrap();
    assert!(kernel.commit(draft).is_err());
    let before = kernel.export();
    assert!(
        kernel
            .set_caregiver_policy(CaregivingPolicy {
                person: 9,
                willing: true
            })
            .is_err()
    );
    assert_eq!(kernel.export(), before);
    kernel.step().unwrap();
    assert_eq!(
        kernel.export().caregiving.unwrap().events[0].caregiver,
        None
    );
}

fn food_care_world() -> World {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/caregiving.toml")).unwrap();
    state.rules.food_growth = 0;
    state.regions[0].food = 2;
    state.regions[0]
        .people
        .iter_mut()
        .find(|p| p.id == 1)
        .unwrap()
        .hunger = 3;
    state.regions[0]
        .people
        .iter_mut()
        .find(|p| p.id == 9)
        .unwrap()
        .hunger = 3;
    let care = state.caregiving.as_mut().unwrap();
    care.policies.retain(|policy| policy.person == 1);
    care.rules.feed_from_cargo = true;
    state.cargo = World::from_toml(include_str!(
        "../../../../content/parallel/personal_food.toml"
    ))
    .unwrap()
    .cargo;
    let cargo = state.cargo.as_mut().unwrap();
    cargo.holdings = vec![game_parallel::Holding { person: 1, food: 2 }];
    cargo.acquire_when_hungry = vec![9];
    state
}

#[test]
fn willing_caregiver_feeds_a_dependent_from_real_custody() {
    let state = food_care_world();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                let before = kernel.export();
                let draft = kernel.prepare().unwrap();
                assert_eq!(kernel.export(), before);
                kernel.commit(draft).unwrap();
                let result = kernel.export();
                assert_eq!(
                    result.regions[0]
                        .people
                        .iter()
                        .find(|p| p.id == 1)
                        .unwrap()
                        .hunger,
                    4
                );
                assert_eq!(
                    result.regions[0]
                        .people
                        .iter()
                        .find(|p| p.id == 9)
                        .unwrap()
                        .hunger,
                    0
                );
                assert_eq!(result.regions[0].food, 2);
                assert!(result.cargo.as_ref().unwrap().holdings.is_empty());
                let restored = World::from_json(&serde_json::to_string(&result).unwrap()).unwrap();
                assert_eq!(Kernel::compile(restored).unwrap().export(), result);
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn refusal_or_missing_carried_food_does_not_feed_a_child() {
    for refuse in [true, false] {
        let mut state = food_care_world();
        if refuse {
            state.caregiving.as_mut().unwrap().policies[0].willing = false;
        } else {
            state.cargo.as_mut().unwrap().holdings[0].food = 1;
        }
        state.cargo.as_mut().unwrap().acquire_when_hungry.clear();
        let mut kernel = Kernel::compile(state).unwrap();
        kernel.step().unwrap();
        let result = kernel.export();
        assert_eq!(
            result.regions[0]
                .people
                .iter()
                .find(|p| p.id == 9)
                .unwrap()
                .hunger,
            4
        );
        assert_eq!(result.regions[0].food, 2);
    }
}

#[test]
fn dependent_feeding_rolls_back_if_a_later_phase_fails() {
    let mut state = food_care_world();
    state.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut kernel = Kernel::compile(state).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}

#[test]
fn caregiver_can_feed_a_dependent_on_the_same_journey() {
    let mut state = food_care_world();
    for id in [1, 9] {
        let index = state.regions[0]
            .people
            .iter()
            .position(|person| person.id == id)
            .unwrap();
        let person = state.regions[0].people.remove(index);
        state.travelers.push(game_parallel::Traveler {
            person,
            source: 1,
            destination: 2,
            ticks_remaining: 2,
        });
    }
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(
        result
            .travelers
            .iter()
            .find(|t| t.person.id == 9)
            .unwrap()
            .person
            .hunger,
        0
    );
    assert_eq!(
        result
            .travelers
            .iter()
            .find(|t| t.person.id == 1)
            .unwrap()
            .person
            .hunger,
        4
    );
    assert!(result.cargo.unwrap().holdings.is_empty());
}

#[test]
fn a_child_who_can_eat_their_own_food_is_not_fed_a_second_meal() {
    let mut state = food_care_world();
    state
        .cargo
        .as_mut()
        .unwrap()
        .holdings
        .push(game_parallel::Holding { person: 9, food: 2 });
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(
        result.regions[0]
            .people
            .iter()
            .find(|p| p.id == 9)
            .unwrap()
            .hunger,
        0
    );
    assert_eq!(
        result.regions[0]
            .people
            .iter()
            .find(|p| p.id == 1)
            .unwrap()
            .hunger,
        0
    );
    assert!(result.cargo.unwrap().holdings.is_empty());
}

#[test]
fn caregiver_fetches_real_local_food_for_a_dependent() {
    let mut state = food_care_world();
    state.cargo.as_mut().unwrap().holdings.clear();
    state.caregiving.as_mut().unwrap().rules.fetch_local_food = true;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                let before = kernel.export();
                let draft = kernel.prepare().unwrap();
                assert_eq!(kernel.export(), before);
                kernel.commit(draft).unwrap();
                let result = kernel.export();
                assert_eq!(
                    result.regions[0]
                        .people
                        .iter()
                        .find(|p| p.id == 9)
                        .unwrap()
                        .hunger,
                    0
                );
                assert_eq!(result.regions[0].food, 0);
                assert!(result.cargo.as_ref().unwrap().holdings.is_empty());
                let restored = World::from_json(&serde_json::to_string(&result).unwrap()).unwrap();
                assert_eq!(Kernel::compile(restored).unwrap().export(), result);
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn caregiver_fetch_competes_with_other_local_acquisitions() {
    let mut state = food_care_world();
    state.cargo.as_mut().unwrap().holdings.clear();
    state.cargo.as_mut().unwrap().acquire_when_hungry = vec![2, 9];
    state.regions[0]
        .people
        .iter_mut()
        .find(|p| p.id == 2)
        .unwrap()
        .hunger = 3;
    state.caregiving.as_mut().unwrap().rules.fetch_local_food = true;
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    let child_hunger = result.regions[0]
        .people
        .iter()
        .find(|p| p.id == 9)
        .unwrap()
        .hunger;
    let holdings = &result.cargo.as_ref().unwrap().holdings;
    assert!(
        (child_hunger == 0 && holdings.is_empty())
            || (child_hunger == 4 && holdings == &[game_parallel::Holding { person: 2, food: 2 }])
    );
    assert_eq!(result.regions[0].food, 0);
}

#[test]
fn a_caregiver_cannot_fetch_a_meal_larger_than_their_capacity() {
    let mut state = food_care_world();
    let cargo = state.cargo.as_mut().unwrap();
    cargo.holdings.clear();
    cargo.capacity = 1;
    state.caregiving.as_mut().unwrap().rules.fetch_local_food = true;
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(
        result.regions[0]
            .people
            .iter()
            .find(|p| p.id == 9)
            .unwrap()
            .hunger,
        4
    );
    assert_eq!(result.regions[0].food, 2);
}

#[test]
fn many_caregivers_share_one_scarce_food_source_deterministically() {
    let mut state = food_care_world();
    let count = 5000u64;
    state.rules.food_capacity = 20000;
    state.regions[0].food = 5000;
    state.regions[0].people = (1..=count * 3)
        .map(|id| Person {
            id,
            health: 100,
            hunger: if id > count * 2 { 3 } else { 0 },
        })
        .collect();
    state.regions[1].people.clear();
    let family = state.families.as_mut().unwrap();
    let parent = family
        .members
        .iter()
        .find(|member| member.id == 1)
        .unwrap()
        .clone();
    let child = family
        .members
        .iter()
        .find(|member| member.id == 9)
        .unwrap()
        .clone();
    family.members = (1..=count * 2)
        .map(|id| {
            let mut member = parent.clone();
            member.id = id;
            member.parents.clear();
            member.partner = None;
            member.pregnancy = None;
            member.reproduction_allowed = false;
            member
        })
        .chain((1..=count).map(|id| {
            let mut member = child.clone();
            member.id = id + count * 2;
            member.parents = vec![id, id + count].into();
            member.reproduction_allowed = false;
            member
        }))
        .collect();
    family.next_id = count * 3 + 1;
    let care = state.caregiving.as_mut().unwrap();
    care.rules.fetch_local_food = true;
    care.policies = (1..=count)
        .map(|person| CaregivingPolicy {
            person,
            willing: true,
        })
        .collect();
    let cargo = state.cargo.as_mut().unwrap();
    cargo.holdings.clear();
    cargo.acquire_when_hungry.clear();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                assert_eq!(result.regions[0].food, 0);
                assert_eq!(
                    result.regions[0]
                        .people
                        .iter()
                        .filter(|p| p.id > count * 2 && p.hunger == 0)
                        .count(),
                    2500
                );
                assert_eq!(
                    result.regions[0]
                        .people
                        .iter()
                        .filter(|p| p.id > count * 2 && p.hunger == 4)
                        .count(),
                    2500
                );
                assert!(result.cargo.as_ref().unwrap().holdings.is_empty());
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn a_person_can_care_from_fear_and_stop_when_the_fear_fades() {
    let mut state = food_care_world();
    state.cargo.as_mut().unwrap().acquire_when_hungry.clear();
    let care = state.caregiving.as_mut().unwrap();
    care.policies[0].willing = false;
    care.rules.pressure_threshold = Some(3);
    state.opinions = World::from_toml(include_str!("../../../../content/parallel/food_aid.toml"))
        .unwrap()
        .opinions;
    state.opinions.as_mut().unwrap().records[0].target = 9;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                kernel.step().unwrap();
                let first = kernel.export();
                assert_eq!(
                    first.caregiving.as_ref().unwrap().events[0].caregiver,
                    Some(1)
                );
                assert!(!first.caregiving.as_ref().unwrap().policies[0].willing);
                assert_eq!(
                    first.regions[0]
                        .people
                        .iter()
                        .find(|p| p.id == 9)
                        .unwrap()
                        .hunger,
                    0
                );
                assert_eq!(first.opinions.as_ref().unwrap().index().fear(1, 9), 2);
                let restored = World::from_json(&serde_json::to_string(&first).unwrap()).unwrap();
                let mut restored = Kernel::compile(restored).unwrap();
                kernel.step().unwrap();
                restored.step().unwrap();
                let second = kernel.export();
                assert_eq!(second, restored.export());
                assert_eq!(
                    second.caregiving.as_ref().unwrap().events[0].caregiver,
                    None
                );
                assert_eq!(
                    second.regions[0]
                        .people
                        .iter()
                        .find(|p| p.id == 9)
                        .unwrap()
                        .health,
                    97
                );
                second
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn felt_obligation_can_motivate_care_without_private_willingness() {
    let mut state = food_care_world();
    let care = state.caregiving.as_mut().unwrap();
    care.policies[0].willing = false;
    care.rules.pressure_threshold = Some(3);
    state.opinions = World::from_toml(include_str!("../../../../content/parallel/food_aid.toml"))
        .unwrap()
        .opinions;
    let record = &mut state.opinions.as_mut().unwrap().records[0];
    record.target = 9;
    record.fear = 0;
    record.obligation = 3;
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(
        result.caregiving.as_ref().unwrap().events[0].caregiver,
        Some(1)
    );
    assert!(!result.caregiving.unwrap().policies[0].willing);
}
