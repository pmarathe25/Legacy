use game_parallel::{StartWork, World};
use game_parallel_fused_physical::Kernel;
fn world() -> World {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/guardians.toml")).unwrap();
    let social = World::from_toml(include_str!(
        "../../../../content/parallel/social_needs.toml"
    ))
    .unwrap();
    let knowledge =
        World::from_toml(include_str!("../../../../content/parallel/knowledge.toml")).unwrap();
    let work = World::from_toml(include_str!("../../../../content/parallel/goals.toml")).unwrap();
    state.social = social.social;
    state.knowledge = knowledge.knowledge;
    state.production = work.production;
    state.autowork = work.autowork;
    state.goals = work.goals;
    state.goals.as_mut().unwrap().people[0].person = 2;
    state
}

#[test]
fn fear_and_obligation_can_motivate_an_unwilling_person_without_granting_trust() {
    use game_parallel::{Opinion, SocialPressurePolicy};
    let run = |workers, fear, obligation| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = world();
                state.autowork = None;
                state.goals = None;
                state
                    .social
                    .as_mut()
                    .unwrap()
                    .participants
                    .iter_mut()
                    .find(|p| p.person == 2)
                    .unwrap()
                    .willing = false;
                state.social.as_mut().unwrap().pressures = vec![SocialPressurePolicy {
                    person: 2,
                    fear_at_least: Some(2),
                    obligation_at_least: Some(3),
                    enabled: false,
                }];
                let mut opinions =
                    World::from_toml(include_str!("../../../../content/parallel/campaign.toml"))
                        .unwrap()
                        .opinions
                        .unwrap();
                opinions.rules.fear_decay_per_tick = 0;
                opinions.records = vec![Opinion {
                    observer: 2,
                    target: 1,
                    trust: 0,
                    fear,
                    obligation,
                    affinity: 0,
                }];
                state.opinions = Some(opinions);
                let mut kernel = Kernel::compile(state).unwrap();
                assert!(!kernel.converse(1, 2).unwrap());
                kernel.set_social_pressure(2, true).unwrap();
                assert!(kernel.converse(1, 2).unwrap());
                let result = kernel.export();
                let event = result
                    .social
                    .as_ref()
                    .unwrap()
                    .events
                    .iter()
                    .find(|event| (event.a, event.b) == (1, 2))
                    .unwrap();
                assert_eq!(event.pressured, Some(2));
                assert_eq!(result.opinions.as_ref().unwrap().index().trust(2, 1), 0);
                assert_eq!(
                    result
                        .social
                        .as_ref()
                        .unwrap()
                        .needs
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .find(|p| p.person == 2)
                        .unwrap()
                        .isolation,
                    4
                );
                let restored = World::from_json(&serde_json::to_string(&result).unwrap()).unwrap();
                assert_eq!(Kernel::compile(restored).unwrap().export(), result);
                result
            })
    };
    for (fear, obligation) in [(2, 0), (0, 3)] {
        assert_eq!(run(1, fear, obligation), run(4, fear, obligation));
    }
}

#[test]
fn pressure_does_not_make_a_physically_busy_person_available() {
    let mut state = world();
    state.social.as_mut().unwrap().participants[1].willing = false;
    state.social.as_mut().unwrap().pressures = vec![game_parallel::SocialPressurePolicy {
        person: 2,
        fear_at_least: Some(1),
        obligation_at_least: None,
        enabled: true,
    }];
    let mut opinions = World::from_toml(include_str!("../../../../content/parallel/campaign.toml"))
        .unwrap()
        .opinions
        .unwrap();
    opinions.records = vec![game_parallel::Opinion {
        observer: 2,
        target: 1,
        trust: 0,
        fear: 5,
        obligation: 0,
        affinity: 0,
    }];
    state.opinions = Some(opinions);
    let mut kernel = Kernel::compile(state).unwrap();
    kernel
        .start_work(&[StartWork {
            id: 1,
            worker: 2,
            region: 1,
            recipe: 1,
            units: 1,
        }])
        .unwrap();
    assert!(!kernel.converse(1, 2).unwrap());
    assert!(
        kernel
            .export()
            .social
            .unwrap()
            .events
            .iter()
            .all(|event| event.a != 1 && event.b != 1)
    );
}

