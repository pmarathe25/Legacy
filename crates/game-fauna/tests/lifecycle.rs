use game_fauna::{
    Animal, AnimalOpinion, Climate, Exposure, Fauna, Feeding, Patch, Pregnancy, SocialResponse,
    Species,
};
use game_genetics::{Genome, Sex};
use std::collections::HashSet;

fn genome(sex: Sex, allele: u16) -> Genome {
    Genome {
        sex,
        alleles: [[allele; 2]; 4],
    }
}
fn species() -> Species {
    Species {
        id: 1,
        name: "grazer".into(),
        food_good: 1,
        intake: 2,
        hunger_per_tick: 1,
        relief_per_unit: 1,
        starvation_threshold: 10,
        starvation_damage: 3,
        max_health: 10,
        adult_age: 1,
        fertile_until: 10,
        lifespan: 30,
        gestation: 1,
        conception_one_in: 1,
        breeding_max_hunger: 5,
        newborn_health: 8,
        mutation_per_million: 0,
        mutation_step: 0,
        climate: None,
        hunting: None,
        newborn_social_response: None,
        newborn_handling_response: None,
        newborn_restraint_response: None,
    }
}
fn animal(id: u64, sex: Sex, allele: u16) -> Animal {
    Animal {
        id,
        species: 1,
        region: 1,
        age: 1,
        health: 10,
        hunger: 1,
        genome: genome(sex, allele),
        pregnancy: None,
        parents: None,
        social_response: None,
        handling_response: None,
        restraint_response: None,
        opinions: vec![],
    }
}
fn initial() -> Fauna {
    Fauna {
        species: vec![species()],
        animals: vec![animal(1, Sex::Female, 100), animal(2, Sex::Male, 900)],
        next_id: 3,
        last_tick: 0,
    }
}

