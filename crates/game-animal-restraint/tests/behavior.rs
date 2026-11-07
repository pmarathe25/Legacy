use game_animal_restraint::{
    Context, DefensePolicy, EventKind, Hold, Manual, Policy, ReleasePolicy, Restraints,
};
use game_economy_person::Person;
use game_fauna::{Animal, Fauna, SocialResponse, Species, restraint};
use game_genetics::{Genome, Sex};

fn fixture(count: u64, shared: bool) -> (Vec<Person>, Fauna, Restraints) {
    let people = (1..=count)
        .map(|id| Person {
            id,
            region: Some(1),
            health: 100,
            hunger: 0,
            immortal: None,
            willing_to_work: false,
            seek_meals: false,
            controlled: false,
        })
        .collect();
    let animals = if shared { 1 } else { count };
    let fauna = Fauna {
        last_tick: 0,
        next_id: animals + 1,
        species: vec![Species {
            id: 1,
            name: "animal".into(),
            food_good: 1,
            intake: 1,
            hunger_per_tick: 1,
            relief_per_unit: 1,
            starvation_threshold: 100,
            starvation_damage: 1,
            max_health: 10,
            adult_age: 1,
            fertile_until: 10,
            lifespan: 20,
            gestation: 2,
            conception_one_in: 100,
            breeding_max_hunger: 10,
            newborn_health: 10,
            mutation_per_million: 0,
            mutation_step: 0,
            climate: None,
            hunting: None,
            newborn_social_response: None,
            newborn_handling_response: None,
            newborn_restraint_response: None,
        }],
        animals: (1..=animals)
            .map(|id| Animal {
                id,
                species: 1,
                region: 1,
                age: 2,
                health: 10,
                hunger: 0,
                genome: Genome {
                    sex: Sex::Male,
                    alleles: [[500; 2]; 4],
                },
                pregnancy: None,
                parents: None,
                social_response: Some(SocialResponse {
                    feed_affinity: 0,
                    minimum: -10,
                    maximum: 10,
                    capacity: 4,
                }),
                handling_response: None,
                restraint_response: Some(restraint::Response {
                    yield_willingly: false,
                    yield_affinity_at_least: Some(2),
                    yield_affinity_at_most: None,
                    resistance: 3,
                    escape_willing: true,
                    escape_effort: 1,
                    on_yielded: -1,
                    on_forced: -3,
                    on_resisted: 2,
                    on_released: 1,
                    on_release_blocked: 0,
                }),
                opinions: vec![],
            })
            .collect(),
    };
    let owner = Restraints {
        inherit_births: false,
        default_effort: 2,
        capacity_per_handler: 1,
        release_policies: vec![].into(),
        defense_policies: vec![].into(),
        holds: vec![].into(),
        events: vec![],
        policies: (1..=count)
            .map(|person| Policy {
                person,
                animal: Some(if shared { 1 } else { person }),
                willing: true,
                affinity_at_least: None,
                affinity_at_most: None,
                effort: 2,
                pressured_by: None,
                pressure_threshold: None,
            })
            .collect::<Vec<_>>()
            .into(),
    };
    (people, fauna, owner)
}

fn prepare(
    owner: &Restraints,
    fauna: &Fauna,
    people: &[Person],
    tick: u64,
    manual: Option<Manual>,
) -> game_animal_restraint::Plan {
    Context {
        owner: Some(owner),
        fauna: Some(fauna),
        people,
        opinions: None,
        affinity: None,
        busy: &[],
        blocked: &[],
        max_hunger: 100,
        seed: 7,
        tick,
        manual,
    }
    .prepare()
    .unwrap()
}

