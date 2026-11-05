use game_parallel_fauna::{Animal, Fauna, Feeding, Patch, Pregnancy, Species};
use game_parallel_genetics::{Genome, Sex};
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
            .is_err()
    );
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
                let started = std::time::Instant::now();
                let draft = state.advance_validated(&[patch], 7, 1).unwrap();
                samples.push(started.elapsed().as_secs_f64());
                assert_eq!(draft.consumed, vec![(1, count)]);
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} animals={count} grazing_seconds={:.6}",
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