#[test]
fn quiet_generations_match_complete_lifecycle_and_preserve_snapshots() {
    let mut state = initial();
    state.animals[0].pregnancy = Some(Pregnancy {
        father: 2,
        father_genome: state.animals[1].genome.clone(),
        due: 5,
    });
    state.animals[0].hunger = 9;
    state.animals[0].health = 3;
    state.animals[1].opinions.push(AnimalOpinion {
        person: 7,
        affinity: -3,
    });
    state.species[0].climate = Some(Climate {
        tolerance: 1,
        inherited_tolerance: 2,
        hardiness_locus: 0,
        damage_per_excess: 1,
    });
    let exposure = [Exposure {
        region: 1,
        severity: 3,
    }];
    let expected = state
        .advance_with_feeding_and_exposure(&[], &[], &exposure, 7, 1)
        .unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let snapshot = state.clone();
                let mut next = state.clone();
                let draft = next.prepare_quiet(&exposure, 7, 1).unwrap().unwrap();
                assert_eq!(draft.deaths(), expected.deaths);
                next.validate_quiet(&draft).unwrap();
                next.publish_quiet_validated(draft);
                assert_eq!(state, snapshot);
                assert_eq!(next, expected.fauna);
                assert_eq!(
                    serde_json::from_str::<Fauna>(&serde_json::to_string(&next).unwrap()).unwrap(),
                    next
                );
                next
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn quiet_generation_rejects_stale_state_and_falls_back_for_unsorted_animals() {
    let mut state = initial();
    state.animals[0].pregnancy = Some(Pregnancy {
        father: 2,
        father_genome: state.animals[1].genome.clone(),
        due: 1,
    });
    let draft = state.prepare_quiet(&[], 7, 1).unwrap().unwrap();
    let mut stale = state.clone();
    stale.animals[0].id = 9;
    assert!(stale.validate_quiet(&draft).is_err());
    assert_eq!(state.last_tick, 0);
    state.animals[0].age = u64::MAX;
    assert!(state.prepare_quiet(&[], 7, 1).is_err());
    state.animals.swap(0, 1);
    assert!(state.prepare_quiet(&[], 7, 1).unwrap().is_none());
}

#[test]
fn staged_births_and_conception_match_complete_lifecycle_across_workers() {
    let mut state = initial();
    let mut isolated = animal(3, Sex::Female, 500);
    isolated.region = 2;
    state.animals.push(isolated);
    state.animals.push(animal(4, Sex::Male, 300));
    state.next_id = 5;
    state.species[0].mutation_per_million = 500_000;
    state.species[0].mutation_step = 10;
    state.species[0].newborn_social_response = Some(SocialResponse {
        feed_affinity: -1,
        minimum: -5,
        maximum: 5,
        capacity: 3,
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut compact = state.clone();
                let mut complete = state.clone();
                for tick in 1..=8 {
                    let expected = complete
                        .advance_with_hunts(&[], &[], &[], &[], 7, tick)
                        .unwrap();
                    let draft = compact.prepare_quiet(&[], 7, tick).unwrap().unwrap();
                    assert_eq!(draft.deaths(), expected.deaths);
                    compact.validate_quiet(&draft).unwrap();
                    compact.publish_quiet_validated(draft);
                    complete = expected.fauna;
                    assert_eq!(compact, complete);
                }
                assert!(compact.animals.len() > state.animals.len());
                assert!(
                    compact
                        .animals
                        .iter()
                        .find(|a| a.id == 3)
                        .unwrap()
                        .pregnancy
                        .is_none()
                );
                compact
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn staged_reproduction_rejects_overflow_without_mutating_the_population() {
    let mut birth = initial();
    birth.animals[0].pregnancy = Some(Pregnancy {
        father: 2,
        father_genome: birth.animals[1].genome.clone(),
        due: 1,
    });
    birth.next_id = u64::MAX;
    let before = birth.clone();
    assert!(birth.prepare_quiet(&[], 7, 1).is_err());
    assert!(birth.advance_with_hunts(&[], &[], &[], &[], 7, 1).is_err());
    assert_eq!(birth, before);
    let mut conception = initial();
    conception.species[0].gestation = u64::MAX;
    let before = conception.clone();
    assert!(conception.prepare_quiet(&[], 7, 1).is_err());
    assert!(
        conception
            .advance_with_hunts(&[], &[], &[], &[], 7, 1)
            .is_err()
    );
    assert_eq!(conception, before);
}

#[test]
fn staged_feeding_and_hunts_match_complete_lifecycle_with_subjective_reactions() {
    let mut state = initial();
    state.animals.push(animal(3, Sex::Male, 400));
    state.next_id = 4;
    state.animals[0].social_response = Some(SocialResponse {
        feed_affinity: -2,
        minimum: -5,
        maximum: 5,
        capacity: 2,
    });
    state.animals[0].opinions.push(AnimalOpinion {
        person: 8,
        affinity: 3,
    });
    let feeding = [
        Feeding {
            animal: 1,
            person: Some(9),
            good: 2,
            quantity: 2,
        },
        Feeding {
            animal: 1,
            person: Some(8),
            good: 1,
            quantity: 1,
        },
        Feeding {
            animal: 1,
            person: Some(8),
            good: 1,
            quantity: 2,
        },
        Feeding {
            animal: 3,
            person: None,
            good: 1,
            quantity: u64::MAX,
        },
    ];
    let hunted = [2];
    let expected = state
        .advance_with_hunts(&[], &feeding, &[], &hunted, 7, 1)
        .unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut next = state.clone();
                let draft = next
                    .prepare_without_grazing(&feeding, &[], &hunted, 7, 1)
                    .unwrap()
                    .unwrap();
                assert_eq!(draft.deaths(), expected.deaths);
                next.validate_quiet(&draft).unwrap();
                next.publish_quiet_validated(draft);
                assert_eq!(next, expected.fauna);
                assert_eq!(next.animals[0].opinions[0].affinity, -1);
                assert_eq!(next.animals[0].opinions[1].affinity, -2);
                assert_eq!(next.animals[1].health, 0);
                next
            })
    };
    assert_eq!(run(1), run(4));
    for (food, prey) in [
        (vec![feeding[0]], vec![1]),
        (
            vec![Feeding {
                quantity: 0,
                ..feeding[0]
            }],
            vec![],
        ),
        (
            vec![Feeding {
                animal: 99,
                ..feeding[0]
            }],
            vec![],
        ),
        (vec![], vec![2, 2]),
        (vec![], vec![99]),
    ] {
        assert!(
            state
                .prepare_without_grazing(&food, &[], &prey, 7, 1)
                .is_err()
        );
        assert!(
            state
                .advance_with_hunts(&[], &food, &[], &prey, 7, 1)
                .is_err()
        );
    }
}

#[test]
fn staged_feeding_handles_sparse_animal_ids_and_unsorted_receipts() {
    let mut state = initial();
    state.animals[1].id = 50;
    state.next_id = 51;
    let food = [
        Feeding {
            animal: 50,
            person: Some(8),
            good: 1,
            quantity: 1,
        },
        Feeding {
            animal: 1,
            person: Some(3),
            good: 1,
            quantity: 1,
        },
        Feeding {
            animal: 50,
            person: Some(2),
            good: 1,
            quantity: 2,
        },
    ];
    let expected = state
        .advance_with_hunts(&[], &food, &[], &[], 7, 1)
        .unwrap()
        .fauna;
    let draft = state
        .prepare_without_grazing(&food, &[], &[], 7, 1)
        .unwrap()
        .unwrap();
    state.validate_quiet(&draft).unwrap();
    state.publish_quiet_validated(draft);
    assert_eq!(state, expected);
}

#[test]
fn staged_grazing_matches_complete_lifecycle_and_conserves_each_patch() {
    let mut state = initial();
    for id in 3..=2048 {
        state
            .animals
            .push(animal(id, Sex::Male, (id % 1000) as u16));
    }
    state.next_id = 2049;
    state.animals[0].social_response = Some(SocialResponse {
        feed_affinity: 2,
        minimum: -5,
        maximum: 5,
        capacity: 3,
    });
    let patches = [
        Patch {
            id: 8,
            region: 1,
            good: 1,
            available: 500,
        },
        Patch {
            id: 2,
            region: 1,
            good: 1,
            available: 0,
        },
        Patch {
            id: 3,
            region: 1,
            good: 1,
            available: 777,
        },
        Patch {
            id: 4,
            region: 1,
            good: 2,
            available: 200,
        },
    ];
    let food = [Feeding {
        animal: 1,
        person: Some(10),
        good: 1,
        quantity: 1,
    }];
    let hunted = [2];
    for seed in [7, 123] {
        let expected = state
            .advance_with_hunts(&patches, &food, &[], &hunted, seed, 1)
            .unwrap();
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut next = state.clone();
                    let draft = next
                        .prepare_compact(&patches, &food, &[], &hunted, seed, 1)
                        .unwrap()
                        .unwrap();
                    assert_eq!(draft.consumed(), expected.consumed);
                    assert_eq!(draft.deaths(), expected.deaths);
                    next.validate_quiet(&draft).unwrap();
                    next.publish_quiet_validated(draft);
                    assert_eq!(next, expected.fauna);
                    next
                })
        };
        assert_eq!(run(1), run(4));
    }
    let invalid = [patches[0], patches[0]];
    assert!(
        state
            .prepare_compact(&invalid, &food, &[], &hunted, 7, 1)
            .is_err()
    );
    assert!(
        state
            .advance_with_hunts(&invalid, &food, &[], &hunted, 7, 1)
            .is_err()
    );
}