#[test]
fn restraint_lookup_handles_sparse_and_reordered_animals_without_changing_results() {
    let (people, mut fauna, mut owner) = fixture(64, false);
    for animal in &mut fauna.animals {
        animal.id *= 2;
    }
    fauna.next_id = 129;
    for policy in owner.policies_mut() {
        policy.animal = policy.animal.map(|id| id * 2);
        policy.effort = 4;
    }
    let mut reversed = fauna.clone();
    reversed.animals.reverse();
    let mut valid_a = owner.clone();
    let mut valid_b = owner.clone();
    valid_a
        .normalize_and_validate(&people, Some(&fauna), 0)
        .unwrap();
    valid_b
        .normalize_and_validate(&people, Some(&reversed), 0)
        .unwrap();
    assert_eq!(valid_a, valid_b);
    let a = prepare(&valid_a, &fauna, &people, 1, None);
    let b = prepare(&valid_b, &reversed, &people, 1, None);
    assert_eq!(a.state, b.state);
    assert_eq!(a.actors, b.actors);
    assert_eq!(a.reactions, b.reactions);
    fauna.animals[4].health = 0;
    reversed
        .animals
        .iter_mut()
        .find(|animal| animal.id == 10)
        .unwrap()
        .health = 0;
    let mut next_a = a.state.unwrap();
    let mut next_b = b.state.unwrap();
    next_a.reconcile(&people, &fauna, &|_| true, 1);
    next_b.reconcile(&people, &reversed, &|_| true, 1);
    assert_eq!(next_a, next_b);
    assert!(
        next_a
            .events
            .iter()
            .any(|event| event.animal == 10 && event.kind == EventKind::LetGo)
    );
}

#[test]
fn an_animal_can_resist_then_yield_and_escape_without_losing_identity() {
    let (people, mut fauna, owner) = fixture(1, true);
    let first = prepare(&owner, &fauna, &people, 1, None);
    assert_eq!(
        first.state.as_ref().unwrap().events[0].kind,
        EventKind::Resisted
    );
    restraint::apply_reactions(&mut fauna, &first.reactions);
    assert_eq!(fauna.animals[0].opinions[0].affinity, 2);
    let second = prepare(first.state.as_ref().unwrap(), &fauna, &people, 2, None);
    assert_eq!(
        second.state.as_ref().unwrap().events[0].kind,
        EventKind::Yielded
    );
    assert!(second.state.as_ref().unwrap().held(1));
    restraint::apply_reactions(&mut fauna, &second.reactions);
    assert_eq!(fauna.animals[0].opinions[0].affinity, 1);
    let third = prepare(second.state.as_ref().unwrap(), &fauna, &people, 3, None);
    assert_eq!(
        third.state.as_ref().unwrap().events[0].kind,
        EventKind::Struggled
    );
    let fourth = prepare(third.state.as_ref().unwrap(), &fauna, &people, 4, None);
    assert_eq!(
        fourth.state.as_ref().unwrap().events[0].kind,
        EventKind::Escaped
    );
    assert!(!fourth.state.as_ref().unwrap().held(1));
    assert_eq!((fauna.animals[0].id, fauna.animals[0].health), (1, 10));
    let saved: Restraints =
        serde_json::from_str(&serde_json::to_string(fourth.state.as_ref().unwrap()).unwrap())
            .unwrap();
    assert_eq!(&saved, fourth.state.as_ref().unwrap());
}

#[test]
fn force_can_overcome_resistance_and_another_person_can_release() {
    let (mut people, fauna, mut owner) = fixture(2, true);
    owner.policies_mut()[0].effort = 4;
    owner.policies_mut()[1].willing = false;
    let captured = prepare(&owner, &fauna, &people, 1, None);
    assert_eq!(
        captured.state.as_ref().unwrap().events[0].kind,
        EventKind::Forced
    );
    people[1].controlled = true;
    let released = prepare(
        captured.state.as_ref().unwrap(),
        &fauna,
        &people,
        2,
        Some(Manual::Release {
            person: 2,
            animal: 1,
        }),
    );
    assert_eq!(
        released.state.as_ref().unwrap().events[0].kind,
        EventKind::Released
    );
    assert!(!released.state.as_ref().unwrap().held(1));
    let mut departed = captured.state.unwrap();
    people[0].region = None;
    departed.reconcile(&people, &fauna, &|_| true, 2);
    assert!(!departed.held(1));
    assert!(
        departed
            .events
            .iter()
            .any(|event| event.kind == EventKind::LetGo)
    );
    assert_eq!(fauna.animals[0].region, 1);
}