#[test]
fn autonomous_contacts_can_be_answered_from_fear_without_privileged_target_selection() {
    let run = |workers, directed_toward_actor| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = World::from_toml(include_str!(
                    "../../../../content/parallel/social_needs.toml"
                ))
                .unwrap();
                state.social.as_mut().unwrap().participants[1].willing = false;
                state.social.as_mut().unwrap().pressures =
                    vec![game_parallel::SocialPressurePolicy {
                        person: 2,
                        fear_at_least: Some(2),
                        obligation_at_least: None,
                        enabled: true,
                    }];
                let mut opinions =
                    World::from_toml(include_str!("../../../../content/parallel/campaign.toml"))
                        .unwrap()
                        .opinions
                        .unwrap();
                opinions.rules.fear_decay_per_tick = 0;
                opinions.records = vec![game_parallel::Opinion {
                    observer: if directed_toward_actor { 2 } else { 1 },
                    target: if directed_toward_actor { 1 } else { 2 },
                    trust: 0,
                    fear: 2,
                    obligation: 0,
                    affinity: 0,
                }];
                state.opinions = Some(opinions);
                let mut kernel = Kernel::compile(state).unwrap();
                kernel.step().unwrap();
                let saved = kernel.export();
                assert_eq!(
                    saved
                        .social
                        .as_ref()
                        .unwrap()
                        .events
                        .iter()
                        .any(|event| event.pressured == Some(2)),
                    directed_toward_actor
                );
                let mut restored = Kernel::compile(
                    World::from_json(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                kernel.step().unwrap();
                restored.step().unwrap();
                assert_eq!(kernel.export(), restored.export());
                saved
            })
    };
    for direction in [false, true] {
        assert_eq!(run(1, direction), run(4, direction));
    }
}