#[test]
fn large_birth_growth_moves_animals_without_losing_individual_state() {
    let mut state = initial();
    state.animals = (1..=16_384)
        .map(|id| animal(id, Sex::Male, (id % 1000) as u16))
        .collect();
    state.animals[0].genome.sex = Sex::Female;
    state.animals[0].pregnancy = Some(Pregnancy {
        father: 2,
        father_genome: state.animals[1].genome.clone(),
        due: 1,
    });
    state.animals[10].opinions.push(AnimalOpinion {
        person: 8,
        affinity: -3,
    });
    state.next_id = 16_385;
    let expected = state
        .advance_with_hunts(&[], &[], &[], &[], 7, 1)
        .unwrap()
        .fauna;
    for workers in [1, 4] {
        let next = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut next = state.clone();
                let draft = next.prepare_quiet(&[], 7, 1).unwrap().unwrap();
                next.validate_quiet(&draft).unwrap();
                next.publish_quiet_validated(draft);
                next
            });
        assert_eq!(next, expected);
    }
}

#[test]
#[ignore = "manual parallel animal reproduction benchmark"]
fn staged_reproduction_benchmark() {
    let count = std::env::var("LEGACY_ANIMALS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(100_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let births = std::env::var_os("LEGACY_BIRTHS").is_some();
    let father = genome(Sex::Male, 900);
    let mut state = initial();
    state.animals = (1..=count)
        .map(|id| {
            let mut a = animal(id, if id % 2 == 0 { Sex::Male } else { Sex::Female }, 500);
            if births && a.genome.sex == Sex::Female {
                a.pregnancy = Some(Pregnancy {
                    father: 2,
                    father_genome: father.clone(),
                    due: 1,
                });
            }
            a
        })
        .collect();
    state.next_id = count + 1;
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            let mut stages = Vec::new();
            for round in 0..4 {
                let mut next = state.clone();
                let now = std::time::Instant::now();
                let draft = next.prepare_quiet(&[], 7, 1).unwrap().unwrap();
                next.validate_quiet(&draft).unwrap();
                let prepared = now.elapsed().as_secs_f64();
                next.publish_quiet_validated(draft);
                let elapsed = now.elapsed().as_secs_f64();
                assert_eq!(
                    next.animals.len(),
                    count as usize
                        + if births {
                            count.div_ceil(2) as usize
                        } else {
                            0
                        }
                );
                if round > 0 {
                    samples.push(elapsed);
                    stages.push((prepared, elapsed - prepared));
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} animals={count} births={births} elapsed={:.6}",
                samples[1]
            );
            stages.sort_by(|a, b| a.0.total_cmp(&b.0));
            println!("prepare={:.6} publish={:.6}", stages[1].0, stages[1].1);
        });
}