#[test]
fn concentrated_and_independent_captures_are_deterministic() {
    for shared in [false, true] {
        let count = 4096;
        let (people, mut fauna, mut owner) = fixture(count, shared);
        for animal in &mut fauna.animals {
            animal.restraint_response.as_mut().unwrap().yield_willingly = true;
        }
        owner
            .normalize_and_validate(&people, Some(&fauna), 0)
            .unwrap();
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| prepare(&owner, &fauna, &people, 1, None))
        };
        let one = run(1);
        let four = run(4);
        assert_eq!(one.actors.len(), count as usize);
        assert_eq!(
            one.state.as_ref().unwrap().holds.len(),
            if shared { 1 } else { count as usize }
        );
        assert_eq!(one.state, four.state);
        assert_eq!(one.actors, four.actors);
    }
}

#[test]
fn autonomous_releases_arbitrate_per_animal_and_precede_escape() {
    for shared in [false, true] {
        let count = 4096;
        let (people, fauna, mut owner) = fixture(count, shared);
        for policy in owner.policies_mut() {
            policy.willing = false;
        }
        *owner.holds_mut() = (1..=if shared { 1 } else { count })
            .map(|animal| Hold {
                animal,
                handler: animal,
                integrity: 2,
            })
            .collect();
        *owner.release_policies_mut() = (1..=count)
            .map(|person| ReleasePolicy {
                person,
                animal: Some(if shared { 1 } else { person }),
                willing: true,
                effort: 1,
                affinity_at_least: None,
                affinity_at_most: None,
                pressured_by: None,
                pressure_threshold: None,
            })
            .collect();
        owner
            .normalize_and_validate(&people, Some(&fauna), 0)
            .unwrap();
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| prepare(&owner, &fauna, &people, 1, None))
        };
        let one = run(1);
        let four = run(4);
        assert_eq!(one.state, four.state);
        assert_eq!(one.actors, four.actors);
        assert_eq!(one.actors.len(), count as usize);
        let state = one.state.unwrap();
        assert!(state.holds.is_empty());
        assert_eq!(state.events.len(), if shared { 1 } else { count as usize });
        if shared {
            let expected = (1..=count)
                .min_by_key(|&person| (game_genetics::random(7 ^ 1, person, 1), person))
                .unwrap();
            assert_eq!(state.events[0].actor, Some(expected));
        }
        assert!(
            state
                .events
                .iter()
                .all(|event| event.kind == EventKind::Released)
        );
    }
}

#[test]
fn flexible_releaser_selects_an_unblocked_local_hold() {
    let (people, fauna, mut owner) = fixture(2, false);
    owner.policies_mut().clear();
    *owner.holds_mut() = vec![
        Hold {
            animal: 1,
            handler: 1,
            integrity: 2,
        },
        Hold {
            animal: 2,
            handler: 2,
            integrity: 2,
        },
    ];
    *owner.release_policies_mut() = vec![ReleasePolicy {
        person: 1,
        animal: None,
        willing: true,
        effort: 1,
        affinity_at_least: None,
        affinity_at_most: None,
        pressured_by: None,
        pressure_threshold: None,
    }];
    let result = Context {
        owner: Some(&owner),
        fauna: Some(&fauna),
        people: &people,
        opinions: None,
        affinity: None,
        busy: &[],
        blocked: &[1],
        max_hunger: 100,
        seed: 7,
        tick: 1,
        manual: None,
    }
    .prepare()
    .unwrap()
    .state
    .unwrap();
    assert!(result.held(1));
    assert!(!result.held(2));
    assert!(result.events.iter().any(|event| event.animal == 2
        && event.actor == Some(1)
        && event.kind == EventKind::Released));
}

