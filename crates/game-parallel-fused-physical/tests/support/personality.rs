use game_parallel::{Person, Personality, PersonalityPerson, SocialParticipant, StartWork, World};
use game_parallel_fused_physical::Kernel;

fn social_world() -> World {
    let mut state = World::from_toml(include_str!(
        "../../../../content/parallel/personality.toml"
    ))
    .unwrap();
    let person = state.regions[1].people.pop().unwrap();
    state.regions[0].people.push(person);
    state
}

#[test]
fn sociability_limits_encounters_until_isolation_makes_them_urgent() {
    let mut kernel = Kernel::compile(social_world()).unwrap();
    kernel.step().unwrap();
    let first = kernel.export();
    let event = &first.social.as_ref().unwrap().events[0];
    assert_eq!((event.a, event.b), (2, 3));
    assert_eq!(
        first
            .social
            .unwrap()
            .needs
            .unwrap()
            .people
            .iter()
            .find(|row| row.person == 1)
            .unwrap()
            .isolation,
        2
    );
    let saved = serde_json::to_string(&kernel.export()).unwrap();
    kernel = Kernel::compile(World::from_json(&saved).unwrap()).unwrap();
    kernel.step().unwrap();
    let state = kernel.export();
    let event = &state.social.as_ref().unwrap().events[0];
    assert_eq!(event.a, 1);
}

#[test]
fn diligence_affects_automatic_work_but_not_direct_commands() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/goals.toml")).unwrap();
    state.personality = Some(Personality {
        social_urgency_per_isolation: 0,
        inherit_births: false,
        people: vec![PersonalityPerson {
            person: 1,
            sociability: 1000,
            diligence: 0,
        }],
    });
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    assert!(kernel.export().production.unwrap().jobs.is_empty());
    kernel
        .start_work(&[StartWork {
            id: 1,
            worker: 1,
            region: 1,
            recipe: 1,
            units: 1,
        }])
        .unwrap();
    assert_eq!(kernel.export().production.unwrap().jobs[0].worker, 1);
}

#[test]
fn invalid_personality_and_unsupported_hosts_are_rejected() {
    let state = social_world();
    let mut editable = state.clone();
    assert!(editable.step().is_err());
    assert_eq!(editable, state);
    assert!(game_parallel_blocks::Engine::compile(state.clone()).is_err());
    let mut invalid = state.clone();
    invalid.personality.as_mut().unwrap().people[0].diligence = 1001;
    assert!(invalid.validate().is_err());
    let mut invalid = state.clone();
    invalid.personality.as_mut().unwrap().people[0].person = 999;
    assert!(invalid.validate().is_err());
    let mut invalid = state;
    let duplicate = invalid.personality.as_ref().unwrap().people[0].clone();
    invalid.personality.as_mut().unwrap().people.push(duplicate);
    assert!(invalid.validate().is_err());
}

#[test]
fn personality_choices_survive_reload_and_worker_count_changes() {
    let mut state = social_world();
    state.regions[0].people = (1..=5000)
        .map(|id| Person {
            id,
            hunger: 0,
            health: 100,
        })
        .collect();
    state.rules.hunger_per_tick = 0;
    state.social.as_mut().unwrap().participants = (1..=5000)
        .map(|person| SocialParticipant {
            person,
            willing: true,
        })
        .collect();
    state.personality.as_mut().unwrap().people = (1..=5000)
        .map(|person| PersonalityPerson {
            person,
            sociability: 350,
            diligence: 700,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                for tick in 1..=4 {
                    kernel.step().unwrap();
                    if tick == 2 {
                        kernel = Kernel::compile(
                            World::from_json(&serde_json::to_string(&kernel.export()).unwrap())
                                .unwrap(),
                        )
                        .unwrap();
                    }
                }
                kernel.export()
            })
    };
    let serial = run(1);
    assert_eq!(serial, run(4));
    serial.validate().unwrap();
}

#[test]
fn later_failure_does_not_publish_personality_decisions() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/enrollment.toml")).unwrap();
    state.families.as_mut().unwrap().totals.partnerships = u64::MAX;
    state.personality = Some(Personality {
        social_urgency_per_isolation: 0,
        inherit_births: false,
        people: vec![PersonalityPerson {
            person: 1,
            sociability: 1000,
            diligence: 1000,
        }],
    });
    let mut kernel = Kernel::compile(state.clone()).unwrap();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), state);
}

#[test]
fn newborns_inherit_recorded_parent_traits_even_without_other_enrollment() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/enrollment.toml")).unwrap();
    state.families.as_mut().unwrap().rules.enroll_births = false;
    state.personality = Some(Personality {
        social_urgency_per_isolation: 0,
        inherit_births: true,
        people: (1..=8)
            .map(|person| PersonalityPerson {
                person,
                sociability: (person * 100) as u16,
                diligence: (person * 50) as u16,
            })
            .collect(),
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                for tick in 1..=8 {
                    kernel.step().unwrap();
                    if tick == 4 {
                        kernel = Kernel::compile(
                            World::from_json(&serde_json::to_string(&kernel.export()).unwrap())
                                .unwrap(),
                        )
                        .unwrap();
                    }
                }
                kernel.export()
            })
    };
    let result = run(1);
    assert_eq!(result, run(4));
    let family = result.families.as_ref().unwrap();
    let traits = &result.personality.as_ref().unwrap().people;
    let children: Vec<_> = family.members.iter().filter(|row| row.id >= 9).collect();
    assert!(!children.is_empty());
    for child in children {
        let child_traits = traits.iter().find(|row| row.person == child.id).unwrap();
        let parents: Vec<_> = child
            .parents
            .iter()
            .map(|id| traits.iter().find(|row| row.person == *id).unwrap())
            .collect();
        assert_eq!(
            child_traits.sociability as u32,
            parents.iter().map(|p| p.sociability as u32).sum::<u32>() / parents.len() as u32
        );
        assert_eq!(
            child_traits.diligence as u32,
            parents.iter().map(|p| p.diligence as u32).sum::<u32>() / parents.len() as u32
        );
    }
    result.validate().unwrap();
}

#[test]
fn declining_work_does_not_prevent_a_hungry_person_from_seeking_food() {
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/personal_food.toml"
    ))
    .unwrap();
    let source = World::from_toml(include_str!(
        "../../../../content/parallel/work_requests.toml"
    ))
    .unwrap();
    world.production = source.production;
    world.autowork = source.autowork;
    world
        .autowork
        .as_mut()
        .unwrap()
        .policies
        .push(game_parallel::WorkPolicy {
            person: 1,
            recipe: 1,
            enabled: true,
            disregard_claims: false,
        });
    world.personality = Some(Personality {
        social_urgency_per_isolation: 0,
        inherit_births: false,
        people: vec![PersonalityPerson {
            person: 1,
            sociability: 0,
            diligence: 0,
        }],
    });
    let cargo = world.cargo.as_mut().unwrap();
    cargo.holdings.clear();
    cargo.acquire_when_hungry = vec![1];
    let mut kernel = Kernel::compile(world).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert!(result.production.unwrap().jobs.is_empty());
    assert_eq!(result.cargo.unwrap().holdings[0].person, 1);
}