#[test]
#[ignore = "manual parallel animal feeding benchmark"]
fn staged_feeding_benchmark() {
    let count = std::env::var("LEGACY_ANIMALS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(100_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let mut state = initial();
    state.animals = (1..=count)
        .map(|id| {
            let mut a = animal(id, Sex::Male, 500);
            a.social_response = Some(SocialResponse {
                feed_affinity: 1,
                minimum: -5,
                maximum: 5,
                capacity: 1,
            });
            a
        })
        .collect();
    state.next_id = count + 1;
    let food: Vec<_> = (1..=count)
        .map(|id| Feeding {
            animal: id,
            person: Some(id),
            good: 1,
            quantity: 1,
        })
        .collect();
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut next = state.clone();
                let now = std::time::Instant::now();
                let draft = next
                    .prepare_without_grazing(&food, &[], &[], 7, 1)
                    .unwrap()
                    .unwrap();
                next.validate_quiet(&draft).unwrap();
                next.publish_quiet_validated(draft);
                let elapsed = now.elapsed().as_secs_f64();
                assert!(
                    next.animals
                        .iter()
                        .all(|a| a.hunger == 1 && a.opinions[0].affinity == 1)
                );
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
fn individual_animals_interpret_handling_and_choose_whether_to_accept() {
    use game_fauna::handling::{Event, Outcome, Response, apply, respond};
    let mut state = initial();
    for animal in &mut state.animals {
        animal.social_response = Some(SocialResponse {
            feed_affinity: 0,
            minimum: -5,
            maximum: 5,
            capacity: 2,
        });
    }
    state.animals[0].handling_response = Some(Response {
        willing: false,
        affinity_at_least: Some(2),
        affinity_at_most: None,
        on_accept: -3,
        on_refuse: 2,
    });
    state.animals[1].handling_response = Some(Response {
        willing: true,
        affinity_at_least: None,
        affinity_at_most: None,
        on_accept: 3,
        on_refuse: 0,
    });
    assert_eq!(respond(&state.animals[0], 10), Outcome::Refused);
    assert_eq!(respond(&state.animals[1], 10), Outcome::Accepted);
    let events = [
        Event {
            tick: 1,
            person: 10,
            animal: 1,
            outcome: Outcome::Refused,
        },
        Event {
            tick: 1,
            person: 10,
            animal: 2,
            outcome: Outcome::Accepted,
        },
    ];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut next = state.clone();
                apply(&mut next, &events);
                next
            })
    };
    let next = run(1);
    assert_eq!(next, run(4));
    assert_eq!(next.animals[0].opinions[0].affinity, 2);
    assert_eq!(next.animals[1].opinions[0].affinity, 3);
    assert_eq!(respond(&next.animals[0], 10), Outcome::Accepted);
    assert_eq!(respond(&next.animals[0], 11), Outcome::Refused);
    let mut again = next.clone();
    apply(
        &mut again,
        &[
            Event {
                tick: 2,
                person: 10,
                animal: 1,
                outcome: Outcome::Accepted,
            },
            Event {
                tick: 2,
                person: 11,
                animal: 2,
                outcome: Outcome::Unavailable,
            },
        ],
    );
    assert_eq!(again.animals[0].opinions[0].affinity, -1);
    assert_eq!(respond(&again.animals[0], 10), Outcome::Refused);
    assert_eq!(again.animals[1].opinions, next.animals[1].opinions);
    let saved: Fauna = serde_json::from_str(&serde_json::to_string(&next).unwrap()).unwrap();
    assert_eq!(saved, next);
}