#[test]
fn a_handler_can_oppose_release_but_can_be_overpowered() {
    let (mut people, mut fauna, mut owner) = fixture(2, true);
    people[1].controlled = true;
    owner.policies_mut().clear();
    *owner.holds_mut() = vec![Hold {
        animal: 1,
        handler: 1,
        integrity: 2,
    }];
    *owner.defense_policies_mut() = vec![DefensePolicy {
        person: 1,
        willing: true,
        effort: 2,
        pressured_by: None,
        pressure_threshold: None,
    }];
    *owner.release_policies_mut() = vec![ReleasePolicy {
        person: 2,
        animal: Some(1),
        willing: false,
        effort: 2,
        affinity_at_least: None,
        affinity_at_most: None,
        pressured_by: None,
        pressure_threshold: None,
    }];
    fauna.animals[0]
        .restraint_response
        .as_mut()
        .unwrap()
        .on_release_blocked = -3;
    let blocked = prepare(
        &owner,
        &fauna,
        &people,
        1,
        Some(Manual::Release {
            person: 2,
            animal: 1,
        }),
    );
    assert_eq!(blocked.actors, vec![1, 2]);
    assert_eq!(
        blocked.state.as_ref().unwrap().events[0].kind,
        EventKind::ReleaseBlocked
    );
    assert_eq!(blocked.state.as_ref().unwrap().events[0].defender, Some(1));
    assert_eq!(blocked.state.as_ref().unwrap().holds[0].integrity, 2);
    restraint::apply_reactions(&mut fauna, &blocked.reactions);
    assert_eq!(fauna.animals[0].opinions[0].affinity, -3);
    owner.release_policies_mut()[0].effort = 3;
    let released = prepare(
        &owner,
        &fauna,
        &people,
        1,
        Some(Manual::Release {
            person: 2,
            animal: 1,
        }),
    );
    assert_eq!(released.actors, vec![1, 2]);
    assert_eq!(
        released.state.as_ref().unwrap().events[0].kind,
        EventKind::Released
    );
    assert_eq!(released.state.as_ref().unwrap().events[0].defender, Some(1));
    assert!(released.state.unwrap().holds.is_empty());
}

#[test]
fn a_handler_cannot_defend_two_animals_at_once() {
    let (people, fauna, mut owner) = fixture(4, false);
    owner.policies_mut().clear();
    owner.capacity_per_handler = 2;
    *owner.holds_mut() = vec![
        Hold {
            animal: 1,
            handler: 1,
            integrity: 2,
        },
        Hold {
            animal: 2,
            handler: 1,
            integrity: 2,
        },
    ];
    *owner.defense_policies_mut() = vec![DefensePolicy {
        person: 1,
        willing: true,
        effort: 1,
        pressured_by: None,
        pressure_threshold: None,
    }];
    *owner.release_policies_mut() = [(3, 1), (4, 2)]
        .into_iter()
        .map(|(person, animal)| ReleasePolicy {
            person,
            animal: Some(animal),
            willing: true,
            effort: 1,
            affinity_at_least: None,
            affinity_at_most: None,
            pressured_by: None,
            pressure_threshold: None,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| prepare(&owner, &fauna, &people, 1, None))
    };
    let one = run(1);
    let four = run(4);
    assert_eq!(one.state, four.state);
    assert_eq!(one.actors, vec![1, 3, 4]);
    let state = one.state.unwrap();
    assert_eq!(state.holds.len(), 1);
    let expected = [1, 2]
        .into_iter()
        .min_by_key(|&animal| (game_genetics::random(7 ^ 1, animal, 1), animal))
        .unwrap();
    assert_eq!(state.holds[0].animal, expected);
    assert_eq!(
        state
            .events
            .iter()
            .filter(|event| event.kind == EventKind::ReleaseBlocked)
            .count(),
        1
    );
    assert_eq!(
        state
            .events
            .iter()
            .filter(|event| event.kind == EventKind::Released)
            .count(),
        1
    );
}