#[test]
fn many_local_pressure_responses_remain_disjoint_and_worker_deterministic() {
    use game_parallel::{Opinion, Person, SocialParticipant, SocialPressurePolicy};
    let count = 8192;
    let mut state = World::from_toml(include_str!(
        "../../../../content/parallel/social_needs.toml"
    ))
    .unwrap();
    state.regions[0].people = (1..=count)
        .map(|id| Person {
            id,
            hunger: 0,
            health: 100,
        })
        .collect();
    state.regions[1].people.clear();
    let social = state.social.as_mut().unwrap();
    social.needs = None;
    social.participants = (1..=count)
        .map(|person| SocialParticipant {
            person,
            willing: !person.is_multiple_of(2),
        })
        .collect();
    social.pressures = (1..=count)
        .filter(|id| id.is_multiple_of(2))
        .map(|person| SocialPressurePolicy {
            person,
            fear_at_least: Some(2),
            obligation_at_least: None,
            enabled: true,
        })
        .collect();
    let mut opinions = World::from_toml(include_str!("../../../../content/parallel/campaign.toml"))
        .unwrap()
        .opinions
        .unwrap();
    opinions.rules.fear_decay_per_tick = 0;
    opinions.rules.encounter_gain = 0;
    opinions.records = (1..=count)
        .filter(|id| id.is_multiple_of(2))
        .flat_map(|id| {
            let first = ((id - 1) / 4096) * 4096 + 1;
            let len = (count - first + 1).min(4096);
            (-15i64..=15)
                .filter(|offset| offset & 1 != 0)
                .map(move |offset| {
                    first + ((id - first) as i64 + offset).rem_euclid(len as i64) as u64
                })
                .map(move |target| Opinion {
                    observer: id,
                    target,
                    trust: 0,
                    fear: 2,
                    obligation: 0,
                    affinity: 0,
                })
        })
        .collect();
    opinions
        .records
        .sort_unstable_by_key(|row| (row.observer, row.target));
    state.opinions = Some(opinions);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                let events = &result.social.as_ref().unwrap().events;
                assert!(
                    events
                        .iter()
                        .filter(|event| event.pressured.is_some())
                        .count()
                        > count as usize / 6
                );
                let mut actors: Vec<_> =
                    events.iter().flat_map(|event| [event.a, event.b]).collect();
                actors.sort_unstable();
                assert!(actors.windows(2).all(|pair| pair[0] != pair[1]));
                result.validate().unwrap();
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn pressured_contacts_can_involve_people_beyond_immediate_id_neighbors() {
    let mut state = World::from_toml(include_str!(
        "../../../../content/parallel/social_needs.toml"
    ))
    .unwrap();
    state.rules.hunger_per_tick = 0;
    state.regions[0].people.push(game_parallel::Person {
        id: 4,
        hunger: 0,
        health: 100,
    });
    state
        .social
        .as_mut()
        .unwrap()
        .participants
        .push(game_parallel::SocialParticipant {
            person: 4,
            willing: true,
        });
    state.social.as_mut().unwrap().participants[1].willing = false;
    state.social.as_mut().unwrap().needs = None;
    state.social.as_mut().unwrap().pressures = vec![game_parallel::SocialPressurePolicy {
        person: 2,
        fear_at_least: Some(2),
        obligation_at_least: None,
        enabled: true,
    }];
    let mut opinions = World::from_toml(include_str!("../../../../content/parallel/campaign.toml"))
        .unwrap()
        .opinions
        .unwrap();
    opinions.rules.fear_decay_per_tick = 0;
    opinions.rules.encounter_gain = 0;
    opinions.records = vec![game_parallel::Opinion {
        observer: 2,
        target: 4,
        trust: 0,
        fear: 2,
        obligation: 0,
        affinity: 0,
    }];
    state.opinions = Some(opinions);
    let mut kernel = Kernel::compile(state).unwrap();
    let mut seen = false;
    for _ in 0..64 {
        kernel.step().unwrap();
        seen |= kernel
            .export()
            .social
            .unwrap()
            .events
            .iter()
            .any(|event| (event.a, event.b, event.pressured) == (2, 4, Some(2)));
    }
    assert!(seen);
}

#[test]
fn autonomous_pressure_choices_survive_travel_block_splits_and_reload() {
    use game_parallel::{Move, Opinion, Person, SocialParticipant, SocialPressurePolicy};
    let count = 256u64;
    let mut state = World::from_toml(include_str!(
        "../../../../content/parallel/social_needs.toml"
    ))
    .unwrap();
    state.rules.hunger_per_tick = 0;
    state.regions[0].people = (1..=count)
        .map(|id| Person {
            id,
            hunger: 0,
            health: 100,
        })
        .collect();
    state.regions[1].people.clear();
    let social = state.social.as_mut().unwrap();
    social.needs = None;
    social.participants = (1..=count)
        .map(|person| SocialParticipant {
            person,
            willing: !person.is_multiple_of(2),
        })
        .collect();
    social.pressures = (1..=count)
        .filter(|id| id.is_multiple_of(2))
        .map(|person| SocialPressurePolicy {
            person,
            fear_at_least: Some(2),
            obligation_at_least: None,
            enabled: true,
        })
        .collect();
    let mut opinions = World::from_toml(include_str!("../../../../content/parallel/campaign.toml"))
        .unwrap()
        .opinions
        .unwrap();
    opinions.rules.fear_decay_per_tick = 0;
    opinions.rules.encounter_gain = 0;
    opinions.records = (1..=count)
        .filter(|id| id.is_multiple_of(2))
        .flat_map(|observer| {
            (1..=count)
                .filter(|id| !id.is_multiple_of(2))
                .map(move |target| Opinion {
                    observer,
                    target,
                    trust: 0,
                    fear: 2,
                    obligation: 0,
                    affinity: 0,
                })
        })
        .collect();
    state.opinions = Some(opinions);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                let moves: Vec<_> = (1..=count)
                    .filter(|id| id.is_multiple_of(3))
                    .map(|person| Move {
                        person,
                        destination: 2,
                    })
                    .collect();
                kernel.move_people(&moves).unwrap();
                kernel.step().unwrap();
                kernel.step().unwrap();
                let saved = kernel.export();
                let mut restored = Kernel::compile(
                    World::from_json(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                for _ in 0..4 {
                    kernel.step().unwrap();
                    restored.step().unwrap();
                    assert_eq!(kernel.export(), restored.export());
                }
                kernel.export()
            })
    };
    assert_eq!(run(1), run(4));
}
#[test]
fn player_conversation_consumes_a_tick_reserves_both_people_and_shares_reports() {
    let mut kernel = Kernel::compile(world()).unwrap();
    let stale = kernel.prepare().unwrap();
    assert!(kernel.converse(1, 2).unwrap());
    assert!(kernel.commit(stale).is_err());
    let first = kernel.export();
    assert_eq!(first.tick, 1);
    assert!(first.production.as_ref().unwrap().jobs.is_empty());
    assert!(
        first
            .social
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|event| (event.a, event.b) == (1, 2))
    );
    assert_eq!(
        first
            .social
            .as_ref()
            .unwrap()
            .relationships
            .iter()
            .find(|r| (r.a, r.b) == (1, 2))
            .unwrap()
            .encounters,
        1
    );
    assert_eq!(
        first
            .knowledge
            .as_ref()
            .unwrap()
            .minds
            .iter()
            .find(|mind| mind.person == 2)
            .unwrap()
            .records
            .iter()
            .find(|record| record.region == 2)
            .unwrap()
            .learned_from,
        Some(1)
    );
    kernel = Kernel::compile(World::from_json(&serde_json::to_string(&first).unwrap()).unwrap())
        .unwrap();
    assert!(kernel.converse(1, 2).unwrap());
    assert_eq!(
        kernel
            .export()
            .social
            .unwrap()
            .relationships
            .iter()
            .find(|r| (r.a, r.b) == (1, 2))
            .unwrap()
            .encounters,
        2
    );
}
#[test]
fn target_can_decline_while_invalid_or_occupied_speakers_do_not_advance_time() {
    let mut state = world();
    state.social.as_mut().unwrap().participants[1].willing = false;
    let mut kernel = Kernel::compile(state).unwrap();
    assert!(!kernel.converse(1, 2).unwrap());
    assert_eq!(kernel.tick(), 1);
    assert!(
        !kernel
            .export()
            .social
            .unwrap()
            .relationships
            .iter()
            .any(|r| (r.a, r.b) == (1, 2))
    );
    let before = kernel.export();
    assert!(kernel.converse(1, 1).is_err());
    assert!(kernel.converse(1, 999).is_err());
    assert_eq!(kernel.export(), before);
    let mut kernel = Kernel::compile(world()).unwrap();
    kernel
        .start_work(&[StartWork {
            id: 1,
            worker: 1,
            region: 1,
            recipe: 1,
            units: 1,
        }])
        .unwrap();
    let before = kernel.export();
    assert!(kernel.converse(1, 2).is_err());
    assert_eq!(kernel.export(), before);
}
#[test]
fn failed_lifecycle_phase_discards_the_entire_conversation() {
    let mut state = world();
    state.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut kernel = Kernel::compile(state).unwrap();
    let before = kernel.export();
    assert!(kernel.converse(1, 2).is_err());
    assert_eq!(kernel.export(), before);
}