#[test]
fn newborn_animals_receive_configured_temperament_but_no_relationships() {
    use game_fauna::handling::Response;
    let mut state = initial();
    state.species[0].newborn_social_response = Some(SocialResponse {
        feed_affinity: 1,
        minimum: -5,
        maximum: 5,
        capacity: 2,
    });
    state.species[0].newborn_handling_response = Some(Response {
        willing: false,
        affinity_at_least: Some(2),
        affinity_at_most: None,
        on_accept: -1,
        on_refuse: 1,
    });
    state.animals[0].social_response = state.species[0].newborn_social_response.clone();
    state.animals[0].opinions.push(AnimalOpinion {
        person: 7,
        affinity: 4,
    });
    state.animals[0].pregnancy = Some(Pregnancy {
        father: 2,
        father_genome: state.animals[1].genome.clone(),
        due: 1,
    });
    let next = state.advance_with_feeding(&[], &[], 7, 1).unwrap().fauna;
    let newborn = next.animals.iter().find(|animal| animal.id == 3).unwrap();
    assert_eq!(
        newborn.social_response,
        state.species[0].newborn_social_response
    );
    assert_eq!(
        newborn.handling_response,
        state.species[0].newborn_handling_response
    );
    assert!(newborn.opinions.is_empty());
}

#[test]
fn inherited_hardiness_changes_individual_response_to_local_exposure() {
    let mut state = initial();
    state.species[0].climate = Some(Climate {
        tolerance: 1,
        inherited_tolerance: 4,
        hardiness_locus: 1,
        damage_per_excess: 1,
    });
    let weather = [Exposure {
        region: 1,
        severity: 6,
    }];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let draft = state
                    .advance_with_feeding_and_exposure(&[], &[], &weather, 7, 1)
                    .unwrap();
                assert_eq!(
                    draft
                        .fauna
                        .animals
                        .iter()
                        .map(|a| a.health)
                        .collect::<Vec<_>>(),
                    vec![5, 8]
                );
                let saved: Fauna =
                    serde_json::from_str(&serde_json::to_string(&draft.fauna).unwrap()).unwrap();
                assert_eq!(saved, draft.fauna);
                draft.fauna
            })
    };
    assert_eq!(run(1), run(4));
    let unexposed = state
        .advance_with_feeding_and_exposure(
            &[],
            &[],
            &[Exposure {
                region: 2,
                severity: 6,
            }],
            7,
            1,
        )
        .unwrap();
    assert!(
        unexposed
            .fauna
            .animals
            .iter()
            .all(|animal| animal.health == 10)
    );
    assert!(
        state
            .advance_with_feeding_and_exposure(&[], &[], &[weather[0], weather[0]], 7, 1)
            .is_err()
    );
    state.species[0].climate.as_mut().unwrap().hardiness_locus = 4;
    assert!(
        state
            .validate(&HashSet::from([1]), &HashSet::from([1]), 0)
            .is_err()
    );
    assert!(
        state
            .advance_with_feeding_and_exposure(&[], &[], &weather, 7, 1)
            .is_err()
    );
}