#[test]
fn a_controlled_handler_can_explicitly_defend_a_hold() {
    let (mut people, fauna, mut owner) = fixture(2, true);
    people[0].controlled = true;
    owner.policies_mut().clear();
    *owner.holds_mut() = vec![Hold {
        animal: 1,
        handler: 1,
        integrity: 2,
    }];
    *owner.release_policies_mut() = vec![ReleasePolicy {
        person: 2,
        animal: Some(1),
        willing: true,
        effort: 2,
        affinity_at_least: None,
        affinity_at_most: None,
        pressured_by: None,
        pressure_threshold: None,
    }];
    let plan = prepare(
        &owner,
        &fauna,
        &people,
        1,
        Some(Manual::Defend {
            person: 1,
            animal: 1,
        }),
    );
    assert_eq!(plan.actors, vec![1, 2]);
    assert_eq!(
        plan.state.as_ref().unwrap().events[0].kind,
        EventKind::ReleaseBlocked
    );
    assert!(plan.state.unwrap().held(1));
}

#[test]
fn older_release_events_load_without_a_defender() {
    let event: game_animal_restraint::Event =
        serde_json::from_str(r#"{"tick":1,"animal":1,"actor":2,"kind":"released"}"#).unwrap();
    assert_eq!(event.defender, None);
    assert_eq!(event.kind, EventKind::Released);
}

#[test]
fn policy_generations_are_shared_until_an_individual_choice_changes() {
    let (people, fauna, owner) = fixture(2, true);
    let plan = prepare(&owner, &fauna, &people, 1, None);
    let mut next = plan.state.unwrap();
    assert!(std::sync::Arc::ptr_eq(&owner.policies, &next.policies));
    assert!(std::sync::Arc::ptr_eq(
        &owner.release_policies,
        &next.release_policies
    ));
    assert!(std::sync::Arc::ptr_eq(
        &owner.defense_policies,
        &next.defense_policies
    ));
    next.policies_mut()[0].willing = false;
    assert!(owner.policies[0].willing);
    assert!(!next.policies[0].willing);
    assert!(!std::sync::Arc::ptr_eq(&owner.policies, &next.policies));
    assert!(std::sync::Arc::ptr_eq(
        &owner.release_policies,
        &next.release_policies
    ));
    let saved: Restraints = serde_json::from_str(&serde_json::to_string(&next).unwrap()).unwrap();
    assert_eq!(saved, next);
    let mut reversed = owner.clone();
    reversed.policies_mut().reverse();
    reversed
        .normalize_and_validate(&people, Some(&fauna), 0)
        .unwrap();
    assert_eq!(reversed.policies, owner.policies);
    assert_eq!(owner.policies[0].person, 1);
}

#[test]
fn unchanged_holds_share_a_generation_and_real_changes_detach_it() {
    let (people, mut fauna, mut owner) = fixture(2, true);
    fauna.animals[0]
        .restraint_response
        .as_mut()
        .unwrap()
        .escape_willing = false;
    owner.policies_mut().clear();
    owner.holds_mut().push(Hold {
        animal: 1,
        handler: 1,
        integrity: 2,
    });
    let snapshot = owner.clone();
    let mut next = prepare(&owner, &fauna, &people, 1, None).state.unwrap();
    assert!(std::sync::Arc::ptr_eq(&owner.holds, &next.holds));
    next.events = vec![
        game_animal_restraint::Event {
            tick: 1,
            animal: 1,
            actor: Some(2),
            defender: None,
            kind: EventKind::Resisted,
        },
        game_animal_restraint::Event {
            tick: 1,
            animal: 1,
            actor: Some(1),
            defender: None,
            kind: EventKind::Resisted,
        },
    ];
    next.reconcile(&people, &fauna, &|_| true, 1);
    assert!(std::sync::Arc::ptr_eq(&owner.holds, &next.holds));
    assert_eq!(
        next.events
            .iter()
            .map(|event| event.actor)
            .collect::<Vec<_>>(),
        vec![Some(1), Some(2)]
    );
    next.holds_mut().clear();
    assert_eq!(owner, snapshot);
    assert_eq!(owner.holds.len(), 1);
    assert!(!std::sync::Arc::ptr_eq(&owner.holds, &next.holds));
    let restored: Restraints =
        serde_json::from_str(&serde_json::to_string(&owner).unwrap()).unwrap();
    assert_eq!(restored, owner);
    *owner.release_policies_mut() = vec![ReleasePolicy {
        person: 2,
        animal: Some(1),
        willing: true,
        effort: 1,
        affinity_at_least: None,
        affinity_at_most: None,
        pressured_by: None,
        pressure_threshold: None,
    }];
    let released = prepare(&owner, &fauna, &people, 1, None).state.unwrap();
    assert!(released.holds.is_empty());
    assert_eq!(owner.holds.len(), 1);
}

#[test]
#[ignore = "manual steady physical holds benchmark"]
fn steady_holds_benchmark() {
    let count = std::env::var("LEGACY_ANIMALS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(100_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let (people, mut fauna, mut owner) = fixture(count, false);
    owner.policies_mut().clear();
    *owner.holds_mut() = (1..=count)
        .map(|id| Hold {
            animal: id,
            handler: id,
            integrity: 2,
        })
        .collect();
    for animal in &mut fauna.animals {
        animal.restraint_response.as_mut().unwrap().escape_willing = false;
    }
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..5 {
                let now = std::time::Instant::now();
                let mut next = prepare(&owner, &fauna, &people, 1, None).state.unwrap();
                next.reconcile(&people, &fauna, &|_| true, 1);
                let elapsed = now.elapsed().as_secs_f64();
                assert!(std::sync::Arc::ptr_eq(&owner.holds, &next.holds));
                assert_eq!(next.holds.len(), count as usize);
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} animals={count} elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn release_and_escape_match_when_capture_preferences_are_absent_or_disabled() {
    let (people, fauna, mut disabled) = fixture(2, true);
    for policy in disabled.policies_mut() {
        policy.willing = false;
    }
    *disabled.holds_mut() = vec![Hold {
        animal: 1,
        handler: 1,
        integrity: 2,
    }];
    *disabled.release_policies_mut() = vec![ReleasePolicy {
        person: 2,
        animal: Some(1),
        willing: true,
        effort: 1,
        affinity_at_least: None,
        affinity_at_most: None,
        pressured_by: None,
        pressure_threshold: None,
    }];
    let compare = |disabled: &Restraints| {
        let mut absent = disabled.clone();
        absent.policies_mut().clear();
        let fast = prepare(&absent, &fauna, &people, 1, None);
        let ordinary = prepare(disabled, &fauna, &people, 1, None);
        assert_eq!(fast.actors, ordinary.actors);
        assert_eq!(fast.reserved, ordinary.reserved);
        assert_eq!(fast.reactions, ordinary.reactions);
        assert_eq!(
            fast.state.as_ref().unwrap().holds,
            ordinary.state.as_ref().unwrap().holds
        );
        assert_eq!(
            fast.state.as_ref().unwrap().events,
            ordinary.state.as_ref().unwrap().events
        );
    };
    compare(&disabled);
    let mut declined_by_affinity = disabled.clone();
    for policy in declined_by_affinity.policies_mut() {
        policy.willing = true;
        policy.affinity_at_least = Some(1);
    }
    compare(&declined_by_affinity);
    disabled.release_policies_mut().clear();
    compare(&disabled);
}