#[test]
fn conversation_respects_personality_and_an_existing_target_job() {
    use game_parallel::{Personality, PersonalityPerson};
    let mut state = world();
    state.personality = Some(Personality {
        social_urgency_per_isolation: 0,
        inherit_births: false,
        people: vec![PersonalityPerson {
            person: 2,
            sociability: 0,
            diligence: 1000,
        }],
    });
    let mut kernel = Kernel::compile(state).unwrap();
    assert!(!kernel.converse(1, 2).unwrap());
    assert_eq!(kernel.tick(), 1);
    let mut kernel = Kernel::compile(world()).unwrap();
    kernel
        .start_work(&[StartWork {
            id: 1,
            worker: 2,
            region: 1,
            recipe: 1,
            units: 1,
        }])
        .unwrap();
    assert!(!kernel.converse(1, 2).unwrap());
    assert_eq!(kernel.tick(), 1);
    assert!(
        kernel
            .export()
            .social
            .unwrap()
            .events
            .iter()
            .all(|event| event.a != 1 && event.b != 1)
    );
}
#[test]
fn conversation_results_are_worker_invariant_after_reload() {
    let state = world();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                assert!(kernel.converse(1, 2).unwrap());
                kernel = Kernel::compile(
                    World::from_json(&serde_json::to_string(&kernel.export()).unwrap()).unwrap(),
                )
                .unwrap();
                assert!(kernel.converse(1, 2).unwrap());
                for _ in 0..3 {
                    kernel.step().unwrap();
                }
                kernel.export()
            })
    };
    let result = run(1);
    assert_eq!(result, run(4));
    result.validate().unwrap();
}
#[test]
fn an_infectious_person_can_transmit_disease_during_a_real_conversation() {
    let mut state = world();
    state.disease = World::from_toml(include_str!("../../../../content/parallel/disease.toml"))
        .unwrap()
        .disease;
    let mut kernel = Kernel::compile(state).unwrap();
    assert!(kernel.converse(1, 2).unwrap());
    assert!(kernel.converse(1, 2).unwrap());
    let result = kernel.export();
    assert!(
        result
            .disease
            .unwrap()
            .cases
            .iter()
            .any(|case| case.person == 2 && case.strain == 1)
    );
}

#[test]
fn player_conversation_earns_mutual_trust_for_later_decisions() {
    let mut state = world();
    state.opinions = World::from_toml(include_str!(
        "../../../../content/parallel/social_trust.toml"
    ))
    .unwrap()
    .opinions;
    let mut kernel = Kernel::compile(state).unwrap();
    assert!(kernel.converse(1, 2).unwrap());
    let result = kernel.export();
    let opinions = result.opinions.unwrap();
    assert_eq!(opinions.index().trust(2, 1), -1);
    assert_eq!(opinions.index().trust(1, 2), 1);
}