#[test]
fn animal_affinity_changes_are_individual_signed_and_bounded() {
    let mut state = initial();
    state.animals[0].social_response = Some(SocialResponse {
        feed_affinity: -3,
        minimum: -5,
        maximum: 5,
        capacity: 2,
    });
    state.animals[1].social_response = Some(SocialResponse {
        feed_affinity: 2,
        minimum: -5,
        maximum: 5,
        capacity: 2,
    });
    state.animals[0].opinions.push(AnimalOpinion {
        person: 3,
        affinity: 1,
    });
    let feeds = [
        Feeding {
            animal: 1,
            person: Some(3),
            good: 99,
            quantity: 1,
        },
        Feeding {
            animal: 1,
            person: Some(3),
            good: 1,
            quantity: 1,
        },
        Feeding {
            animal: 1,
            person: Some(2),
            good: 1,
            quantity: 1,
        },
        Feeding {
            animal: 1,
            person: Some(1),
            good: 1,
            quantity: 1,
        },
        Feeding {
            animal: 2,
            person: Some(3),
            good: 99,
            quantity: 1,
        },
    ];
    let run = |workers, reverse| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut input = feeds.to_vec();
                if reverse {
                    input.reverse();
                }
                let draft = state.advance_with_feeding(&[], &input, 7, 1).unwrap();
                assert_eq!(
                    draft.fauna.animals[0].opinions,
                    vec![
                        AnimalOpinion {
                            person: 1,
                            affinity: -3
                        },
                        AnimalOpinion {
                            person: 3,
                            affinity: -5
                        }
                    ]
                );
                assert_eq!(
                    draft.fauna.animals[1].opinions,
                    vec![AnimalOpinion {
                        person: 3,
                        affinity: 2
                    }]
                );
                let saved: Fauna =
                    serde_json::from_str(&serde_json::to_string(&draft.fauna).unwrap()).unwrap();
                assert_eq!(saved, draft.fauna);
                saved
            })
    };
    assert_eq!(run(1, false), run(4, true));
}

#[test]
fn grazers_consume_actual_food_and_inherit_from_local_parents() {
    let initial = initial();
    initial
        .validate(&HashSet::from([1]), &HashSet::from([1]), 0)
        .unwrap();
    let patch = Patch {
        id: 1,
        region: 1,
        good: 1,
        available: 4,
    };
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let first = initial.advance(&[patch], 7, 1).unwrap();
                assert_eq!(first.consumed, vec![(1, 4)]);
                assert!(first.births.is_empty());
                assert_eq!(first.fauna.animals[0].pregnancy.as_ref().unwrap().father, 2);
                let saved: Fauna =
                    serde_json::from_str(&serde_json::to_string(&first.fauna).unwrap()).unwrap();
                let second = first.fauna.advance(&[patch], 7, 2).unwrap();
                let restored = saved.advance(&[patch], 7, 2).unwrap();
                assert_eq!(second.fauna, restored.fauna);
                assert_eq!(second.births, vec![3]);
                let child = &second.fauna.animals[2];
                assert_eq!(child.parents, Some([1, 2]));
                assert!(child.genome.alleles.iter().all(|pair| *pair == [100, 900]));
                assert_eq!(child.health, 8);
                second
                    .fauna
                    .validate(&HashSet::from([1]), &HashSet::from([1]), 2)
                    .unwrap();
                second.fauna
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn physically_fed_animals_need_less_food_from_the_patch() {
    let state = initial();
    let patch = Patch {
        id: 1,
        region: 1,
        good: 1,
        available: 10,
    };
    let feed = Feeding {
        animal: 1,
        person: None,
        good: 1,
        quantity: 2,
    };
    assert_eq!(state.check_feeding(feed).unwrap(), 1);
    let draft = state.advance_with_feeding(&[patch], &[feed], 7, 1).unwrap();
    assert_eq!(draft.consumed, vec![(1, 3)]);
    assert!(draft.fauna.animals.iter().all(|animal| animal.hunger == 0));
    let unsuitable = Feeding { good: 99, ..feed };
    assert_eq!(state.check_feeding(unsuitable).unwrap(), 1);
    let wrong = state
        .advance_with_feeding(&[], &[unsuitable], 7, 1)
        .unwrap();
    assert_eq!(wrong.fauna.animals[0].hunger, 2);
    assert!(
        state
            .advance_with_feeding(
                &[patch],
                &[
                    Feeding {
                        quantity: u64::MAX,
                        ..feed
                    },
                    feed
                ],
                7,
                1
            )
            .is_ok()
    );
}

#[test]
fn depleted_patches_match_absent_patches_without_skipping_other_animal_behavior() {
    let mut state = initial();
    state.animals[0].social_response = Some(SocialResponse {
        feed_affinity: 2,
        minimum: -10,
        maximum: 10,
        capacity: 4,
    });
    let feed = Feeding {
        animal: 1,
        person: Some(3),
        good: 1,
        quantity: 1,
    };
    let empty = Patch {
        id: 1,
        region: 1,
        good: 1,
        available: 0,
    };
    let run = |workers, patches: &[Patch]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let draft = state.advance_with_feeding(patches, &[feed], 7, 1).unwrap();
                (draft.fauna, draft.consumed, draft.births, draft.deaths)
            })
    };
    let absent = run(1, &[]);
    assert_eq!(run(1, &[empty]), absent);
    assert_eq!(run(4, &[empty]), absent);
    assert_eq!(absent.0.animals[0].opinions[0].affinity, 2);
    assert!(absent.1.is_empty());
}

#[test]
fn a_nearly_sated_animal_does_not_consume_its_full_maximum_intake() {
    let mut state = initial();
    state.animals.truncate(1);
    state.animals[0].hunger = 0;
    let patch = Patch {
        id: 1,
        region: 1,
        good: 1,
        available: 10,
    };
    let draft = state.advance(&[patch], 7, 1).unwrap();
    assert_eq!(draft.consumed, vec![(1, 1)]);
    assert_eq!(draft.fauna.animals[0].hunger, 0);
}

#[test]
fn contested_grazing_conserves_stock_and_is_worker_invariant() {
    let count = 20_000;
    let mut state = initial();
    state.species[0].conception_one_in = u64::MAX;
    state.species[0].hunger_per_tick = 0;
    state.animals = (1..=count)
        .rev()
        .map(|id| {
            let mut a = animal(id, Sex::Male, 100);
            a.hunger = 3;
            a
        })
        .collect();
    state.next_id = count + 1;
    let patch = Patch {
        id: 7,
        region: 1,
        good: 1,
        available: 7_777,
    };
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let draft = state.advance(&[patch], 9, 1).unwrap();
                assert_eq!(draft.consumed, vec![(7, 7_777)]);
                assert_eq!(
                    draft
                        .fauna
                        .animals
                        .iter()
                        .map(|a| u64::from(3 - a.hunger))
                        .sum::<u64>(),
                    7_777
                );
                draft.fauna
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn concentrated_grazing_rotates_priority_and_handles_partial_grants() {
    let mut state = initial();
    state.animals = (1..=64)
        .map(|id| {
            let mut a = animal(id, Sex::Male, 500);
            a.hunger = (id % 3) as u32;
            a
        })
        .collect();
    state.next_id = 65;
    let patch = Patch {
        id: 1,
        region: 1,
        good: 1,
        available: 33,
    };
    let seed = 42;
    let offset = (game_genetics::random(seed ^ 0x6772617a655f726f, 1, 1) % 64) as usize;
    let mut expected: Vec<_> = state.animals.iter().map(|a| a.hunger + 1).collect();
    let mut available = patch.available;
    for step in 0..state.animals.len() {
        let index = (offset + step) % state.animals.len();
        let eaten = available.min(u64::from(expected[index].min(2)));
        expected[index] -= eaten as u32;
        available -= eaten;
    }
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let draft = state
                    .advance_with_hunts(&[patch], &[], &[], &[], seed, 1)
                    .unwrap();
                assert_eq!(draft.consumed, vec![(1, 33)]);
                assert_eq!(
                    draft
                        .fauna
                        .animals
                        .iter()
                        .map(|a| a.hunger)
                        .collect::<Vec<_>>(),
                    expected
                );
                draft.fauna
            })
    };
    let next = run(1);
    assert_eq!(next, run(4));
    let mut reversed = state.clone();
    reversed.animals.reverse();
    assert_eq!(
        next,
        reversed
            .advance_with_hunts(&[patch], &[], &[], &[], seed, 1)
            .unwrap()
            .fauna
    );
    let winners: HashSet<_> = (0..128)
        .map(|seed| {
            let draft = state
                .advance_with_hunts(
                    &[Patch {
                        available: 1,
                        ..patch
                    }],
                    &[],
                    &[],
                    &[],
                    seed,
                    1,
                )
                .unwrap();
            draft
                .fauna
                .animals
                .iter()
                .position(|a| a.hunger < state.animals[(a.id - 1) as usize].hunger + 1)
                .unwrap()
        })
        .collect();
    assert!(winners.len() > 40);
}

#[test]
fn independent_patches_rotate_and_allocate_their_own_claims() {
    let mut state = initial();
    state.animals = (1..=16)
        .map(|id| {
            let mut a = animal(id, Sex::Male, 500);
            a.region = if id % 2 == 0 { 2 } else { 1 };
            a.hunger = (id % 3) as u32;
            a
        })
        .collect();
    state.next_id = 17;
    let patches = [
        Patch {
            id: 5,
            region: 1,
            good: 1,
            available: 5,
        },
        Patch {
            id: 9,
            region: 2,
            good: 1,
            available: 7,
        },
    ];
    let seed = 22;
    let mut expected: Vec<_> = state.animals.iter().map(|a| a.hunger + 1).collect();
    for patch in patches {
        let indices: Vec<_> = state
            .animals
            .iter()
            .enumerate()
            .filter_map(|(i, a)| (a.region == patch.region).then_some(i))
            .collect();
        let first = (game_genetics::random(seed ^ 0x6772617a655f726f, patch.id, 1)
            % indices.len() as u64) as usize;
        let mut remaining = patch.available;
        for step in 0..indices.len() {
            let i = indices[(first + step) % indices.len()];
            let eaten = remaining.min(u64::from(expected[i].min(2)));
            expected[i] -= eaten as u32;
            remaining -= eaten;
        }
    }
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let draft = state
                    .advance_with_hunts(&patches, &[], &[], &[], seed, 1)
                    .unwrap();
                assert_eq!(draft.consumed, vec![(5, 5), (9, 7)]);
                assert_eq!(
                    draft
                        .fauna
                        .animals
                        .iter()
                        .map(|a| a.hunger)
                        .collect::<Vec<_>>(),
                    expected
                );
                draft.fauna
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore = "performance measurement"]
fn concentrated_grazing_benchmark() {
    let count: u64 = std::env::var("LEGACY_COUNT")
        .unwrap_or_else(|_| "100000".into())
        .parse()
        .unwrap();
    let workers: usize = std::env::var("LEGACY_WORKERS")
        .unwrap_or_else(|_| "1".into())
        .parse()
        .unwrap();
    let staged = std::env::var_os("LEGACY_STAGED").is_some();
    let mut state = initial();
    state.species[0].conception_one_in = u64::MAX;
    state.animals = (1..=count).map(|id| animal(id, Sex::Male, 100)).collect();
    state.next_id = count + 1;
    let patch = Patch {
        id: 1,
        region: 1,
        good: 1,
        available: count,
    };
    state
        .validate(&HashSet::from([1]), &HashSet::from([1]), 0)
        .unwrap();
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for _ in 0..4 {
                let mut next = staged.then(|| state.clone());
                let started = std::time::Instant::now();
                if let Some(next) = &mut next {
                    let draft = next
                        .prepare_compact(&[patch], &[], &[], &[], 7, 1)
                        .unwrap()
                        .unwrap();
                    assert_eq!(draft.consumed(), &[(1, count)]);
                    next.validate_quiet(&draft).unwrap();
                    next.publish_quiet_validated(draft);
                } else {
                    let draft = state.advance_validated(&[patch], 7, 1).unwrap();
                    assert_eq!(draft.consumed, vec![(1, count)]);
                }
                samples.push(started.elapsed().as_secs_f64());
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} animals={count} staged={staged} grazing_seconds={:.6}",
                samples[2]
            );
        });
}

#[test]
fn impossible_parentage_and_fabricated_fathers_are_rejected() {
    let mut state = initial();
    state.next_id = 100;
    state.animals[0].pregnancy = Some(Pregnancy {
        father: 99,
        father_genome: genome(Sex::Male, 900),
        due: 1,
    });
    assert!(
        state
            .validate(&HashSet::from([1]), &HashSet::from([1]), 0)
            .is_err()
    );
    state.animals[0].pregnancy = None;
    let mut child = animal(3, Sex::Male, 100);
    child.parents = Some([1, 99]);
    state.animals.push(child);
    assert!(
        state
            .validate(&HashSet::from([1]), &HashSet::from([1]), 0)
            .is_err()
    );
}

#[test]
fn invalid_food_and_birth_overflow_leave_the_original_generation_unchanged() {
    let mut state = initial();
    state.next_id = u64::MAX;
    state.animals[0].pregnancy = Some(Pregnancy {
        father: 2,
        father_genome: genome(Sex::Male, 900),
        due: 1,
    });
    let before = state.clone();
    let patch = Patch {
        id: 1,
        region: 1,
        good: 1,
        available: 10,
    };
    assert!(state.advance(&[patch, patch], 7, 1).is_err());
    assert!(state.advance(&[patch], 7, 1).is_err());
    assert_eq!(state, before);
}
