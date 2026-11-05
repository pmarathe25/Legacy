use super::*;
use game_parallel_agreements::{Contract, Policy as AgreementPolicy, Status};
use game_parallel_autowork::Policy as WorkPolicy;
use game_parallel_lot_runtime::Snapshot as Physical;
use game_parallel_lots::{Container, Lot, Place, State as Custody};
use game_parallel_nutrition::Rules as NutritionRules;
use game_parallel_opinions::{Opinion, Rules as OpinionRules};
use game_parallel_production_model::{Amount, FoodProfile, Good, Recipe};
use std::collections::BTreeMap;

fn fixture() -> State {
    let goods = vec![
        Good {
            id: 1,
            name: "meal".into(),
            food: Some(FoodProfile {
                portion: 1,
                hunger_relief: 5,
                nutrients: BTreeMap::from([("energy".into(), 7)]),
                health_effect: -1,
                shelf_days: Some(10),
            }),
        },
        Good {
            id: 2,
            name: "material".into(),
            food: None,
        },
        Good {
            id: 3,
            name: "product".into(),
            food: None,
        },
    ];
    let stock = |good, quantity| {
        Container::new(
            Place::Stock { region: 1, good },
            10,
            if quantity == 0 {
                vec![]
            } else {
                vec![Lot {
                    good,
                    quantity,
                    expires_at: None,
                }]
            },
        )
        .unwrap()
    };
    let holder = |person, quantity| {
        Container::new(
            Place::Person(person),
            10,
            if quantity == 0 {
                vec![]
            } else {
                vec![Lot {
                    good: 1,
                    quantity,
                    expires_at: Some(10),
                }]
            },
        )
        .unwrap()
    };
    State {
        seed: 7,
        physical: Physical {
            custody: Custody {
                tick: 0,
                revision: 0,
                containers: vec![
                    stock(1, 0),
                    stock(2, 2),
                    stock(3, 0),
                    holder(1, 1),
                    holder(2, 0),
                    holder(3, 0),
                    holder(4, 1),
                ],
            },
            goods,
            recipes: vec![Recipe {
                id: 1,
                name: "make".into(),
                ticks: 1,
                inputs: vec![Amount {
                    good: 2,
                    quantity: 1,
                }],
                outputs: vec![Amount {
                    good: 3,
                    quantity: 1,
                }],
            }],
            tick_millis: 86_400_000,
            work: vec![],
        },
        people: (1..=4)
            .map(|id| Person {
                id,
                region: Some(1),
                health: 100,
                hunger: if id == 1 { 5 } else { 0 },
                willing_to_work: id == 2,
                seek_meals: id == 1,
                controlled: false,
            })
            .collect(),
        rules: Rules {
            hunger_per_tick: 1,
            meal_threshold: 5,
            starvation_threshold: 100,
            starvation_damage: 1,
            max_health: 100,
            carrying_capacity: 10,
            max_work_hunger: 10,
        },
        market: None,
        agreements: Some(Agreements {
            offering: vec![],
            work_offering: vec![],
            next_id: 2,
            policies: vec![AgreementPolicy {
                person: 4,
                honor: false,
                accept: false,
                minimum_trust: None,
                minimum_honor_trust: None,
                fear_honor_threshold: Some(3),
                obligation_honor_threshold: None,
                accept_pressure_threshold: None,
            }],
            bulk_debtors: vec![],
            contracts: vec![Contract {
                id: 1,
                debtor: 4,
                creditor: 1,
                good: 1,
                quantity: 1,
                delivered: 0,
                due: 1,
                status: Status::Active,
            }],
            events: vec![],
        }),
        opinions: Some(Opinions {
            rules: OpinionRules {
                minimum: -10,
                maximum: 10,
                initial: 0,
                fulfilled_gain: 1,
                default_loss: 1,
                theft_loss: 0,
                encounter_gain: 0,
                fear_per_damage: 0,
                fear_decay_per_tick: 0,
                gratitude_gain: 0,
                obligation_relief: 0,
                minimum_report_trust: None,
                affinity: None,
                newborn_obligation_development: None,
            },
            sensitivities: vec![].into(),
            records: vec![Opinion {
                observer: 4,
                target: 1,
                trust: 0,
                fear: 5,
                obligation: 0,
                affinity: 0,
            }],
            events: vec![],
        }),
        autowork: Some(Autowork {
            next_id: 1,
            policies: vec![WorkPolicy {
                person: 2,
                recipe: 1,
                enabled: true,
                disregard_claims: false,
            }],
            requests: None,
        }),
        work_tools: vec![],
        work_sites: None,
        construction: None,
        goals: None,
        dreams: None,
        nutrition: Some(Nutrition {
            rules: NutritionRules {
                interval: 1,
                capacity: [100; 6],
                initial: [0; 6],
                metabolism: [0; 6],
                minimum: [0; 6],
                deficiency_damage: 0,
                scalar_meal: [0; 6],
            },
            people: vec![],
        }),
        pending_food: vec![],
        travel: None,
        family: None,
        care: None,
        agriculture: None,
        dynamics: None,
        missions: None,
        campaign: None,
        skills: None,
        knowledge: None,
        combat: None,
        fatigue: None,
        environment: None,
        purpose: None,
        recovery: None,
        gathering: None,
        fauna: None,
        animal_care: None,
        animal_feelings: None,
        hunting: None,
        food_search: None,
        giving: None,
        medical: None,
        hauling: None,
        local_transport: None,
    }
}

#[test]
fn autonomous_animal_care_uses_real_food_and_personal_motivation() {
    use game_parallel_genetics::{Genome, Sex};
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.fauna = Some(Fauna {
        last_tick: 0,
        next_id: 2,
        species: vec![AnimalSpecies {
            id: 1,
            name: "grazer".into(),
            food_good: 1,
            intake: 2,
            hunger_per_tick: 1,
            relief_per_unit: 1,
            starvation_threshold: 10,
            starvation_damage: 1,
            max_health: 10,
            adult_age: 1,
            fertile_until: 10,
            lifespan: 30,
            gestation: 2,
            conception_one_in: 10,
            breeding_max_hunger: 5,
            newborn_health: 10,
            mutation_per_million: 0,
            mutation_step: 0,
            climate: None,
            hunting: None,
        }],
        animals: vec![Animal {
            id: 1,
            species: 1,
            region: 1,
            age: 1,
            health: 10,
            hunger: 4,
            genome: Genome {
                sex: Sex::Male,
                alleles: [[100; 2]; 4],
            },
            pregnancy: None,
            parents: None,
            social_response: Some(game_parallel_fauna::SocialResponse {
                feed_affinity: 2,
                minimum: -10,
                maximum: 10,
                capacity: 4,
            }),
            opinions: vec![],
        }],
    });
    initial.animal_care = Some(AnimalCare {
        policies: vec![
            AnimalCarePolicy {
                person: 1,
                animal: 1,
                good: 1,
                quantity: 1,
                willing: true,
                health_at_most: 10,
                affinity_at_least: None,
                affinity_at_most: None,
                pressured_by: None,
                pressure_threshold: None,
            },
            AnimalCarePolicy {
                person: 4,
                animal: 1,
                good: 1,
                quantity: 1,
                willing: false,
                health_at_most: 0,
                affinity_at_least: Some(9),
                affinity_at_most: None,
                pressured_by: Some(1),
                pressure_threshold: Some(3),
            },
        ],
    });
    let mut invalid = initial.clone();
    invalid.fauna.as_mut().unwrap().animals[0]
        .opinions
        .push(game_parallel_fauna::AnimalOpinion {
            person: 99,
            affinity: 1,
        });
    assert!(Economy::compile(invalid).is_err());
    let mut changing = Economy::compile(initial.clone()).unwrap();
    let stale = changing.prepare().unwrap();
    changing.set_animal_care_choice(1, false).unwrap();
    assert!(changing.commit(stale).is_err());
    changing.advance().unwrap();
    assert_eq!(
        changing.snapshot().fauna.as_ref().unwrap().animals[0].hunger,
        4
    );
    let run = |state: State, workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(state).unwrap();
                let saved = world.snapshot();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    let pressured = run(initial.clone(), 1);
    assert_eq!(pressured, run(initial.clone(), 4));
    assert_eq!(pressured.fauna.as_ref().unwrap().animals[0].hunger, 3);
    assert_eq!(
        pressured.fauna.as_ref().unwrap().animals[0]
            .opinions
            .iter()
            .map(|row| (row.person, row.affinity))
            .collect::<Vec<_>>(),
        vec![(1, 2), (4, 2)]
    );
    for id in [1, 4] {
        assert_eq!(
            pressured
                .physical
                .custody
                .containers
                .iter()
                .find(|c| c.place == Place::Person(id))
                .unwrap()
                .used,
            0
        );
    }
    let mut unwilling = initial.clone();
    unwilling.opinions.as_mut().unwrap().records.clear();
    let unwilling = run(unwilling, 4);
    assert_eq!(unwilling.fauna.as_ref().unwrap().animals[0].hunger, 4);
    assert_eq!(
        unwilling.fauna.as_ref().unwrap().animals[0].opinions.len(),
        1
    );
    assert_eq!(
        unwilling
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(4))
            .unwrap()
            .used,
        1
    );
    let mut mistaken = initial.clone();
    mistaken.fauna.as_mut().unwrap().species[0].food_good = 2;
    let mistaken = run(mistaken, 4);
    assert_eq!(mistaken.fauna.as_ref().unwrap().animals[0].hunger, 5);
    assert_eq!(
        mistaken.fauna.as_ref().unwrap().animals[0].opinions.len(),
        2
    );
    for id in [1, 4] {
        assert_eq!(
            mistaken
                .physical
                .custody
                .containers
                .iter()
                .find(|c| c.place == Place::Person(id))
                .unwrap()
                .used,
            0
        );
    }
    let mut obligated = initial.clone();
    let opinion = &mut obligated.opinions.as_mut().unwrap().records[0];
    opinion.fear = 0;
    opinion.obligation = 4;
    assert_eq!(
        run(obligated, 4).fauna.as_ref().unwrap().animals[0].hunger,
        3
    );
    let mut controlled = initial.clone();
    controlled.people[0].controlled = true;
    assert_eq!(
        run(controlled, 4).fauna.as_ref().unwrap().animals[0].hunger,
        4
    );
    let mut remote = initial.clone();
    remote.people[0].region = None;
    assert_eq!(run(remote, 4).fauna.as_ref().unwrap().animals[0].hunger, 4);
    let mut no_stock = initial;
    let holder = no_stock
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(4))
        .unwrap();
    *holder = Container::new(Place::Person(4), 10, vec![]).unwrap();
    assert_eq!(
        run(no_stock, 4).fauna.as_ref().unwrap().animals[0].hunger,
        4
    );
}

#[test]
fn people_interpret_their_own_animal_experiences_individually() {
    let mut initial = hunting_fixture();
    initial.fauna.as_mut().unwrap().animals[0].pregnancy = None;
    initial.hunting = None;
    initial.animal_care = Some(AnimalCare {
        policies: vec![
            AnimalCarePolicy {
                person: 1,
                animal: 1,
                good: 1,
                quantity: 1,
                willing: true,
                health_at_most: 10,
                affinity_at_least: None,
                affinity_at_most: Some(-1),
                pressured_by: None,
                pressure_threshold: None,
            },
            AnimalCarePolicy {
                person: 4,
                animal: 1,
                good: 1,
                quantity: 1,
                willing: true,
                health_at_most: 10,
                affinity_at_least: None,
                affinity_at_most: None,
                pressured_by: None,
                pressure_threshold: None,
            },
        ],
    });
    initial.animal_feelings = Some(AnimalFeelings {
        minds: vec![
            AnimalFeelingMind {
                person: 1,
                minimum: -10,
                maximum: 10,
                capacity: 4,
                on_feed: -3,
                on_hunt: 4,
                opinions: vec![AnimalFeelingOpinion {
                    animal: 1,
                    affinity: -2,
                }],
            },
            AnimalFeelingMind {
                person: 2,
                minimum: -10,
                maximum: 10,
                capacity: 4,
                on_feed: 9,
                on_hunt: 9,
                opinions: vec![],
            },
            AnimalFeelingMind {
                person: 4,
                minimum: -10,
                maximum: 10,
                capacity: 4,
                on_feed: 2,
                on_hunt: -4,
                opinions: vec![],
            },
        ],
    });
    let mut stale_world = Economy::compile(initial.clone()).unwrap();
    let stale = stale_world.prepare().unwrap();
    stale_world.set_animal_care_choice(1, false).unwrap();
    assert!(stale_world.commit(stale).is_err());
    assert_eq!(stale_world.animal_affinity(1, 1), Some(-2));
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                assert_eq!(world.animal_affinity(1, 1), Some(-5));
                assert_eq!(world.animal_affinity(4, 1), Some(2));
                assert_eq!(world.animal_affinity(2, 1), Some(0));
                let saved = world.snapshot();
                let restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                assert_eq!(restored.snapshot(), saved);
                saved
            })
    };
    assert_eq!(run(1), run(4));
    let mut hunter = initial;
    hunter.animal_care = None;
    hunter.people[0].controlled = true;
    let mut world = Economy::compile(hunter).unwrap();
    assert!(world.hunt_animal(1, 1).unwrap());
    assert_eq!(world.animal_affinity(1, 1), Some(2));
    assert_eq!(world.animal_affinity(4, 1), Some(0));
}

#[test]
fn a_persons_affinity_can_influence_their_hunting_choice_in_either_direction() {
    let mut state = hunting_fixture();
    state.fauna.as_mut().unwrap().animals[0].pregnancy = None;
    state.hunting.as_mut().unwrap().policies.truncate(1);
    state.animal_feelings = Some(AnimalFeelings {
        minds: vec![AnimalFeelingMind {
            person: 1,
            minimum: -10,
            maximum: 10,
            capacity: 4,
            on_feed: 0,
            on_hunt: 0,
            opinions: vec![AnimalFeelingOpinion {
                animal: 1,
                affinity: 3,
            }],
        }],
    });
    state.hunting.as_mut().unwrap().policies[0].affinity_at_most = Some(0);
    let mut avoid_liked = Economy::compile(state.clone()).unwrap();
    avoid_liked.advance().unwrap();
    assert!(avoid_liked.snapshot().fauna.unwrap().animals[0].health > 0);
    let policy = &mut state.hunting.as_mut().unwrap().policies[0];
    policy.affinity_at_most = None;
    policy.affinity_at_least = Some(3);
    let mut seek_liked = Economy::compile(state).unwrap();
    seek_liked.advance().unwrap();
    assert_eq!(seek_liked.snapshot().fauna.unwrap().animals[0].health, 0);
}

#[test]
fn personal_animal_memories_are_bounded_and_do_not_invent_other_feelings() {
    let mut state = hunting_fixture();
    state.fauna.as_mut().unwrap().animals[0].pregnancy = None;
    state.hunting = None;
    state.animal_care = Some(AnimalCare {
        policies: vec![AnimalCarePolicy {
            person: 1,
            animal: 2,
            good: 1,
            quantity: 1,
            willing: true,
            health_at_most: 10,
            affinity_at_least: None,
            affinity_at_most: None,
            pressured_by: None,
            pressure_threshold: None,
        }],
    });
    state.animal_feelings = Some(AnimalFeelings {
        minds: vec![AnimalFeelingMind {
            person: 1,
            minimum: -2,
            maximum: 2,
            capacity: 1,
            on_feed: 2,
            on_hunt: -2,
            opinions: vec![AnimalFeelingOpinion {
                animal: 1,
                affinity: -1,
            }],
        }],
    });
    let mut world = Economy::compile(state).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    let opinions = &state.animal_feelings.unwrap().minds[0].opinions;
    assert_eq!(
        opinions,
        &[AnimalFeelingOpinion {
            animal: 1,
            affinity: -1
        }]
    );
    assert_eq!(world.animal_affinity(1, 2), Some(0));
    assert_eq!(world.animal_affinity(2, 2), None);
}

fn hunting_fixture() -> State {
    use game_parallel_genetics::{Genome, Sex};
    let mut state = fixture();
    state.agreements = None;
    state.autowork = None;
    for person in &mut state.people {
        person.seek_meals = false;
    }
    let female = Genome {
        sex: Sex::Female,
        alleles: [[100; 2]; 4],
    };
    let male = Genome {
        sex: Sex::Male,
        alleles: [[900; 2]; 4],
    };
    state.fauna = Some(Fauna {
        last_tick: 0,
        next_id: 3,
        species: vec![AnimalSpecies {
            id: 1,
            name: "grazer".into(),
            food_good: 1,
            intake: 2,
            hunger_per_tick: 1,
            relief_per_unit: 1,
            starvation_threshold: 10,
            starvation_damage: 1,
            max_health: 10,
            adult_age: 1,
            fertile_until: 10,
            lifespan: 30,
            gestation: 2,
            conception_one_in: u64::MAX,
            breeding_max_hunger: 5,
            newborn_health: 10,
            mutation_per_million: 0,
            mutation_step: 0,
            climate: None,
            hunting: Some(game_parallel_fauna::HuntYield {
                good: 2,
                quantity: 2,
                success_per_thousand: 1000,
            }),
        }],
        animals: vec![
            Animal {
                id: 1,
                species: 1,
                region: 1,
                age: 1,
                health: 10,
                hunger: 1,
                genome: female,
                pregnancy: Some(game_parallel_fauna::Pregnancy {
                    father: 2,
                    father_genome: male.clone(),
                    due: 1,
                }),
                parents: None,
                social_response: None,
                opinions: vec![],
            },
            Animal {
                id: 2,
                species: 1,
                region: 1,
                age: 1,
                health: 10,
                hunger: 1,
                genome: male,
                pregnancy: None,
                parents: None,
                social_response: None,
                opinions: vec![],
            },
        ],
    });
    state.hunting = Some(Hunting {
        inherit_births: false,
        policies: vec![
            HuntingPolicy {
                person: 1,
                animal: Some(1),
                willing: true,
                affinity_at_least: None,
                affinity_at_most: None,
                hunger_at_least: None,
                reserve_good: None,
                reserve_below: None,
                pressured_by: None,
                pressure_threshold: None,
            },
            HuntingPolicy {
                person: 4,
                animal: Some(1),
                willing: false,
                affinity_at_least: None,
                affinity_at_most: None,
                hunger_at_least: None,
                reserve_good: None,
                reserve_below: None,
                pressured_by: Some(1),
                pressure_threshold: Some(3),
            },
        ],
    });
    state
}

#[test]
fn contested_hunting_conserves_yield_and_prevents_births() {
    let mut initial = hunting_fixture();
    initial.gathering = Some(Gathering {
        max_hunger: 20,
        inherit_births: false,
        nodes: vec![ResourceNode {
            id: 1,
            region: 1,
            good: 1,
            available: 10,
            capacity: 10,
            regrowth: 0,
            yield_per_action: 1,
        }],
        policies: vec![],
    });
    let run = |workers, reverse| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = initial.clone();
                if reverse {
                    state.hunting.as_mut().unwrap().policies.reverse();
                }
                let mut world = Economy::compile(state).unwrap();
                let stale = world.prepare().unwrap();
                world.advance().unwrap();
                let after = world.snapshot();
                assert!(world.commit(stale).is_err());
                assert_eq!(world.snapshot(), after);
                let fauna = after.fauna.as_ref().unwrap();
                assert_eq!(fauna.animals[0].health, 0);
                assert_eq!(fauna.animals.len(), 2);
                assert_eq!(after.gathering.as_ref().unwrap().nodes[0].available, 8);
                let yield_count: u64 = after
                    .physical
                    .custody
                    .containers
                    .iter()
                    .filter(|c| matches!(c.place, Place::Person(_)))
                    .flat_map(|c| c.lots.iter())
                    .filter(|lot| lot.good == 2)
                    .map(|lot| lot.quantity)
                    .sum();
                assert_eq!(yield_count, 2);
                let saved = serde_json::to_string(&after).unwrap();
                let mut restored = Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                let next = world.snapshot();
                let next_yield: u64 = next
                    .physical
                    .custody
                    .containers
                    .iter()
                    .filter(|c| matches!(c.place, Place::Person(_)))
                    .flat_map(|c| c.lots.iter())
                    .filter(|lot| lot.good == 2)
                    .map(|lot| lot.quantity)
                    .sum();
                assert_eq!(next_yield, 2);
                after
            })
    };
    let a = run(1, false);
    let b = run(4, true);
    assert_eq!(a.fauna, b.fauna);
    assert_eq!(a.physical, b.physical);
}

#[test]
fn hunting_needs_capacity_and_yields_to_animal_feeding() {
    let mut full = hunting_fixture();
    full.hunting.as_mut().unwrap().policies.truncate(1);
    full.fauna.as_mut().unwrap().animals[0].pregnancy = None;
    let holder = full
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(
        Place::Person(1),
        10,
        vec![Lot {
            good: 1,
            quantity: 10,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut world = Economy::compile(full).unwrap();
    world.advance().unwrap();
    assert!(world.snapshot().fauna.unwrap().animals[0].health > 0);
    let mut feeding = hunting_fixture();
    feeding.animal_care = Some(AnimalCare {
        policies: vec![AnimalCarePolicy {
            person: 1,
            animal: 1,
            good: 1,
            quantity: 1,
            willing: true,
            health_at_most: 10,
            affinity_at_least: None,
            affinity_at_most: None,
            pressured_by: None,
            pressure_threshold: None,
        }],
    });
    let mut world = Economy::compile(feeding).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    assert!(state.fauna.as_ref().unwrap().animals[0].health > 0);
    assert_eq!(
        state
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(1))
            .unwrap()
            .used,
        0
    );
    assert!(
        !state
            .physical
            .custody
            .containers
            .iter()
            .filter(|c| matches!(c.place, Place::Person(_)))
            .flat_map(|c| c.lots.iter())
            .any(|lot| lot.good == 2)
    );
}

#[test]
fn independent_hunters_can_hunt_distinct_animals_together() {
    let mut state = hunting_fixture();
    let policy = &mut state.hunting.as_mut().unwrap().policies[1];
    policy.animal = Some(2);
    policy.willing = true;
    let mut world = Economy::compile(state).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    assert!(
        state
            .fauna
            .unwrap()
            .animals
            .iter()
            .all(|animal| animal.health == 0)
    );
    for person in [1, 4] {
        assert!(
            state
                .physical
                .custody
                .containers
                .iter()
                .find(|c| c.place == Place::Person(person))
                .unwrap()
                .lots
                .iter()
                .any(|lot| lot.good == 2 && lot.quantity == 2)
        );
    }
}

#[test]
fn hunters_can_select_another_local_animal_and_change_their_mind() {
    let mut initial = hunting_fixture();
    initial.fauna.as_mut().unwrap().animals[0].pregnancy = None;
    initial.hunting.as_mut().unwrap().policies.truncate(1);
    initial.hunting.as_mut().unwrap().policies[0].animal = None;
    let mut invalid = initial.clone();
    invalid.fauna = None;
    assert!(Economy::compile(invalid).is_err());
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let stale = world.prepare().unwrap();
                world.set_hunting_choice(1, false).unwrap();
                assert!(world.commit(stale).is_err());
                world.advance().unwrap();
                assert!(
                    world
                        .snapshot()
                        .fauna
                        .as_ref()
                        .unwrap()
                        .animals
                        .iter()
                        .all(|a| a.health > 0)
                );
                world.set_hunting_choice(1, true).unwrap();
                world.advance().unwrap();
                assert_eq!(
                    world
                        .snapshot()
                        .fauna
                        .as_ref()
                        .unwrap()
                        .animals
                        .iter()
                        .filter(|a| a.health == 0)
                        .count(),
                    1
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&world.snapshot()).unwrap())
                        .unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                let state = world.snapshot();
                assert!(
                    state
                        .fauna
                        .as_ref()
                        .unwrap()
                        .animals
                        .iter()
                        .all(|a| a.health == 0)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn individual_hunting_preferences_can_consider_hunger_and_carried_reserves() {
    let mut initial = hunting_fixture();
    initial.fauna.as_mut().unwrap().animals[0].pregnancy = None;
    initial.people[0].hunger = 4;
    initial.hunting.as_mut().unwrap().policies.truncate(1);
    let policy = &mut initial.hunting.as_mut().unwrap().policies[0];
    policy.animal = None;
    policy.hunger_at_least = Some(5);
    policy.reserve_good = Some(2);
    policy.reserve_below = Some(2);
    let mut invalid = initial.clone();
    invalid.hunting.as_mut().unwrap().policies[0].reserve_good = None;
    assert!(Economy::compile(invalid).is_err());
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                assert!(
                    world
                        .snapshot()
                        .fauna
                        .as_ref()
                        .unwrap()
                        .animals
                        .iter()
                        .all(|a| a.health > 0)
                );
                world.advance().unwrap();
                let saved = world.snapshot();
                assert_eq!(
                    saved
                        .fauna
                        .as_ref()
                        .unwrap()
                        .animals
                        .iter()
                        .filter(|a| a.health == 0)
                        .count(),
                    1
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                let state = world.snapshot();
                assert_eq!(
                    state
                        .fauna
                        .as_ref()
                        .unwrap()
                        .animals
                        .iter()
                        .filter(|a| a.health == 0)
                        .count(),
                    1
                );
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(1))
                        .unwrap()
                        .lots
                        .iter()
                        .filter(|lot| lot.good == 2)
                        .map(|lot| lot.quantity)
                        .sum::<u64>(),
                    2
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn controlled_hunting_is_a_choice_and_unwilling_hunters_need_pressure() {
    let mut initial = hunting_fixture();
    initial.hunting.as_mut().unwrap().policies.remove(0);
    initial.opinions.as_mut().unwrap().records.clear();
    initial.fauna.as_mut().unwrap().animals[0].pregnancy = None;
    let mut world = Economy::compile(initial.clone()).unwrap();
    world.advance().unwrap();
    assert!(world.snapshot().fauna.unwrap().animals[0].health > 0);
    initial.people[0].controlled = true;
    let mut world = Economy::compile(initial).unwrap();
    assert!(world.hunt_animal(1, 1).unwrap());
    assert_eq!(world.snapshot().fauna.unwrap().animals[0].health, 0);
    assert!(world.hunt_animal(1, 1).is_err());
}

#[test]
fn fear_can_motivate_hunting_despite_personal_affinity_and_reserve_preferences() {
    let mut state = hunting_fixture();
    state.fauna.as_mut().unwrap().animals[0].pregnancy = None;
    state.hunting.as_mut().unwrap().policies.remove(0);
    let policy = &mut state.hunting.as_mut().unwrap().policies[0];
    policy.affinity_at_least = Some(9);
    policy.hunger_at_least = Some(99);
    policy.reserve_good = Some(2);
    policy.reserve_below = Some(0);
    let mut afraid = Economy::compile(state.clone()).unwrap();
    afraid.advance().unwrap();
    assert_eq!(afraid.snapshot().fauna.unwrap().animals[0].health, 0);
    state.opinions.as_mut().unwrap().records.clear();
    let mut unafraid = Economy::compile(state).unwrap();
    unafraid.advance().unwrap();
    assert!(unafraid.snapshot().fauna.unwrap().animals[0].health > 0);
}

#[test]
fn an_attempt_does_not_require_private_knowledge_of_the_animals_yield() {
    let mut state = hunting_fixture();
    state.fauna.as_mut().unwrap().species[0].hunting = None;
    state.fauna.as_mut().unwrap().animals[0].pregnancy = None;
    state.hunting.as_mut().unwrap().policies.truncate(1);
    state.people[0].willing_to_work = true;
    state.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![WorkPolicy {
            person: 1,
            recipe: 1,
            enabled: true,
            disregard_claims: false,
        }],
        requests: None,
    });
    let mut attempting = Economy::compile(state.clone()).unwrap();
    attempting.advance().unwrap();
    assert!(attempting.snapshot().physical.work.is_empty());
    state.hunting = None;
    let mut not_attempting = Economy::compile(state.clone()).unwrap();
    not_attempting.advance().unwrap();
    assert!(!not_attempting.snapshot().physical.work.is_empty());
    state.people[0].controlled = true;
    let mut controlled = Economy::compile(state).unwrap();
    assert!(!controlled.hunt_animal(1, 1).unwrap());
    assert_eq!(controlled.tick(), 1);
}

#[test]
#[ignore]
fn concentrated_hunting_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let flexible = std::env::var_os("LEGACY_FLEXIBLE_PREY").is_some();
    assert!(count >= 128);
    let mut state = concentrated(count);
    state.autowork = None;
    state.nutrition = None;
    for person in &mut state.people {
        person.seek_meals = false;
        person.hunger = 0;
    }
    state.fauna = hunting_fixture().fauna;
    state.hunting = Some(Hunting {
        inherit_births: false,
        policies: (1..=count)
            .map(|person| HuntingPolicy {
                person,
                animal: (!flexible).then_some(1),
                willing: true,
                affinity_at_least: None,
                affinity_at_most: None,
                hunger_at_least: None,
                reserve_good: None,
                reserve_below: None,
                pressured_by: None,
                pressure_threshold: None,
            })
            .collect(),
    });
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(state.clone()).unwrap();
                let start = std::time::Instant::now();
                world.advance().unwrap();
                let elapsed = start.elapsed().as_secs_f64();
                let state = world.snapshot();
                assert_eq!(state.fauna.unwrap().animals[0].health, 0);
                let yield_count: u64 = state
                    .physical
                    .custody
                    .containers
                    .iter()
                    .filter(|c| matches!(c.place, Place::Person(_)))
                    .flat_map(|c| c.lots.iter())
                    .filter(|lot| lot.good == 2)
                    .map(|lot| lot.quantity)
                    .sum();
            assert_eq!(yield_count, if flexible { 4 } else { 2 });
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
            "workers={workers} people={count} flexible={flexible} concentrated_hunting_seconds={:.6}",
                samples[1]
            );
        });
}

fn local_store_fixture() -> State {
    let mut state = fixture();
    state.autowork = None;
    state.agreements = None;
    for person in &mut state.people {
        person.seek_meals = false;
    }
    state.physical.custody.containers.extend([
        Container::new(
            Place::Inventory(1),
            10,
            vec![Lot {
                good: 3,
                quantity: 2,
                expires_at: Some(10),
            }],
        )
        .unwrap(),
        Container::new(Place::Inventory(2), 10, vec![]).unwrap(),
    ]);
    state
        .physical
        .custody
        .containers
        .sort_unstable_by_key(|row| row.place);
    state.local_transport = Some(LocalTransport {
        goal_fetchers: vec![],
        stores: vec![
            LocalStore {
                id: 1,
                region: 1,
                name: "source".into(),
            },
            LocalStore {
                id: 2,
                region: 1,
                name: "destination".into(),
            },
        ],
        policies: vec![LocalTransportPolicy {
            person: 2,
            region: 1,
            source: LocalEndpoint::Store { id: 1 },
            destination: LocalEndpoint::Store { id: 2 },
            good: 3,
            quantity: 1,
            willing: true,
            pressured_by: None,
            pressure_threshold: None,
            disregard_claims: false,
        }],
        events: vec![],
    });
    state
}

#[test]
fn local_carrier_keeps_actual_goods_when_they_change_their_mind() {
    let initial = local_store_fixture();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let pickup = world.snapshot();
                assert_eq!(
                    pickup
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Person(2))
                        .unwrap()
                        .lots[0]
                        .expires_at,
                    Some(10)
                );
                assert_eq!(
                    pickup
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Inventory(2))
                        .unwrap()
                        .used,
                    0
                );
                let stale = world.prepare().unwrap();
                world.set_local_carrying_choice(2, false).unwrap();
                assert!(world.commit(stale).is_err());
                world.advance().unwrap();
                let paused = world.snapshot();
                assert_eq!(
                    paused
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Person(2))
                        .unwrap()
                        .used,
                    1
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&paused).unwrap()).unwrap(),
                )
                .unwrap();
                world.set_local_carrying_choice(2, true).unwrap();
                restored.set_local_carrying_choice(2, true).unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                let destination = state
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|row| row.place == Place::Inventory(2))
                    .unwrap();
                assert_eq!(
                    (destination.used, destination.lots[0].expires_at),
                    (1, Some(10))
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn local_delivery_makes_real_inputs_available_for_later_work() {
    let mut initial = local_store_fixture();
    initial.people[2].willing_to_work = true;
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![WorkPolicy {
            person: 3,
            recipe: 1,
            enabled: true,
            disregard_claims: false,
        }],
        requests: None,
    });
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 2 })
        .unwrap();
    *stock = Container::new(stock.place, stock.capacity, vec![]).unwrap();
    let source = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Inventory(1))
        .unwrap();
    *source = Container::new(
        source.place,
        source.capacity,
        vec![Lot {
            good: 2,
            quantity: 1,
            expires_at: Some(10),
        }],
    )
    .unwrap();
    let policy = &mut initial.local_transport.as_mut().unwrap().policies[0];
    policy.good = 2;
    policy.destination = LocalEndpoint::Stock;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.advance().unwrap().started.is_empty());
                assert!(world.advance().unwrap().started.is_empty());
                let state = world.snapshot();
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Stock { region: 1, good: 2 })
                        .unwrap()
                        .used,
                    1
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                assert_eq!(world.advance().unwrap().started, vec![1]);
                assert_eq!(restored.advance().unwrap().started, vec![1]);
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn a_person_fetches_observed_local_inputs_for_their_own_work_goal() {
    use game_parallel_goals::Goal;
    let mut initial = local_store_fixture();
    initial.local_transport.as_mut().unwrap().policies.clear();
    initial.local_transport.as_mut().unwrap().goal_fetchers = vec![GoalFetchPreference {
        person: 2,
        enabled: true,
        max_quantity: 1,
    }];
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: None,
    });
    initial.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: false,
        people: vec![Goal {
            person: 2,
            recipe: 1,
            priority: 1,
            target: 1,
            progress: 0,
            next_review: 0,
            enabled: true,
            recognize_help: false,
        }],
        events: vec![],
    });
    let source = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Inventory(1))
        .unwrap();
    *source = Container::new(
        source.place,
        source.capacity,
        vec![Lot {
            good: 2,
            quantity: 1,
            expires_at: Some(10),
        }],
    )
    .unwrap();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 2 })
        .unwrap();
    *stock = Container::new(stock.place, stock.capacity, vec![]).unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.advance().unwrap().started.is_empty());
                let picked = world.snapshot();
                assert!(picked.local_transport.as_ref().unwrap().events[0].moved);
                assert_eq!(
                    picked
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Person(2))
                        .unwrap()
                        .lots[0]
                        .expires_at,
                    Some(10)
                );
                assert_eq!(picked.goals.as_ref().unwrap().people[0].next_review, 0);
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&picked).unwrap()).unwrap(),
                )
                .unwrap();
                assert!(world.advance().unwrap().started.is_empty());
                assert!(restored.advance().unwrap().started.is_empty());
                assert_eq!(world.advance().unwrap().started, vec![1]);
                assert_eq!(restored.advance().unwrap().started, vec![1]);
                assert_eq!(world.advance().unwrap().finished, vec![1]);
                assert_eq!(restored.advance().unwrap().finished, vec![1]);
                assert_eq!(world.snapshot(), restored.snapshot());
                assert_eq!(
                    world.snapshot().goals.as_ref().unwrap().people[0].progress,
                    1
                );
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
    let mut disabled = Economy::compile(initial.clone()).unwrap();
    disabled.set_goal_fetch_choice(2, false).unwrap();
    disabled.advance().unwrap();
    assert!(
        disabled
            .snapshot()
            .local_transport
            .unwrap()
            .events
            .is_empty()
    );
    let mut remote = initial;
    remote.local_transport.as_mut().unwrap().stores[0].region = 2;
    remote
        .physical
        .custody
        .containers
        .push(Container::new(Place::Stock { region: 2, good: 2 }, 10, vec![]).unwrap());
    remote
        .physical
        .custody
        .containers
        .sort_unstable_by_key(|row| row.place);
    let mut remote = Economy::compile(remote).unwrap();
    remote.advance().unwrap();
    assert!(remote.snapshot().local_transport.unwrap().events.is_empty());
}

#[test]
fn goal_fetching_finds_inputs_for_a_real_production_chain() {
    use game_parallel_goals::Goal;
    let mut initial = local_store_fixture();
    let transport = initial.local_transport.as_mut().unwrap();
    transport.policies.clear();
    transport.goal_fetchers = vec![GoalFetchPreference {
        person: 2,
        enabled: true,
        max_quantity: 1,
    }];
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: None,
    });
    initial.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: true,
        people: vec![Goal {
            person: 2,
            recipe: 1,
            priority: 1,
            target: 1,
            progress: 0,
            next_review: 0,
            enabled: true,
            recognize_help: false,
        }],
        events: vec![],
    });
    initial.physical.recipes.push(Recipe {
        id: 2,
        name: "prepare material".into(),
        ticks: 1,
        inputs: vec![Amount {
            good: 3,
            quantity: 1,
        }],
        outputs: vec![Amount {
            good: 2,
            quantity: 1,
        }],
    });
    let source = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Inventory(1))
        .unwrap();
    *source = Container::new(
        source.place,
        source.capacity,
        vec![Lot {
            good: 3,
            quantity: 1,
            expires_at: Some(20),
        }],
    )
    .unwrap();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 2 })
        .unwrap();
    *stock = Container::new(stock.place, stock.capacity, vec![]).unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                assert_eq!(
                    world.snapshot().local_transport.as_ref().unwrap().events[0].good,
                    3
                );
                world.advance().unwrap();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&world.snapshot()).unwrap())
                        .unwrap(),
                )
                .unwrap();
                for _ in 0..5 {
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                assert_eq!(world.snapshot(), restored.snapshot());
                assert_eq!(
                    world.snapshot().goals.as_ref().unwrap().people[0].progress,
                    1
                );
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
    let mut cyclic = initial.clone();
    let source = cyclic
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Inventory(1))
        .unwrap();
    *source = Container::new(source.place, source.capacity, vec![]).unwrap();
    let mut cyclic = Economy::compile(cyclic).unwrap();
    cyclic.advance().unwrap();
    assert!(cyclic.snapshot().local_transport.unwrap().events.is_empty());
    let mut direct_only = initial;
    direct_only.goals.as_mut().unwrap().plan_inputs = false;
    let mut direct_only = Economy::compile(direct_only).unwrap();
    direct_only.advance().unwrap();
    assert!(
        direct_only
            .snapshot()
            .local_transport
            .unwrap()
            .events
            .is_empty()
    );
}

#[test]
fn local_store_actions_respect_location_capacity_and_expiry() {
    let mut initial = local_store_fixture();
    initial.people[0].controlled = true;
    initial.local_transport.as_mut().unwrap().policies.clear();
    let mut world = Economy::compile(initial.clone()).unwrap();
    assert!(!world.move_local_store(1, 1, 3, 3, false).unwrap());
    assert!(world.move_local_store(1, 1, 3, 1, false).unwrap());
    assert!(world.move_local_store(1, 2, 3, 1, true).unwrap());
    let before = world.snapshot();
    assert!(world.move_local_store(1, 99, 3, 1, false).is_err());
    assert_eq!(world.snapshot(), before);
    let mut remote = initial.clone();
    remote.local_transport.as_mut().unwrap().stores[1].region = 2;
    assert!(Economy::compile(remote.clone()).is_err());
    remote
        .physical
        .custody
        .containers
        .push(Container::new(Place::Stock { region: 2, good: 1 }, 10, vec![]).unwrap());
    remote
        .physical
        .custody
        .containers
        .sort_unstable_by_key(|row| row.place);
    let mut remote = Economy::compile(remote).unwrap();
    let before = remote.snapshot();
    assert!(remote.move_local_store(1, 2, 3, 1, false).is_err());
    assert_eq!(remote.snapshot(), before);
    let mut orphan = initial.clone();
    orphan.local_transport = None;
    assert!(Economy::compile(orphan).is_err());
    let source = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Inventory(1))
        .unwrap();
    *source = Container::new(
        source.place,
        source.capacity,
        vec![Lot {
            good: 3,
            quantity: 1,
            expires_at: Some(1),
        }],
    )
    .unwrap();
    let mut world = Economy::compile(initial).unwrap();
    assert!(!world.move_local_store(1, 1, 3, 1, false).unwrap());
    assert_eq!(
        world
            .snapshot()
            .physical
            .custody
            .containers
            .iter()
            .find(|row| row.place == Place::Person(1))
            .unwrap()
            .lots
            .iter()
            .filter(|lot| lot.good == 3)
            .count(),
        0
    );
}

#[test]
fn local_carrying_can_be_motivated_by_fear_without_obligation() {
    let mut initial = local_store_fixture();
    initial.opinions.as_mut().unwrap().records.push(Opinion {
        observer: 2,
        target: 1,
        trust: 0,
        fear: 5,
        obligation: 0,
        affinity: 0,
    });
    let policy = &mut initial.local_transport.as_mut().unwrap().policies[0];
    policy.willing = false;
    policy.pressured_by = Some(1);
    policy.pressure_threshold = Some(5);
    let mut world = Economy::compile(initial).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    assert!(state.local_transport.as_ref().unwrap().events[0].moved);
    assert_eq!(state.opinions.unwrap().index().obligation(2, 1), 0);
}

fn concentrated_local_carrying(count: u64) -> State {
    let mut state = field_workload(count);
    state.agriculture = None;
    state.physical.custody.containers.extend([
        Container::new(
            Place::Inventory(1),
            count,
            vec![Lot {
                good: 2,
                quantity: count / 8,
                expires_at: None,
            }],
        )
        .unwrap(),
        Container::new(Place::Inventory(2), count, vec![]).unwrap(),
    ]);
    state
        .physical
        .custody
        .containers
        .sort_unstable_by_key(|row| row.place);
    state.local_transport = Some(LocalTransport {
        goal_fetchers: vec![],
        stores: vec![
            LocalStore {
                id: 1,
                region: 1,
                name: "source".into(),
            },
            LocalStore {
                id: 2,
                region: 1,
                name: "destination".into(),
            },
        ],
        policies: (2..=count)
            .map(|person| LocalTransportPolicy {
                person,
                region: 1,
                source: LocalEndpoint::Store { id: 1 },
                destination: LocalEndpoint::Store { id: 2 },
                good: 2,
                quantity: 1,
                willing: true,
                pressured_by: None,
                pressure_threshold: None,
                disregard_claims: false,
            })
            .collect(),
        events: vec![],
    });
    state
}

fn concentrated_goal_fetching(count: u64) -> State {
    use game_parallel_goals::Goal;
    let mut state = concentrated_local_carrying(count);
    let transport = state.local_transport.as_mut().unwrap();
    transport.policies.clear();
    transport.goal_fetchers = (2..=count)
        .map(|person| GoalFetchPreference {
            person,
            enabled: true,
            max_quantity: 1,
        })
        .collect();
    state.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: None,
    });
    state.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: false,
        people: (2..=count)
            .map(|person| Goal {
                person,
                recipe: 1,
                priority: 1,
                target: 1,
                progress: 0,
                next_review: 0,
                enabled: true,
                recognize_help: false,
            })
            .collect(),
        events: vec![],
    });
    for person in &mut state.people {
        person.willing_to_work = true;
    }
    let stock = state
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 2 })
        .unwrap();
    *stock = Container::new(stock.place, stock.capacity, vec![]).unwrap();
    state
}

fn concentrated_chain_fetching(count: u64) -> State {
    let mut state = concentrated_goal_fetching(count);
    state.goals.as_mut().unwrap().plan_inputs = true;
    state.physical.recipes.push(Recipe {
        id: 2,
        name: "prepare material".into(),
        ticks: 1,
        inputs: vec![Amount {
            good: 3,
            quantity: 1,
        }],
        outputs: vec![Amount {
            good: 2,
            quantity: 1,
        }],
    });
    let source = state
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Inventory(1))
        .unwrap();
    *source = Container::new(
        source.place,
        source.capacity,
        vec![Lot {
            good: 3,
            quantity: count / 8,
            expires_at: None,
        }],
    )
    .unwrap();
    state
}

#[test]
fn concentrated_chain_fetching_conserves_goods_across_workers() {
    let count = 4096;
    let initial = concentrated_chain_fetching(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state
                        .local_transport
                        .as_ref()
                        .unwrap()
                        .events
                        .iter()
                        .filter(|event| event.moved && event.good == 3)
                        .count(),
                    count as usize / 8
                );
                let carried: u64 = state
                    .physical
                    .custody
                    .containers
                    .iter()
                    .filter(|row| matches!(row.place, Place::Person(_)))
                    .flat_map(|row| &row.lots)
                    .filter(|lot| lot.good == 3)
                    .map(|lot| lot.quantity)
                    .sum();
                assert_eq!(carried, count / 8);
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn concentrated_goal_fetching_conserves_goods_across_workers() {
    let count = 4096;
    let initial = concentrated_goal_fetching(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state
                        .local_transport
                        .as_ref()
                        .unwrap()
                        .events
                        .iter()
                        .filter(|event| event.moved)
                        .count(),
                    count as usize / 8
                );
                let carried: u64 = state
                    .physical
                    .custody
                    .containers
                    .iter()
                    .filter(|row| matches!(row.place, Place::Person(_)))
                    .flat_map(|row| &row.lots)
                    .filter(|lot| lot.good == 2)
                    .map(|lot| lot.quantity)
                    .sum();
                assert_eq!(carried, count / 8);
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore = "performance measurement"]
fn concentrated_goal_fetch_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = if std::env::var_os("LEGACY_CHAIN").is_some() {
        concentrated_chain_fetching(count)
    } else {
        concentrated_goal_fetching(count)
    };
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let start = std::time::Instant::now();
                world.advance().unwrap();
                let elapsed = start.elapsed().as_secs_f64();
                assert_eq!(
                    world
                        .snapshot()
                        .local_transport
                        .unwrap()
                        .events
                        .iter()
                        .filter(|event| event.moved)
                        .count(),
                    count as usize / 8
                );
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} goal_fetch_seconds={:.6}",
                samples[1]
            );
        });
}

#[test]
fn concentrated_local_withdrawals_conserve_goods_across_workers() {
    let count = 4096;
    let initial = concentrated_local_carrying(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state
                        .local_transport
                        .as_ref()
                        .unwrap()
                        .events
                        .iter()
                        .filter(|event| event.moved)
                        .count(),
                    count as usize / 8
                );
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Inventory(1))
                        .unwrap()
                        .used,
                    0
                );
                let carried: u64 = state
                    .physical
                    .custody
                    .containers
                    .iter()
                    .filter(|row| matches!(row.place, Place::Person(_)))
                    .flat_map(|row| &row.lots)
                    .filter(|lot| lot.good == 2)
                    .map(|lot| lot.quantity)
                    .sum();
                assert_eq!(carried, count / 8);
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore = "performance measurement"]
fn concentrated_local_carrying_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = concentrated_local_carrying(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let start = std::time::Instant::now();
                world.advance().unwrap();
                let elapsed = start.elapsed().as_secs_f64();
                assert_eq!(
                    world
                        .snapshot()
                        .local_transport
                        .unwrap()
                        .events
                        .iter()
                        .filter(|event| event.moved)
                        .count(),
                    count as usize / 8
                );
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} local_carry_seconds={:.6}",
                samples[1]
            );
        });
}

#[test]
fn regional_exposure_rotates_finite_shelter_and_survives_reload() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    initial.nutrition = None;
    initial.rules.hunger_per_tick = 0;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.environment = Some(Environment {
        regions: vec![Hazard {
            region: 1,
            damage: 10,
            shelter: 1,
            shelter_reduction: 8,
            shelter_good: Some(2),
        }],
        seasons: vec![Season {
            name: "mild".into(),
            duration: 10,
            food_growth_per_thousand: 1000,
            exposure_per_thousand: 500,
            planting_allowed: true,
            tending_allowed: true,
            harvest_allowed: true,
        }],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state.people.iter().map(|p| p.health).collect::<Vec<_>>(),
                    vec![99, 99, 99, 95]
                );
                let mut restored = Economy::compile(state).unwrap();
                restored.advance().unwrap();
                assert_eq!(
                    restored
                        .snapshot()
                        .people
                        .iter()
                        .map(|p| p.health)
                        .collect::<Vec<_>>(),
                    vec![94, 98, 98, 94]
                );
                restored.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
    let mut invalid = initial;
    invalid.environment.as_mut().unwrap().regions[0].shelter_good = Some(999);
    assert!(Economy::compile(invalid).is_err());
}

#[test]
fn shelter_capacity_follows_physical_stock_at_each_boundary() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    initial.nutrition = None;
    initial.rules.hunger_per_tick = 0;
    initial.people[0].controlled = true;
    for p in &mut initial.people {
        p.seek_meals = false;
    }
    initial.environment = Some(Environment {
        regions: vec![Hazard {
            region: 1,
            damage: 10,
            shelter: 0,
            shelter_reduction: 10,
            shelter_good: Some(2),
        }],
        seasons: vec![],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.transfer_stock(1, 2, 1, true).unwrap());
                assert_eq!(
                    world
                        .snapshot()
                        .people
                        .iter()
                        .map(|p| p.health)
                        .collect::<Vec<_>>(),
                    vec![100, 100, 90, 90]
                );
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state.people.iter().map(|p| p.health).collect::<Vec<_>>(),
                    vec![90, 100, 80, 80]
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn seasons_gate_planting_and_grow_crops_at_authored_rate() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    initial.rules.hunger_per_tick = 0;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.people[1].controlled = true;
    initial.agriculture = Some(Agriculture {
        inherit_births: false,
        crops: vec![Crop {
            id: 1,
            seed: 2,
            seed_quantity: 1,
            output: 1,
            output_quantity: 1,
            growth_ticks: 2,
        }],
        fields: vec![Field {
            id: 1,
            region: 1,
            crop: 1,
            phase: CropPhase::Fallow,
            remaining: 0,
        }],
        policies: vec![],
        max_hunger: 20,
    });
    let season = |name: &str, duration, planting_allowed, factor| Season {
        name: name.into(),
        duration,
        food_growth_per_thousand: factor,
        exposure_per_thousand: 1000,
        planting_allowed,
        tending_allowed: true,
        harvest_allowed: true,
    };
    initial.environment = Some(Environment {
        regions: vec![],
        seasons: vec![season("cold", 1, false, 0), season("warm", 8, true, 500)],
    });
    let mut world = Economy::compile(initial).unwrap();
    assert!(world.farm_field(2, 1).is_err());
    assert_eq!(world.tick(), 0);
    world.advance().unwrap();
    assert!(world.farm_field(2, 1).unwrap());
    for _ in 0..4 {
        world.advance().unwrap();
    }
    assert_eq!(
        world.snapshot().agriculture.unwrap().fields[0].phase,
        CropPhase::Ripe
    );
}

#[test]
fn travelers_are_exposed_even_when_shelter_is_available() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    initial.nutrition = None;
    initial.rules.hunger_per_tick = 0;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial
        .physical
        .custody
        .containers
        .push(Container::new(Place::Stock { region: 2, good: 1 }, 10, vec![]).unwrap());
    initial
        .physical
        .custody
        .containers
        .sort_unstable_by_key(|row| row.place);
    initial.travel = Some(Travel {
        routes: vec![Route {
            source: 1,
            destination: 2,
            ticks: 3,
        }],
        journeys: vec![],
    });
    initial.environment = Some(Environment {
        regions: vec![Hazard {
            region: 1,
            damage: 10,
            shelter: 10,
            shelter_reduction: 10,
            shelter_good: None,
        }],
        seasons: vec![],
    });
    let mut fatal = initial.clone();
    fatal.people[0].health = 10;
    let mut hungry = initial.clone();
    hungry.people[0].controlled = true;
    hungry.people[0].seek_meals = true;
    let mut world = Economy::compile(initial).unwrap();
    world
        .depart(&[Departure {
            person: 1,
            destination: 2,
        }])
        .unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    assert_eq!(state.people[0].health, 90);
    assert_eq!(state.people[1].health, 100);
    assert_eq!(state.people[0].region, None);
    let mut world = Economy::compile(fatal).unwrap();
    world
        .depart(&[Departure {
            person: 1,
            destination: 2,
        }])
        .unwrap();
    world.advance().unwrap();
    let saved = world.snapshot();
    assert_eq!(
        (saved.people[0].health, saved.people[0].region),
        (0, Some(1))
    );
    assert!(saved.travel.as_ref().unwrap().journeys.is_empty());
    assert!(
        saved
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(1))
            .unwrap()
            .used
            > 0
    );
    Economy::compile(saved).unwrap();
    let mut world = Economy::compile(hungry).unwrap();
    world
        .depart(&[Departure {
            person: 1,
            destination: 2,
        }])
        .unwrap();
    assert!(world.eat(1, 1).unwrap());
    assert_eq!(world.tick(), 1);
    assert_eq!(
        world.snapshot().travel.as_ref().unwrap().journeys[0].remaining,
        2
    );
    assert_eq!(
        world
            .snapshot()
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(1))
            .unwrap()
            .used,
        0
    );
}

#[test]
fn exposure_death_updates_actual_family_and_lineage() {
    let mut initial = campaign_fixture();
    initial.pending_food.clear();
    initial.people[2].health = 1;
    initial.environment = Some(Environment {
        regions: vec![Hazard {
            region: 1,
            damage: 1,
            shelter: 0,
            shelter_reduction: 0,
            shelter_good: None,
        }],
        seasons: vec![],
    });
    let mut world = Economy::compile(initial).unwrap();
    world.advance().unwrap();
    let saved = world.snapshot();
    assert_eq!(saved.people[2].health, 0);
    assert!(
        !saved
            .family
            .as_ref()
            .unwrap()
            .members
            .iter()
            .find(|m| m.id == 3)
            .unwrap()
            .alive
    );
    assert_eq!(saved.campaign.as_ref().unwrap().lineage.current, None);
    let mut restored =
        Economy::compile(serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap())
            .unwrap();
    world.advance().unwrap();
    restored.advance().unwrap();
    assert_eq!(world.snapshot(), restored.snapshot());
}

#[test]
fn travelers_eat_only_their_own_unexpired_supplies_while_the_journey_advances() {
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    initial.nutrition = None;
    for p in &mut initial.people {
        p.seek_meals = false;
    }
    initial.people[0].controlled = true;
    initial.people[3].seek_meals = true;
    initial.people[3].hunger = 5;
    initial
        .physical
        .custody
        .containers
        .push(Container::new(Place::Stock { region: 2, good: 1 }, 10, vec![]).unwrap());
    initial
        .physical
        .custody
        .containers
        .sort_unstable_by_key(|c| c.place);
    initial.travel = Some(Travel {
        routes: vec![Route {
            source: 1,
            destination: 2,
            ticks: 3,
        }],
        journeys: vec![],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world
                    .depart(&[
                        Departure {
                            person: 1,
                            destination: 2,
                        },
                        Departure {
                            person: 4,
                            destination: 2,
                        },
                    ])
                    .unwrap();
                let saved = world.snapshot();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                assert!(world.eat(1, 1).unwrap());
                assert!(restored.eat(1, 1).unwrap());
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert!(
                    state
                        .travel
                        .as_ref()
                        .unwrap()
                        .journeys
                        .iter()
                        .all(|j| j.remaining == 2)
                );
                for id in [1, 4] {
                    assert_eq!(
                        state
                            .physical
                            .custody
                            .containers
                            .iter()
                            .find(|c| c.place == Place::Person(id))
                            .unwrap()
                            .used,
                        0
                    );
                }
                assert_eq!(state.people[3].hunger, 1);
                state
            })
    };
    assert_eq!(run(1), run(4));
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(4))
        .unwrap();
    *holder = Container::new(
        holder.place,
        holder.capacity,
        vec![Lot {
            good: 1,
            quantity: 1,
            expires_at: Some(1),
        }],
    )
    .unwrap();
    let mut world = Economy::compile(initial).unwrap();
    world
        .depart(&[Departure {
            person: 4,
            destination: 2,
        }])
        .unwrap();
    world.advance().unwrap();
    assert_eq!(world.snapshot().people[3].hunger, 6);
}

#[test]
fn care_on_a_shared_journey_can_transmit_illness_and_feed_a_child() {
    use game_parallel_dependent_care::{Policy as CarePolicy, Rules as CareRules};
    let mut initial = campaign_fixture();
    initial.campaign = None;
    initial.pending_food.clear();
    initial.nutrition = None;
    initial.people[0].controlled = false;
    initial.people[0].seek_meals = false;
    initial.people[0].hunger = 0;
    initial.people[2].health = 100;
    initial.people[2].hunger = 5;
    initial.people[2].seek_meals = true;
    let family = initial.family.as_mut().unwrap();
    for member in &mut family.members {
        member.pregnancy = None;
        member.reproduction_allowed = false;
    }
    family.members[2].age = 1;
    family.members[2].parents = vec![1, 2].into();
    initial.travel.as_mut().unwrap().routes[0].ticks = 3;
    initial.care = Some(DependentCare {
        rules: CareRules {
            dependent_until: 5,
            max_caregiver_hunger: 100,
            neglect_damage: 0,
            feed_from_cargo: true,
            fetch_local_food: false,
            pressure_threshold: None,
        },
        inherit_births: false,
        policies: vec![CarePolicy {
            person: 1,
            willing: true,
        }],
        events: vec![],
    });
    let mut dynamics = infection_from(1);
    dynamics.disease.as_mut().unwrap().strains[0].transmission_per_million = 1_000_000;
    initial.dynamics = Some(dynamics);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world
                    .depart(&[
                        Departure {
                            person: 1,
                            destination: 2,
                        },
                        Departure {
                            person: 3,
                            destination: 2,
                        },
                    ])
                    .unwrap();
                world.advance().unwrap();
                let saved = world.snapshot();
                assert!(
                    saved
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .disease
                        .as_ref()
                        .unwrap()
                        .cases
                        .iter()
                        .any(|c| c.person == 3 && c.source == Some(1))
                );
                assert_eq!(
                    saved
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(3))
                        .unwrap()
                        .used,
                    1
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                assert_eq!(world.snapshot().people[2].hunger, 2);
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn companions_share_supplies_and_provide_care_on_the_same_journey() {
    use game_parallel_disease::Treatment;
    let mut initial = expedition_fixture();
    initial.people[0].controlled = true;
    initial.travel.as_mut().unwrap().routes[0].ticks = 3;
    let mut dynamics = infection_from(1);
    dynamics.disease.as_mut().unwrap().strains[0].transmission_per_million = 1_000_000;
    initial.dynamics = Some(dynamics);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world
                    .depart(&[
                        Departure {
                            person: 1,
                            destination: 2,
                        },
                        Departure {
                            person: 2,
                            destination: 2,
                        },
                    ])
                    .unwrap();
                assert!(world.give(1, 3, 1, 1).is_err());
                assert_eq!(world.tick(), 0);
                assert!(world.give(1, 2, 1, 1).unwrap());
                let saved = world.snapshot();
                assert!(
                    saved
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .disease
                        .as_ref()
                        .unwrap()
                        .cases
                        .iter()
                        .any(|c| c.person == 2 && c.source == Some(1))
                );
                assert_eq!(
                    saved
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap()
                        .used,
                    1
                );
                assert!(
                    saved
                        .travel
                        .as_ref()
                        .unwrap()
                        .journeys
                        .iter()
                        .all(|j| j.remaining == 2)
                );
                saved
            })
    };
    assert_eq!(run(1), run(4));
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(
        holder.place,
        holder.capacity,
        vec![Lot {
            good: 2,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut dynamics = infection_from(2);
    let disease = dynamics.disease.as_mut().unwrap();
    disease.strains[0].transmission_per_million = 1_000_000;
    disease.treatments.push(Treatment {
        strain: 1,
        medicine: 2,
    });
    initial.dynamics = Some(dynamics);
    let mut world = Economy::compile(initial).unwrap();
    world
        .depart(&[
            Departure {
                person: 1,
                destination: 2,
            },
            Departure {
                person: 2,
                destination: 2,
            },
        ])
        .unwrap();
    assert!(world.treat_other(1, 2, 2, 1).unwrap());
    let saved = world.snapshot();
    let cases = &saved
        .dynamics
        .as_ref()
        .unwrap()
        .disease
        .as_ref()
        .unwrap()
        .cases;
    assert!(!cases.iter().any(|c| c.person == 2));
    assert!(cases.iter().any(|c| c.person == 1 && c.source == Some(2)));
}

#[test]
fn companions_can_meet_on_a_shared_journey_without_forcing_their_response() {
    let mut initial = expedition_fixture();
    initial.people[0].controlled = true;
    initial.travel.as_mut().unwrap().routes[0].ticks = 3;
    let social = initial.dynamics.as_mut().unwrap().social.as_mut().unwrap();
    social
        .participants
        .iter_mut()
        .find(|p| p.person == 2)
        .unwrap()
        .willing = true;
    let mut disease = infection_from(1).disease.unwrap();
    disease.strains[0].transmission_per_million = 1_000_000;
    initial.dynamics.as_mut().unwrap().disease = Some(disease);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world
                    .depart(&[
                        Departure {
                            person: 1,
                            destination: 2,
                        },
                        Departure {
                            person: 2,
                            destination: 2,
                        },
                    ])
                    .unwrap();
                assert!(world.meet(1, 2).unwrap());
                let state = world.snapshot();
                assert!(
                    state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .disease
                        .as_ref()
                        .unwrap()
                        .cases
                        .iter()
                        .any(|c| c.person == 2 && c.source == Some(1))
                );
                assert_eq!(
                    state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .social
                        .as_ref()
                        .unwrap()
                        .events
                        .len(),
                    1
                );
                assert!(
                    state
                        .travel
                        .as_ref()
                        .unwrap()
                        .journeys
                        .iter()
                        .all(|j| j.remaining == 2)
                );
                Economy::compile(state.clone()).unwrap();
                state
            })
    };
    assert_eq!(run(1), run(4));
    initial
        .dynamics
        .as_mut()
        .unwrap()
        .social
        .as_mut()
        .unwrap()
        .participants
        .iter_mut()
        .find(|p| p.person == 2)
        .unwrap()
        .willing = false;
    let mut world = Economy::compile(initial).unwrap();
    world
        .depart(&[
            Departure {
                person: 1,
                destination: 2,
            },
            Departure {
                person: 2,
                destination: 2,
            },
        ])
        .unwrap();
    assert!(!world.meet(1, 2).unwrap());
    assert!(
        !world
            .snapshot()
            .dynamics
            .unwrap()
            .disease
            .unwrap()
            .cases
            .iter()
            .any(|c| c.person == 2)
    );
}

#[test]
fn autonomous_encounters_keep_travelers_separate_from_people_at_the_source() {
    use game_parallel_social::Participant;
    let mut initial = expedition_fixture();
    initial.travel.as_mut().unwrap().routes[0].ticks = 3;
    let social = initial.dynamics.as_mut().unwrap().social.as_mut().unwrap();
    social.participants = (1..=4)
        .map(|person| Participant {
            person,
            willing: true,
        })
        .collect();
    let mut disease = infection_from(1).disease.unwrap();
    disease.strains[0].transmission_per_million = 1_000_000;
    initial.dynamics.as_mut().unwrap().disease = Some(disease);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world
                    .depart(&[
                        Departure {
                            person: 1,
                            destination: 2,
                        },
                        Departure {
                            person: 2,
                            destination: 2,
                        },
                    ])
                    .unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let events = &state
                    .dynamics
                    .as_ref()
                    .unwrap()
                    .social
                    .as_ref()
                    .unwrap()
                    .events;
                assert_eq!(events.len(), 2);
                assert!(events.iter().any(|e| (e.a, e.b, e.region) == (1, 2, 1)));
                assert!(events.iter().any(|e| (e.a, e.b, e.region) == (3, 4, 1)));
                let cases = &state
                    .dynamics
                    .as_ref()
                    .unwrap()
                    .disease
                    .as_ref()
                    .unwrap()
                    .cases;
                assert!(cases.iter().any(|c| c.person == 2 && c.source == Some(1)));
                assert!(!cases.iter().any(|c| c.person == 3 || c.person == 4));
                Economy::compile(state.clone()).unwrap();
                state
            })
    };
    assert_eq!(run(1), run(4));
    initial.people[0].region = None;
    initial.people[1].region = None;
    initial.people[2].region = None;
    initial.travel.as_mut().unwrap().journeys = vec![
        Journey {
            person: 1,
            source: 1,
            destination: 2,
            remaining: 3,
        },
        Journey {
            person: 2,
            source: 1,
            destination: 2,
            remaining: 3,
        },
        Journey {
            person: 3,
            source: 1,
            destination: 2,
            remaining: 2,
        },
    ];
    let mut world = Economy::compile(initial).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    let events = &state
        .dynamics
        .as_ref()
        .unwrap()
        .social
        .as_ref()
        .unwrap()
        .events;
    assert_eq!(events.len(), 1);
    assert_eq!((events[0].a, events[0].b), (1, 2));
}

#[test]
fn fear_can_motivate_an_autonomous_encounter_during_travel() {
    use game_parallel_social::PressurePolicy;
    let mut initial = expedition_fixture();
    initial.travel.as_mut().unwrap().routes[0].ticks = 3;
    initial
        .dynamics
        .as_mut()
        .unwrap()
        .social
        .as_mut()
        .unwrap()
        .pressures = vec![PressurePolicy {
        person: 2,
        fear_at_least: Some(3),
        obligation_at_least: None,
        enabled: true,
    }];
    initial.opinions.as_mut().unwrap().records = vec![Opinion {
        observer: 2,
        target: 1,
        trust: 0,
        fear: 5,
        obligation: 0,
        affinity: 0,
    }];
    let mut world = Economy::compile(initial).unwrap();
    world
        .depart(&[
            Departure {
                person: 1,
                destination: 2,
            },
            Departure {
                person: 2,
                destination: 2,
            },
        ])
        .unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    let events = &state
        .dynamics
        .as_ref()
        .unwrap()
        .social
        .as_ref()
        .unwrap()
        .events;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].pressured, Some(2));
}

#[test]
fn purpose_deprivation_death_drives_real_succession() {
    use game_parallel_purpose::{Person as PurposePerson, Rules as PurposeRules};
    let mut initial = campaign_fixture();
    initial.pending_food.clear();
    initial.people[2].health = 1;
    initial.purpose = Some(Purpose {
        rules: PurposeRules {
            max: 10,
            per_tick: 1,
            blocked_increase: 1,
            completion_relief: 1,
            damage_threshold: 5,
            damage: 1,
        },
        people: vec![PurposePerson {
            person: 3,
            deprivation: 4,
            eligible_at: 0,
        }],
    });
    let mut world = Economy::compile(initial).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    assert_eq!(state.people[2].health, 0);
    assert_eq!(state.campaign.as_ref().unwrap().lineage.current, None);
    assert!(
        !state
            .family
            .as_ref()
            .unwrap()
            .members
            .iter()
            .find(|m| m.id == 3)
            .unwrap()
            .alive
    );
    Economy::compile(state).unwrap();
}

fn infection_from(person: u64) -> Dynamics {
    use game_parallel_disease::{Case, Disease, Strain};
    Dynamics {
        personality: None,
        social: None,
        disease: Some(Disease {
            strains: vec![Strain {
                id: 1,
                name: "fever".into(),
                incubation: 0,
                infectious_ticks: 10,
                immunity_ticks: 0,
                damage_per_tick: 0,
                transmission_per_million: 1_000_000,
                combat_transmission_per_million: 0,
            }],
            treatments: vec![],
            cases: vec![Case {
                person,
                strain: 1,
                infected_at: 0,
                source: None,
            }],
            events: vec![],
        }),
        treatment: vec![],
        claims: None,
    }
}

#[test]
fn actual_personal_encounters_transmit_disease_and_survive_reload() {
    use game_parallel_disease::{Case, Disease, Strain};
    use game_parallel_social::{Participant, Rules as SocialRules, Social};
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    initial.opinions = None;
    for person in &mut initial.people {
        person.seek_meals = false;
        person.willing_to_work = false;
    }
    initial.dynamics = Some(Dynamics {
        personality: None,
        treatment: vec![],
        claims: None,
        social: Some(Social {
            rules: SocialRules {
                interval: 1,
                min_health: 1,
                max_hunger: 100,
                familiarity_gain: 1,
                familiarity_max: 10,
                relationship_retention: None,
            },
            participants: vec![
                Participant {
                    person: 1,
                    willing: true,
                },
                Participant {
                    person: 2,
                    willing: true,
                },
            ],
            pressures: vec![],
            relationships: vec![],
            events: vec![],
            needs: None,
        }),
        disease: Some(Disease {
            strains: vec![Strain {
                id: 1,
                name: "fever".into(),
                incubation: 0,
                infectious_ticks: 3,
                immunity_ticks: 1,
                damage_per_tick: 4,
                transmission_per_million: 1_000_000,
                combat_transmission_per_million: 0,
            }],
            treatments: vec![],
            cases: vec![Case {
                person: 1,
                strain: 1,
                infected_at: 0,
                source: None,
            }],
            events: vec![],
        }),
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut economy = Economy::compile(initial.clone()).unwrap();
                economy.advance().unwrap();
                let saved = economy.snapshot();
                let dynamics = saved.dynamics.as_ref().unwrap();
                assert_eq!(dynamics.social.as_ref().unwrap().events.len(), 1);
                assert!(
                    dynamics
                        .disease
                        .as_ref()
                        .unwrap()
                        .cases
                        .iter()
                        .any(|case| case.person == 2 && case.source == Some(1))
                );
                assert_eq!(saved.people[0].health, 96);
                assert_eq!(saved.people[1].health, 100);
                let json = serde_json::to_string(&saved).unwrap();
                let mut resumed = Economy::compile(serde_json::from_str(&json).unwrap()).unwrap();
                economy.advance().unwrap();
                resumed.advance().unwrap();
                assert_eq!(economy.snapshot(), resumed.snapshot());
                resumed.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
    let mut blocked = initial;
    blocked.people[1].controlled = true;
    let mut economy = Economy::compile(blocked).unwrap();
    economy.advance().unwrap();
    assert_eq!(
        economy
            .snapshot()
            .dynamics
            .unwrap()
            .disease
            .unwrap()
            .cases
            .len(),
        1
    );
}

#[test]
fn personal_fear_or_obligation_can_cause_an_unwilling_encounter() {
    use game_parallel_social::{Participant, PressurePolicy, Rules as SocialRules, Social};
    let mut initial = fixture();
    initial.autowork.as_mut().unwrap().policies.clear();
    initial.agreements = None;
    for person in &mut initial.people {
        person.seek_meals = false;
        person.willing_to_work = false;
    }
    initial.opinions.as_mut().unwrap().rules.fear_decay_per_tick = 2;
    initial.opinions.as_mut().unwrap().rules.encounter_gain = 2;
    initial.dynamics = Some(Dynamics {
        personality: Some(game_parallel_economy_dynamics::personality::Personality {
            social_urgency_per_isolation: 0,
            inherit_births: false,
            people: vec![
                game_parallel_economy_dynamics::personality::Person {
                    person: 1,
                    sociability: 1000,
                    diligence: 1000,
                },
                game_parallel_economy_dynamics::personality::Person {
                    person: 4,
                    sociability: 0,
                    diligence: 1000,
                },
            ],
        }),
        social: Some(Social {
            rules: SocialRules {
                interval: 1,
                min_health: 1,
                max_hunger: 100,
                familiarity_gain: 1,
                familiarity_max: 10,
                relationship_retention: None,
            },
            participants: vec![
                Participant {
                    person: 1,
                    willing: true,
                },
                Participant {
                    person: 4,
                    willing: true,
                },
            ],
            pressures: vec![PressurePolicy {
                person: 4,
                fear_at_least: Some(3),
                obligation_at_least: Some(3),
                enabled: true,
            }],
            relationships: vec![],
            events: vec![],
            needs: None,
        }),
        disease: None,
        treatment: vec![],
        claims: None,
    });
    let run = |workers, state: State| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(state).unwrap();
                world.advance().unwrap();
                let first = world.snapshot();
                let events = &first
                    .dynamics
                    .as_ref()
                    .unwrap()
                    .social
                    .as_ref()
                    .unwrap()
                    .events;
                assert_eq!(events.len(), 1);
                assert_eq!(events[0].pressured, Some(4));
                let opinions = first.opinions.as_ref().unwrap();
                assert_eq!(
                    opinions
                        .records
                        .iter()
                        .find(|o| o.observer == 4 && o.target == 1)
                        .unwrap()
                        .trust,
                    0
                );
                assert_eq!(
                    opinions
                        .records
                        .iter()
                        .find(|o| o.observer == 1 && o.target == 4)
                        .unwrap()
                        .trust,
                    2
                );
                first
            })
    };
    assert_eq!(run(1, initial.clone()), run(4, initial.clone()));
    let mut resisting = Economy::compile(initial.clone()).unwrap();
    resisting.set_social_pressure(4, false).unwrap();
    resisting.advance().unwrap();
    assert!(
        resisting
            .snapshot()
            .dynamics
            .unwrap()
            .social
            .unwrap()
            .events
            .is_empty()
    );
    let mut world = Economy::compile(initial.clone()).unwrap();
    world.advance().unwrap();
    world.advance().unwrap();
    world.advance().unwrap();
    assert!(
        world
            .snapshot()
            .dynamics
            .unwrap()
            .social
            .unwrap()
            .events
            .is_empty()
    );
    let opinion = &mut initial.opinions.as_mut().unwrap().records[0];
    opinion.fear = 0;
    opinion.obligation = 3;
    run(4, initial);
}

#[test]
fn hungry_person_can_ask_a_met_acquaintance_who_remains_free_to_refuse() {
    use game_parallel_social::{Participant, Rules as SocialRules, Social};
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    for person in &mut initial.people {
        person.seek_meals = false;
        person.hunger = 0;
    }
    for (id, quantity) in [(1, 0), (2, 2)] {
        let holder = initial
            .physical
            .custody
            .containers
            .iter_mut()
            .find(|row| row.place == Place::Person(id))
            .unwrap();
        *holder = Container::new(
            holder.place,
            holder.capacity,
            if quantity == 0 {
                vec![]
            } else {
                vec![Lot {
                    good: 1,
                    quantity,
                    expires_at: Some(10),
                }]
            },
        )
        .unwrap();
    }
    initial.dynamics = Some(Dynamics {
        personality: None,
        disease: None,
        treatment: vec![],
        claims: None,
        social: Some(Social {
            rules: SocialRules {
                interval: 1,
                min_health: 1,
                max_hunger: 100,
                familiarity_gain: 1,
                familiarity_max: 10,
                relationship_retention: None,
            },
            participants: vec![
                Participant {
                    person: 1,
                    willing: true,
                },
                Participant {
                    person: 2,
                    willing: true,
                },
            ],
            pressures: vec![],
            relationships: vec![],
            events: vec![],
            needs: None,
        }),
    });
    initial.giving = Some(Giving {
        practices: None,
        inherit_births: false,
        policies: vec![],
        learning: vec![],
        asking: vec![],
        requests: vec![],
        responses: vec![FoodResponse {
            person: 2,
            good: 1,
            quantity: 1,
            recipient_hunger_at_least: 0,
            willing: false,
            minimum_trust: None,
            affinity_threshold: None,
            affinity_at_most: None,
            affinity_weight: None,
            pressure_threshold: None,
        }],
        acquaintance_asking: vec![AcquaintanceAsking {
            person: 1,
            enabled: true,
            min_hunger: 5,
            retry_ticks: 3,
            request_lifetime: 4,
            min_familiarity: 1,
            min_affinity: None,
            prefer_affinity: false,
            max_affinity: None,
            affinity_weight: None,
            avoid_last: None,
            next_review: 0,
            last_target: None,
        }],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let mut state = world.snapshot();
                assert!(state.giving.as_ref().unwrap().requests.is_empty());
                assert_eq!(
                    state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .social
                        .as_ref()
                        .unwrap()
                        .relationships
                        .len(),
                    1
                );
                state.people[0].hunger = 5;
                let mut overflow = state.clone();
                overflow.giving.as_mut().unwrap().acquaintance_asking[0].retry_ticks = u64::MAX;
                let mut rejected = Economy::compile(overflow).unwrap();
                let before = rejected.snapshot();
                assert!(rejected.advance().is_err());
                assert_eq!(before, rejected.snapshot());
                let mut world = Economy::compile(state).unwrap();
                world.advance().unwrap();
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored = Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                let snapshot = world.snapshot();
                let request = &snapshot.giving.as_ref().unwrap().requests[0];
                assert_eq!((request.requester, request.target), (1, 2));
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                let giver = state
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|row| row.place == Place::Person(2))
                    .unwrap();
                assert_eq!(giver.lots.iter().map(|lot| lot.quantity).sum::<u64>(), 2);
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn acquaintance_requests_follow_personal_affinity_and_try_other_known_people() {
    use game_parallel_social::{Participant, Relationship, Rules as SocialRules, Social};
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    for person in &mut initial.people {
        person.seek_meals = false;
        person.hunger = 0;
    }
    initial.people[0].hunger = 5;
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(holder.place, holder.capacity, vec![]).unwrap();
    let opinions = initial.opinions.as_mut().unwrap();
    opinions.rules.affinity = Some(game_parallel_opinions::AffinityRules {
        limit: 20,
        encounter_change: 0,
        pressured_encounter_change: 0,
        help_change: 0,
        harm_change_per_damage: 0,
    });
    opinions.records.extend([
        Opinion {
            observer: 1,
            target: 2,
            trust: 0,
            fear: 0,
            obligation: 0,
            affinity: -3,
        },
        Opinion {
            observer: 1,
            target: 3,
            trust: 0,
            fear: 0,
            obligation: 0,
            affinity: 4,
        },
    ]);
    initial.dynamics = Some(Dynamics {
        personality: None,
        disease: None,
        treatment: vec![],
        claims: None,
        social: Some(Social {
            rules: SocialRules {
                interval: 10,
                min_health: 1,
                max_hunger: 100,
                familiarity_gain: 1,
                familiarity_max: 10,
                relationship_retention: None,
            },
            participants: (1..=3)
                .map(|person| Participant {
                    person,
                    willing: true,
                })
                .collect(),
            pressures: vec![],
            relationships: [(1, 2), (1, 3)]
                .into_iter()
                .map(|(a, b)| Relationship {
                    a,
                    b,
                    familiarity: 1,
                    encounters: 1,
                    last_met: 0,
                })
                .collect(),
            events: vec![],
            needs: None,
        }),
    });
    initial.giving = Some(Giving {
        practices: None,
        inherit_births: false,
        policies: vec![],
        responses: vec![],
        learning: vec![],
        asking: vec![],
        requests: vec![],
        acquaintance_asking: vec![AcquaintanceAsking {
            person: 1,
            enabled: true,
            min_hunger: 5,
            retry_ticks: 1,
            request_lifetime: 1,
            min_familiarity: 1,
            min_affinity: None,
            prefer_affinity: true,
            max_affinity: None,
            affinity_weight: None,
            avoid_last: None,
            next_review: 0,
            last_target: None,
        }],
    });
    let run = |workers, minimum| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = initial.clone();
                state.giving.as_mut().unwrap().acquaintance_asking[0].min_affinity = minimum;
                let mut world = Economy::compile(state).unwrap();
                world.advance().unwrap();
                assert_eq!(
                    world.snapshot().giving.as_ref().unwrap().requests[0].target,
                    3
                );
                world.advance().unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state.giving.as_ref().unwrap().requests[0].target,
                    if minimum.is_some() { 3 } else { 2 }
                );
                state
            })
    };
    assert_eq!(run(1, None), run(4, None));
    assert_eq!(run(1, Some(0)), run(4, Some(0)));
    let mut subjective = initial.clone();
    let preference = &mut subjective.giving.as_mut().unwrap().acquaintance_asking[0];
    preference.affinity_weight = Some(-1);
    preference.avoid_last = Some(false);
    let select = |workers, state: &State| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(state.clone()).unwrap();
                world.advance().unwrap();
                assert_eq!(
                    world.snapshot().giving.as_ref().unwrap().requests[0].target,
                    2
                );
                let saved = world.snapshot();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(state.giving.as_ref().unwrap().requests[0].target, 2);
                state
            })
    };
    assert_eq!(select(1, &subjective), select(4, &subjective));
    let preference = &mut subjective.giving.as_mut().unwrap().acquaintance_asking[0];
    preference.affinity_weight = Some(1);
    preference.max_affinity = Some(-1);
    assert_eq!(select(4, &subjective).giving.unwrap().requests[0].target, 2);
    subjective.giving.as_mut().unwrap().acquaintance_asking[0].min_affinity = Some(0);
    assert!(Economy::compile(subjective).is_err());
}

#[test]
fn isolation_changes_personal_social_choices_without_compelling_contact() {
    use game_parallel_economy_dynamics::personality::{Person as Disposition, Personality};
    use game_parallel_social::{NeedRules, Needs, Participant, Rules as SocialRules, Social};
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    initial.opinions = None;
    initial.nutrition = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.dynamics = Some(Dynamics {
        personality: Some(Personality {
            social_urgency_per_isolation: 1000,
            inherit_births: true,
            people: vec![
                Disposition {
                    person: 1,
                    sociability: 0,
                    diligence: 500,
                },
                Disposition {
                    person: 2,
                    sociability: 1000,
                    diligence: 500,
                },
            ],
        }),
        social: Some(Social {
            rules: SocialRules {
                interval: 1,
                min_health: 1,
                max_hunger: 100,
                familiarity_gain: 1,
                familiarity_max: 10,
                relationship_retention: None,
            },
            participants: vec![
                Participant {
                    person: 1,
                    willing: true,
                },
                Participant {
                    person: 2,
                    willing: true,
                },
            ],
            pressures: vec![],
            relationships: vec![],
            events: vec![],
            needs: Some(Needs {
                rules: NeedRules {
                    capacity: 10,
                    isolation_gain: 1,
                    encounter_relief: 3,
                },
                people: vec![],
            }),
        }),
        disease: None,
        treatment: vec![],
        claims: None,
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let first = world.snapshot();
                assert!(
                    first
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .social
                        .as_ref()
                        .unwrap()
                        .events
                        .is_empty()
                );
                assert_eq!(
                    first
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .social
                        .as_ref()
                        .unwrap()
                        .needs
                        .as_ref()
                        .unwrap()
                        .people[0]
                        .isolation,
                    1
                );
                let mut world = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&first).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                let second = world.snapshot();
                let social = second.dynamics.as_ref().unwrap().social.as_ref().unwrap();
                assert_eq!(social.events.len(), 1);
                assert!(social.needs.as_ref().unwrap().people.is_empty());
                second
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn automatic_treatment_without_a_threshold_uses_visible_health_not_hidden_infection() {
    use game_parallel_disease::Treatment;
    use game_parallel_economy_dynamics::TreatmentPolicy;
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    initial.nutrition = None;
    for person in &mut initial.people {
        person.seek_meals = false;
        person.willing_to_work = false;
    }
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|holder| holder.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(
        Place::Person(1),
        10,
        vec![Lot {
            good: 2,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut dynamics = infection_from(1);
    dynamics
        .disease
        .as_mut()
        .unwrap()
        .treatments
        .push(Treatment {
            strain: 1,
            medicine: 2,
        });
    dynamics.treatment.push(TreatmentPolicy {
        person: 1,
        medicine: 2,
        quantity: 1,
        willing: true,
        health_at_most: None,
    });
    initial.dynamics = Some(dynamics);
    let run = |workers, sick: bool, health| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = initial.clone();
                state.people[0].health = health;
                if !sick {
                    state
                        .dynamics
                        .as_mut()
                        .unwrap()
                        .disease
                        .as_mut()
                        .unwrap()
                        .cases
                        .clear();
                }
                let mut world = Economy::compile(state).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let stock = state
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|holder| holder.place == Place::Person(1))
                    .unwrap()
                    .used;
                (stock, state)
            })
    };
    for sick in [false, true] {
        assert_eq!(run(1, sick, 80), run(4, sick, 80));
        assert_eq!(
            run(1, sick, 80).0,
            0,
            "poor health can prompt an ineffective course"
        );
        assert_eq!(
            run(1, sick, 100).0,
            1,
            "an unseen infection does not trigger treatment"
        );
    }

    let mut caregiver = initial.clone();
    caregiver.dynamics.as_mut().unwrap().treatment.clear();
    caregiver
        .dynamics
        .as_mut()
        .unwrap()
        .disease
        .as_mut()
        .unwrap()
        .cases[0]
        .person = 3;
    caregiver.medical = Some(Medical {
        acquaintances: vec![],
        policies: vec![MedicalPolicy {
            person: 1,
            patient: 3,
            medicine: 2,
            quantity: 1,
            willing: true,
            patient_health_at_most: None,
            pressured_by: None,
            pressure_threshold: None,
        }],
    });
    let run = |sick: bool, health| {
        let mut state = caregiver.clone();
        state.people[2].health = health;
        if !sick {
            state
                .dynamics
                .as_mut()
                .unwrap()
                .disease
                .as_mut()
                .unwrap()
                .cases
                .clear();
        }
        let mut world = Economy::compile(state).unwrap();
        world.advance().unwrap();
        world
            .snapshot()
            .physical
            .custody
            .containers
            .iter()
            .find(|holder| holder.place == Place::Person(1))
            .unwrap()
            .used
    };
    for sick in [false, true] {
        assert_eq!(run(sick, 80), 0);
        assert_eq!(run(sick, 100), 1);
    }
}

#[test]
fn self_treatment_requires_personal_choice_and_actual_unexpired_medicine() {
    use game_parallel_disease::{Case, Disease, EventKind, Strain, Treatment};
    use game_parallel_economy_dynamics::TreatmentPolicy;
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    initial.opinions = None;
    initial.people[0].seek_meals = false;
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(
        Place::Person(1),
        10,
        vec![Lot {
            good: 2,
            quantity: 2,
            expires_at: Some(3),
        }],
    )
    .unwrap();
    initial.dynamics = Some(Dynamics {
        personality: None,
        social: None,
        disease: Some(Disease {
            strains: vec![Strain {
                id: 1,
                name: "fever".into(),
                incubation: 0,
                infectious_ticks: 10,
                immunity_ticks: 0,
                damage_per_tick: 1,
                transmission_per_million: 0,
                combat_transmission_per_million: 0,
            }],
            treatments: vec![Treatment {
                strain: 1,
                medicine: 2,
            }],
            cases: vec![Case {
                person: 1,
                strain: 1,
                infected_at: 0,
                source: None,
            }],
            events: vec![],
        }),
        treatment: vec![TreatmentPolicy {
            person: 1,
            medicine: 2,
            quantity: 2,
            willing: false,
            health_at_most: None,
        }],
        claims: None,
    });
    let mut world = Economy::compile(initial.clone()).unwrap();
    world.advance().unwrap();
    assert_eq!(
        world
            .snapshot()
            .dynamics
            .as_ref()
            .unwrap()
            .disease
            .as_ref()
            .unwrap()
            .cases
            .len(),
        1
    );
    world.set_treatment_choice(1, true).unwrap();
    world.advance().unwrap();
    let saved = world.snapshot();
    let disease = saved.dynamics.as_ref().unwrap().disease.as_ref().unwrap();
    assert!(disease.cases.is_empty());
    assert!(
        disease
            .events
            .iter()
            .any(|event| event.kind == EventKind::Treated && event.person == 1)
    );
    assert_eq!(
        saved
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(1))
            .unwrap()
            .used,
        0
    );
    Economy::compile(saved).unwrap();
    let mut expired = Economy::compile(initial).unwrap();
    expired.advance().unwrap();
    expired.advance().unwrap();
    expired.set_treatment_choice(1, true).unwrap();
    expired.advance().unwrap();
    assert_eq!(
        expired
            .snapshot()
            .dynamics
            .unwrap()
            .disease
            .unwrap()
            .cases
            .len(),
        1
    );
}

#[test]
fn self_treatment_can_be_wrong_without_revealing_hidden_infection() {
    use game_parallel_disease::Treatment;
    use game_parallel_economy_dynamics::TreatmentPolicy;
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    initial.people[0].seek_meals = false;
    initial.people[0].health = 70;
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(
        holder.place,
        10,
        vec![Lot {
            good: 2,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut dynamics = infection_from(1);
    let disease = dynamics.disease.as_mut().unwrap();
    let mut other = disease.strains[0].clone();
    other.id = 2;
    other.name = "other illness".into();
    disease.strains.push(other);
    disease.cases[0].strain = 2;
    disease.treatments.push(Treatment {
        strain: 1,
        medicine: 2,
    });
    dynamics.treatment.push(TreatmentPolicy {
        person: 1,
        medicine: 2,
        quantity: 1,
        willing: true,
        health_at_most: Some(80),
    });
    initial.dynamics = Some(dynamics);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&world.snapshot()).unwrap())
                        .unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(1))
                        .unwrap()
                        .used,
                    0
                );
                assert_eq!(
                    state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .disease
                        .as_ref()
                        .unwrap()
                        .cases[0]
                        .strain,
                    2
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
    initial.people[0].controlled = true;
    initial
        .dynamics
        .as_mut()
        .unwrap()
        .disease
        .as_mut()
        .unwrap()
        .cases
        .clear();
    let mut player = Economy::compile(initial).unwrap();
    assert!(player.treat(1).unwrap());
    let state = player.snapshot();
    assert_eq!(
        state
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(1))
            .unwrap()
            .used,
        0
    );
    assert!(state.dynamics.unwrap().disease.unwrap().cases.is_empty());
}

#[test]
fn controlled_treatment_consumes_only_unexpired_medicine() {
    use game_parallel_disease::Treatment;
    use game_parallel_economy_dynamics::TreatmentPolicy;
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    initial.people[0].controlled = true;
    initial.people[0].seek_meals = false;
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(
        Place::Person(1),
        10,
        vec![Lot {
            good: 2,
            quantity: 2,
            expires_at: Some(3),
        }],
    )
    .unwrap();
    let mut dynamics = infection_from(1);
    dynamics
        .disease
        .as_mut()
        .unwrap()
        .treatments
        .push(Treatment {
            strain: 1,
            medicine: 2,
        });
    dynamics.treatment.push(TreatmentPolicy {
        person: 1,
        medicine: 2,
        quantity: 2,
        willing: false,
        health_at_most: None,
    });
    initial.dynamics = Some(dynamics);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.treat(1).unwrap());
                let state = world.snapshot();
                assert!(
                    state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .disease
                        .as_ref()
                        .unwrap()
                        .cases
                        .is_empty()
                );
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(1))
                        .unwrap()
                        .used,
                    0
                );
                assert_eq!(
                    state,
                    Economy::compile(
                        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap()
                    )
                    .unwrap()
                    .snapshot()
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(
        Place::Person(1),
        10,
        vec![Lot {
            good: 2,
            quantity: 2,
            expires_at: Some(1),
        }],
    )
    .unwrap();
    let mut world = Economy::compile(initial).unwrap();
    assert!(!world.treat(1).unwrap());
    assert_eq!(world.tick(), 1);
    assert_eq!(
        world
            .snapshot()
            .dynamics
            .unwrap()
            .disease
            .unwrap()
            .cases
            .len(),
        1
    );
}

#[test]
fn a_caregiver_treats_with_real_medicine_and_can_catch_the_illness() {
    use game_parallel_disease::Treatment;
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    initial.nutrition = None;
    for p in &mut initial.people {
        p.seek_meals = false;
    }
    initial.people[0].controlled = true;
    initial.opinions.as_mut().unwrap().rules.gratitude_gain = 2;
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(
        holder.place,
        holder.capacity,
        vec![Lot {
            good: 2,
            quantity: 2,
            expires_at: Some(3),
        }],
    )
    .unwrap();
    let mut dynamics = infection_from(2);
    let disease = dynamics.disease.as_mut().unwrap();
    disease.treatments.push(Treatment {
        strain: 1,
        medicine: 2,
    });
    disease.strains[0].transmission_per_million = 1_000_000;
    initial.dynamics = Some(dynamics);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.treat_other(1, 2, 2, 2).unwrap());
                let saved = world.snapshot();
                let disease = saved.dynamics.as_ref().unwrap().disease.as_ref().unwrap();
                assert!(!disease.cases.iter().any(|c| c.person == 2));
                assert!(
                    disease
                        .cases
                        .iter()
                        .any(|c| c.person == 1 && c.source == Some(2))
                );
                assert_eq!(
                    saved
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(1))
                        .unwrap()
                        .used,
                    0
                );
                assert!(
                    saved
                        .opinions
                        .as_ref()
                        .unwrap()
                        .records
                        .iter()
                        .any(|o| o.observer == 2 && o.target == 1 && o.obligation == 2)
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
    let mut wrong = initial.clone();
    let holder = wrong
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(
        holder.place,
        holder.capacity,
        vec![Lot {
            good: 3,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let opinions = wrong.opinions.as_mut().unwrap();
    opinions.rules.affinity = Some(game_parallel_opinions::AffinityRules {
        limit: 10,
        encounter_change: 0,
        pressured_encounter_change: 0,
        help_change: 0,
        harm_change_per_damage: 0,
    });
    opinions.sensitivities = vec![OpinionSensitivity {
        person: 2,
        report_reception: None,
        affinity_report_discrepancy: None,
        trust_report_discrepancy: None,
        trust_promise_fulfilled: None,
        trust_promise_unfulfilled: None,
        fear_per_damage: None,
        gratitude_gain: Some(0),
        obligation_relief: None,
        theft_loss: None,
        confrontation: None,
        affinity_help: Some(-2),
        affinity_harm_per_damage: None,
        affinity_encounter: None,
        affinity_pressured_encounter: None,
        affinity_site_use: None,
        affinity_site_destruction: None,
        affinity_site_salvage: None,
        affinity_work_success: None,
        affinity_work_refusal: None,
        relationship_cases: vec![],
        obligation_development: None,
        promise_obligation_gain: None,
    }]
    .into();
    let mut world = Economy::compile(wrong).unwrap();
    assert!(world.treat_other(1, 2, 3, 1).unwrap());
    let state = world.snapshot();
    assert_eq!(
        state
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(1))
            .unwrap()
            .used,
        0
    );
    assert!(
        state
            .dynamics
            .unwrap()
            .disease
            .unwrap()
            .cases
            .iter()
            .any(|c| c.person == 2)
    );
    let opinions = state.opinions.as_ref().unwrap().index();
    assert_eq!(opinions.affinity(2, 1), -2);
    assert_eq!(opinions.obligation(2, 1), 0);
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(
        holder.place,
        holder.capacity,
        vec![Lot {
            good: 2,
            quantity: 2,
            expires_at: Some(1),
        }],
    )
    .unwrap();
    let mut world = Economy::compile(initial).unwrap();
    assert!(!world.treat_other(1, 2, 2, 2).unwrap());
    let state = world.snapshot();
    assert!(
        state
            .dynamics
            .unwrap()
            .disease
            .unwrap()
            .cases
            .iter()
            .any(|c| c.person == 2)
    );
    assert!(
        !state
            .opinions
            .unwrap()
            .records
            .iter()
            .any(|o| o.observer == 2 && o.target == 1)
    );
}

#[test]
fn fear_can_motivate_a_caregiver_and_their_preference_can_change() {
    use game_parallel_disease::Treatment;
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    for p in &mut initial.people {
        p.seek_meals = false;
    }
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(4))
        .unwrap();
    *holder = Container::new(
        holder.place,
        holder.capacity,
        vec![Lot {
            good: 2,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut dynamics = infection_from(3);
    dynamics
        .disease
        .as_mut()
        .unwrap()
        .treatments
        .push(Treatment {
            strain: 1,
            medicine: 2,
        });
    initial.dynamics = Some(dynamics);
    initial.medical = Some(Medical {
        acquaintances: vec![],
        policies: vec![MedicalPolicy {
            person: 4,
            patient: 3,
            medicine: 2,
            quantity: 1,
            willing: false,
            patient_health_at_most: Some(100),
            pressured_by: Some(1),
            pressure_threshold: Some(3),
        }],
    });
    initial.fatigue = Some(Fatigue {
        rules: fatigue_rules(),
        people: vec![game_parallel_fatigue::Person {
            id: 3,
            fatigue: 8,
            resting: true,
        }],
    });
    let mut world = Economy::compile(initial.clone()).unwrap();
    world.advance().unwrap();
    assert!(
        !world
            .snapshot()
            .dynamics
            .unwrap()
            .disease
            .unwrap()
            .cases
            .iter()
            .any(|c| c.person == 3)
    );
    assert!(
        world
            .snapshot()
            .fatigue
            .as_ref()
            .unwrap()
            .people
            .iter()
            .any(|p| p.id == 3 && p.fatigue == 5)
    );
    assert!(
        world
            .snapshot()
            .fatigue
            .as_ref()
            .unwrap()
            .people
            .iter()
            .any(|p| p.id == 4 && p.fatigue == 2)
    );
    initial.opinions.as_mut().unwrap().records[0].fear = 0;
    let mut world = Economy::compile(initial).unwrap();
    let stale = world.prepare().unwrap();
    world.set_medical_choice(4, true).unwrap();
    assert!(world.commit(stale).is_err());
    world.set_medical_choice(4, false).unwrap();
    world.advance().unwrap();
    assert!(
        world
            .snapshot()
            .dynamics
            .unwrap()
            .disease
            .unwrap()
            .cases
            .iter()
            .any(|c| c.person == 3)
    );
}

fn medical_workload(count: u64) -> State {
    use game_parallel_disease::Treatment;
    let mut state = field_workload(count);
    state.agriculture = None;
    for holder in &mut state.physical.custody.containers {
        if let Place::Person(id) = holder.place {
            let lots = if id == 1 {
                vec![]
            } else {
                vec![Lot {
                    good: 2,
                    quantity: 1,
                    expires_at: None,
                }]
            };
            *holder = Container::new(holder.place, holder.capacity, lots).unwrap();
        }
    }
    let mut dynamics = infection_from(1);
    dynamics
        .disease
        .as_mut()
        .unwrap()
        .treatments
        .push(Treatment {
            strain: 1,
            medicine: 2,
        });
    state.dynamics = Some(dynamics);
    state.medical = Some(Medical {
        acquaintances: vec![],
        policies: (2..=count)
            .map(|person| MedicalPolicy {
                person,
                patient: 1,
                medicine: 2,
                quantity: 1,
                willing: true,
                patient_health_at_most: Some(100),
                pressured_by: None,
                pressure_threshold: None,
            })
            .collect(),
    });
    state
}

#[test]
fn caregivers_choose_reachable_acquaintances_from_their_own_priorities_and_feelings() {
    use game_parallel_social::{Participant, Relationship, Rules as SocialRules, Social};
    let mut initial = medical_workload(4);
    initial.people[0].health = 60;
    initial.people[2].health = 70;
    initial.people[3].health = 10; // Unknown to the caregiver.
    let dynamics = initial.dynamics.as_mut().unwrap();
    let disease = dynamics.disease.as_mut().unwrap();
    disease.strains[0].transmission_per_million = 0;
    let mut other_case = disease.cases[0].clone();
    other_case.person = 3;
    disease.cases.push(other_case);
    dynamics.social = Some(Social {
        rules: SocialRules {
            interval: 1,
            min_health: 1,
            max_hunger: 100,
            familiarity_gain: 1,
            familiarity_max: 10,
            relationship_retention: None,
        },
        participants: (1..=4)
            .map(|person| Participant {
                person,
                willing: false,
            })
            .collect(),
        pressures: vec![],
        relationships: vec![
            Relationship {
                a: 1,
                b: 2,
                familiarity: 2,
                encounters: 1,
                last_met: 0,
            },
            Relationship {
                a: 2,
                b: 3,
                familiarity: 2,
                encounters: 1,
                last_met: 0,
            },
        ],
        events: vec![],
        needs: None,
    });
    let mut opinions = fixture().opinions.unwrap();
    opinions.records = vec![
        Opinion {
            observer: 2,
            target: 1,
            trust: 0,
            fear: 0,
            obligation: 0,
            affinity: -5,
        },
        Opinion {
            observer: 2,
            target: 3,
            trust: 0,
            fear: 3,
            obligation: 0,
            affinity: 8,
        },
    ];
    initial.opinions = Some(opinions);
    initial.medical = Some(Medical {
        policies: vec![],
        acquaintances: vec![MedicalAcquaintancePolicy {
            person: 2,
            medicine: 2,
            quantity: 1,
            patient_health_at_most: 80,
            willing: true,
            pressure_threshold: Some(3),
            urgency_weight: 1,
            affinity_weight: 3,
        }],
    });
    let mut missing_social = initial.clone();
    missing_social.dynamics.as_mut().unwrap().social = None;
    assert!(Economy::compile(missing_social).is_err());
    let run = |workers, state: &State| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(state.clone()).unwrap();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&world.snapshot()).unwrap())
                        .unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                state
            })
    };
    let preferred = run(1, &initial);
    assert_eq!(preferred, run(4, &initial));
    let remaining: Vec<_> = preferred
        .dynamics
        .as_ref()
        .unwrap()
        .disease
        .as_ref()
        .unwrap()
        .cases
        .iter()
        .map(|c| c.person)
        .collect();
    assert_eq!(remaining, [1]);
    initial.medical.as_mut().unwrap().acquaintances[0].affinity_weight = -3;
    let reversed = run(4, &initial);
    assert_eq!(
        reversed
            .dynamics
            .unwrap()
            .disease
            .unwrap()
            .cases
            .iter()
            .map(|c| c.person)
            .collect::<Vec<_>>(),
        [3]
    );
    initial.medical.as_mut().unwrap().acquaintances[0].willing = false;
    let pressured = run(4, &initial);
    assert_eq!(
        pressured
            .dynamics
            .unwrap()
            .disease
            .unwrap()
            .cases
            .iter()
            .map(|c| c.person)
            .collect::<Vec<_>>(),
        [1]
    );
    initial.opinions.as_mut().unwrap().records[1].fear = 0;
    let quiet = run(4, &initial);
    assert_eq!(quiet.dynamics.unwrap().disease.unwrap().cases.len(), 2);
    let mut world = Economy::compile(initial).unwrap();
    world.set_medical_choice(2, true).unwrap();
    world.advance().unwrap();
    assert_eq!(
        world
            .snapshot()
            .dynamics
            .unwrap()
            .disease
            .unwrap()
            .cases
            .len(),
        1
    );
}

#[test]
fn a_caregiver_can_treat_observed_poor_health_without_knowing_the_diagnosis() {
    let mut initial = medical_workload(4);
    initial.people[0].health = 70;
    initial
        .dynamics
        .as_mut()
        .unwrap()
        .disease
        .as_mut()
        .unwrap()
        .cases
        .clear();
    let policies = &mut initial.medical.as_mut().unwrap().policies;
    policies.truncate(1);
    policies[0].patient_health_at_most = Some(80);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&world.snapshot()).unwrap())
                        .unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                let medicine = state
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|c| c.place == Place::Person(2))
                    .unwrap();
                assert_eq!(medicine.used, 0);
                assert!(
                    state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .disease
                        .as_ref()
                        .unwrap()
                        .cases
                        .is_empty()
                );
                assert!(
                    !state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .disease
                        .as_ref()
                        .unwrap()
                        .events
                        .iter()
                        .any(|event| event.kind == game_parallel_disease::EventKind::Treated)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
    initial.medical.as_mut().unwrap().policies[0].patient_health_at_most = Some(60);
    let mut cautious = Economy::compile(initial).unwrap();
    cautious.advance().unwrap();
    assert_eq!(
        cautious
            .snapshot()
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(2))
            .unwrap()
            .used,
        1
    );
}

#[test]
fn concentrated_medical_care_selects_one_real_course_across_workers() {
    let initial = medical_workload(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let events = world.advance().unwrap();
                assert_eq!(
                    events
                        .physical_actions
                        .iter()
                        .filter(|d| d.accepted)
                        .count(),
                    1
                );
                let state = world.snapshot();
                assert!(
                    !state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .disease
                        .as_ref()
                        .unwrap()
                        .cases
                        .iter()
                        .any(|c| c.person == 1)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn a_controlled_patient_can_rest_during_care_but_not_give_at_the_same_time() {
    let mut initial = medical_workload(4);
    initial.people[0].controlled = true;
    initial.fatigue = Some(Fatigue {
        rules: fatigue_rules(),
        people: vec![],
    });
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(
        holder.place,
        holder.capacity,
        vec![Lot {
            good: 3,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut resting = Economy::compile(initial.clone()).unwrap();
    resting.rest(1).unwrap();
    assert!(
        !resting
            .snapshot()
            .dynamics
            .unwrap()
            .disease
            .unwrap()
            .cases
            .iter()
            .any(|case| case.person == 1)
    );
    let mut giving = Economy::compile(initial).unwrap();
    assert!(giving.give(1, 2, 3, 1).unwrap());
    assert!(
        giving
            .snapshot()
            .dynamics
            .unwrap()
            .disease
            .unwrap()
            .cases
            .iter()
            .any(|case| case.person == 1)
    );
}

#[test]
fn one_person_cannot_treat_and_receive_treatment_in_the_same_boundary() {
    let mut initial = medical_workload(4);
    initial
        .dynamics
        .as_mut()
        .unwrap()
        .disease
        .as_mut()
        .unwrap()
        .strains[0]
        .transmission_per_million = 0;
    initial
        .dynamics
        .as_mut()
        .unwrap()
        .disease
        .as_mut()
        .unwrap()
        .cases
        .push(game_parallel_disease::Case {
            person: 3,
            strain: 1,
            infected_at: 0,
            source: None,
        });
    initial.medical.as_mut().unwrap().policies = vec![
        MedicalPolicy {
            person: 2,
            patient: 3,
            medicine: 2,
            quantity: 1,
            willing: true,
            patient_health_at_most: Some(100),
            pressured_by: None,
            pressure_threshold: None,
        },
        MedicalPolicy {
            person: 3,
            patient: 1,
            medicine: 2,
            quantity: 1,
            willing: true,
            patient_health_at_most: Some(100),
            pressured_by: None,
            pressure_threshold: None,
        },
    ];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let events = world.advance().unwrap();
                assert_eq!(
                    events
                        .physical_actions
                        .iter()
                        .filter(|d| d.accepted)
                        .count(),
                    1
                );
                let state = world.snapshot();
                assert_eq!(
                    state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .disease
                        .as_ref()
                        .unwrap()
                        .cases
                        .len(),
                    1
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_medical_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = medical_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

fn distributed_medical_workload(count: u64) -> State {
    let mut state = medical_workload(count);
    let disease = state.dynamics.as_mut().unwrap().disease.as_mut().unwrap();
    disease.strains[0].transmission_per_million = 0;
    disease.cases = (1..=count / 2)
        .map(|person| game_parallel_disease::Case {
            person,
            strain: 1,
            infected_at: 0,
            source: None,
        })
        .collect();
    state.medical.as_mut().unwrap().policies = (count / 2 + 1..=count)
        .map(|person| MedicalPolicy {
            person,
            patient: person - count / 2,
            medicine: 2,
            quantity: 1,
            willing: true,
            patient_health_at_most: Some(100),
            pressured_by: None,
            pressure_threshold: None,
        })
        .collect();
    state
}

fn acquainted_medical_workload(count: u64) -> State {
    use game_parallel_social::{Participant, Relationship, Rules as SocialRules, Social};
    let mut state = distributed_medical_workload(count);
    let medical = state.medical.as_mut().unwrap();
    medical.policies.clear();
    medical.acquaintances = (count / 2 + 1..=count)
        .map(|person| MedicalAcquaintancePolicy {
            person,
            medicine: 2,
            quantity: 1,
            patient_health_at_most: 100,
            willing: true,
            pressure_threshold: None,
            urgency_weight: 1,
            affinity_weight: 0,
        })
        .collect();
    state.dynamics.as_mut().unwrap().social = Some(Social {
        rules: SocialRules {
            interval: 1,
            min_health: 1,
            max_hunger: 100,
            familiarity_gain: 1,
            familiarity_max: 10,
            relationship_retention: None,
        },
        participants: (1..=count)
            .map(|person| Participant {
                person,
                willing: false,
            })
            .collect(),
        pressures: vec![],
        relationships: (1..=count / 2)
            .map(|person| Relationship {
                a: person,
                b: person + count / 2,
                familiarity: 1,
                encounters: 1,
                last_met: 0,
            })
            .collect(),
        events: vec![],
        needs: None,
    });
    state
}

#[test]
fn independent_acquaintance_care_matches_across_workers() {
    let initial = acquainted_medical_workload(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert!(
                    state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .disease
                        .as_ref()
                        .unwrap()
                        .cases
                        .is_empty()
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn acquainted_caregivers_competing_for_one_patient_resolve_once_across_workers() {
    use game_parallel_social::Relationship;
    let count = 4096;
    let mut initial = acquainted_medical_workload(count);
    let dynamics = initial.dynamics.as_mut().unwrap();
    dynamics
        .disease
        .as_mut()
        .unwrap()
        .cases
        .retain(|case| case.person == 1);
    dynamics.social.as_mut().unwrap().relationships = (count / 2 + 1..=count)
        .map(|person| Relationship {
            a: 1,
            b: person,
            familiarity: 1,
            encounters: 1,
            last_met: 0,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let events = world.advance().unwrap();
                assert_eq!(
                    events
                        .physical_actions
                        .iter()
                        .filter(|event| event.accepted)
                        .count(),
                    1
                );
                let state = world.snapshot();
                assert!(
                    state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .disease
                        .as_ref()
                        .unwrap()
                        .cases
                        .is_empty()
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore = "performance measurement"]
fn acquainted_medical_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = acquainted_medical_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
            #[cfg(feature = "profile")]
            for (name, measurement) in game_diagnostics::snapshot() {
                println!(
                    "{name}: {:.3} ms/call",
                    measurement.nanoseconds as f64 / measurement.calls as f64 / 1_000_000.0
                );
            }
        });
}

#[test]
fn distributed_medical_treatments_match_across_workers() {
    let count = 4096;
    let initial = distributed_medical_workload(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert!(
                    state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .disease
                        .as_ref()
                        .unwrap()
                        .cases
                        .is_empty()
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn distributed_medical_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = distributed_medical_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn conflicting_claims_are_personal_and_observed_only_after_actual_use() {
    use game_parallel_economy_claims::Claims;
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    initial.people[0].seek_meals = false;
    initial.people[1].seek_meals = true;
    initial.people[1].hunger = 5;
    initial.opinions.as_mut().unwrap().rules.theft_loss = 2;
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        Place::Stock { region: 1, good: 1 },
        10,
        vec![Lot {
            good: 1,
            quantity: 2,
            expires_at: Some(8),
        }],
    )
    .unwrap();
    initial.dynamics = Some(Dynamics {
        personality: None,
        social: None,
        disease: None,
        treatment: vec![],
        claims: Some(Claims::default()),
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.claim_stock(1, 1, 1).unwrap();
                world.claim_stock(4, 1, 1).unwrap();
                assert!(world.claim_stock(1, 1, 1).is_err());
                world.advance().unwrap();
                let before = world.snapshot();
                assert!(
                    before
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .claims
                        .as_ref()
                        .unwrap()
                        .events
                        .is_empty()
                );
                assert_eq!(
                    before
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap()
                        .used,
                    0
                );
                let stale = world.prepare().unwrap();
                world.set_claim_choice(2, true).unwrap();
                assert!(world.commit(stale).is_err());
                world.advance().unwrap();
                let state = world.snapshot();
                let events = &state
                    .dynamics
                    .as_ref()
                    .unwrap()
                    .claims
                    .as_ref()
                    .unwrap()
                    .events;
                assert_eq!(events.len(), 2);
                assert!(
                    events
                        .iter()
                        .all(|e| e.person == 2 && e.witnessed && e.quantity == 1)
                );
                let holder = state
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|c| c.place == Place::Person(2))
                    .unwrap();
                assert_eq!((holder.used, holder.lots[0].expires_at), (1, Some(8)));
                let opinions = state.opinions.as_ref().unwrap();
                let observers: Vec<_> = opinions
                    .records
                    .iter()
                    .filter(|o| o.target == 2 && (o.observer == 1 || o.observer == 4))
                    .collect();
                assert_eq!(observers.len(), 2);
                assert!(observers.iter().all(|o| o.trust == -2));
                Economy::compile(state.clone()).unwrap();
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn hunger_can_change_one_persons_claim_choice_after_open_food_is_considered() {
    use game_parallel_economy_claims::{Claim, Claims, Preference};
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    initial.nutrition = None;
    for p in &mut initial.people {
        p.seek_meals = false;
    }
    initial.people[1].seek_meals = true;
    initial.people[1].hunger = 5;
    initial.opinions.as_mut().unwrap().rules.theft_loss = 2;
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![Lot {
            good: 1,
            quantity: 2,
            expires_at: Some(8),
        }],
    )
    .unwrap();
    initial.dynamics = Some(Dynamics {
        personality: None,
        social: None,
        disease: None,
        treatment: vec![],
        claims: Some(Claims {
            inherit_births: false,
            site_claims: vec![],
            site_events: vec![],
            claims: vec![Claim {
                person: 1,
                region: 1,
                good: 1,
            }],
            preferences: vec![Preference {
                person: 2,
                disregard_others: false,
                assert_outputs: false,
                assert_built_sites: false,
                desperate_hunger: Some(7),
            }],
            events: vec![],
        }),
    });
    let mut alternative = initial.clone();
    alternative.people[1].hunger = 7;
    let mut food = alternative.physical.goods[0].clone();
    food.id = 4;
    food.name = "other meal".into();
    alternative.physical.goods.push(food);
    alternative.physical.custody.containers.push(
        Container::new(
            Place::Stock { region: 1, good: 4 },
            10,
            vec![Lot {
                good: 4,
                quantity: 1,
                expires_at: Some(8),
            }],
        )
        .unwrap(),
    );
    alternative
        .physical
        .custody
        .containers
        .sort_unstable_by_key(|c| c.place);
    let mut world = Economy::compile(alternative).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    assert_eq!(
        state
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(2))
            .unwrap()
            .lots[0]
            .good,
        4
    );
    assert!(state.dynamics.unwrap().claims.unwrap().events.is_empty());
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                for _ in 0..2 {
                    world.advance().unwrap();
                }
                assert!(
                    world
                        .snapshot()
                        .dynamics
                        .unwrap()
                        .claims
                        .unwrap()
                        .events
                        .is_empty()
                );
                let saved = world.snapshot();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                let state = world.snapshot();
                let events = &state
                    .dynamics
                    .as_ref()
                    .unwrap()
                    .claims
                    .as_ref()
                    .unwrap()
                    .events;
                assert_eq!(events.len(), 1);
                assert_eq!(
                    (events[0].person, events[0].claimant, events[0].good),
                    (2, 1, 1)
                );
                assert!(events[0].witnessed);
                state
            })
    };
    assert_eq!(run(1), run(4));
    let mut respectful = Economy::compile(initial).unwrap();
    let stale = respectful.prepare().unwrap();
    respectful.set_desperate_food_choice(2, None).unwrap();
    assert!(respectful.commit(stale).is_err());
    for _ in 0..3 {
        respectful.advance().unwrap();
    }
    assert!(
        respectful
            .snapshot()
            .dynamics
            .unwrap()
            .claims
            .unwrap()
            .events
            .is_empty()
    );
}

#[test]
fn work_respects_or_disregards_personal_claims_and_records_only_admitted_inputs() {
    use game_parallel_economy_claims::Claims;
    let mut initial = fixture();
    initial.agreements = None;
    initial.people[0].seek_meals = false;
    initial.opinions.as_mut().unwrap().rules.theft_loss = 2;
    initial.dynamics = Some(Dynamics {
        personality: None,
        social: None,
        disease: None,
        treatment: vec![],
        claims: Some(Claims::default()),
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.claim_stock(1, 1, 2).unwrap();
                assert!(world.advance().unwrap().started.is_empty());
                assert!(
                    world
                        .snapshot()
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .claims
                        .as_ref()
                        .unwrap()
                        .events
                        .is_empty()
                );
                world.set_claim_choice(2, true).unwrap();
                assert_eq!(world.advance().unwrap().started.len(), 1);
                let state = world.snapshot();
                let events = &state
                    .dynamics
                    .as_ref()
                    .unwrap()
                    .claims
                    .as_ref()
                    .unwrap()
                    .events;
                assert_eq!(events.len(), 1);
                assert_eq!(
                    (
                        events[0].person,
                        events[0].claimant,
                        events[0].good,
                        events[0].quantity
                    ),
                    (2, 1, 2, 1)
                );
                assert_eq!(
                    state
                        .opinions
                        .as_ref()
                        .unwrap()
                        .records
                        .iter()
                        .find(|o| o.observer == 1 && o.target == 2)
                        .unwrap()
                        .trust,
                    -2
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
    initial.autowork.as_mut().unwrap().policies[0].disregard_claims = true;
    let mut world = Economy::compile(initial).unwrap();
    world.claim_stock(1, 1, 2).unwrap();
    assert_eq!(world.advance().unwrap().started.len(), 1);
}

#[test]
fn cultivation_retries_another_person_and_observes_claimed_seed_use() {
    use game_parallel_economy_claims::Claims;
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.dynamics = Some(Dynamics {
        personality: None,
        social: None,
        disease: None,
        treatment: vec![],
        claims: Some(Claims::default()),
    });
    initial.agriculture = Some(Agriculture {
        inherit_births: false,
        crops: vec![Crop {
            id: 1,
            seed: 2,
            seed_quantity: 1,
            output: 1,
            output_quantity: 1,
            growth_ticks: 2,
        }],
        fields: vec![Field {
            id: 1,
            region: 1,
            crop: 1,
            phase: CropPhase::Fallow,
            remaining: 0,
        }],
        policies: (1..=2)
            .map(|person| FarmPolicy {
                person,
                willing: true,
                pressured_by: None,
                pressure_threshold: None,
            })
            .collect(),
        max_hunger: 20,
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.claim_stock(2, 1, 2).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state.agriculture.as_ref().unwrap().fields[0].phase,
                    CropPhase::Growing
                );
                assert!(
                    state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .claims
                        .as_ref()
                        .unwrap()
                        .events
                        .is_empty()
                );
                world.set_output_claim_choice(1, true).unwrap();
                world.advance().unwrap();
                world.advance().unwrap();
                assert!(
                    !world
                        .snapshot()
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .claims
                        .as_ref()
                        .unwrap()
                        .claims
                        .iter()
                        .any(|claim| claim.good == 1)
                );
                world.advance().unwrap();
                let state = world.snapshot();
                assert!(
                    state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .claims
                        .as_ref()
                        .unwrap()
                        .claims
                        .iter()
                        .any(|claim| claim.person == 1 && claim.good == 1)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
    initial
        .agriculture
        .as_mut()
        .unwrap()
        .policies
        .retain(|p| p.person == 1);
    let mut world = Economy::compile(initial).unwrap();
    world.claim_stock(2, 1, 2).unwrap();
    world.advance().unwrap();
    assert_eq!(
        world.snapshot().agriculture.as_ref().unwrap().fields[0].phase,
        CropPhase::Fallow
    );
    world.set_claim_choice(1, true).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    assert_eq!(
        state.agriculture.as_ref().unwrap().fields[0].phase,
        CropPhase::Growing
    );
    let uses = &state
        .dynamics
        .as_ref()
        .unwrap()
        .claims
        .as_ref()
        .unwrap()
        .events;
    assert_eq!(uses.len(), 1);
    assert_eq!((uses[0].person, uses[0].claimant, uses[0].good), (1, 2, 2));
}

#[test]
fn caregiver_can_choose_to_use_claimed_food_for_a_dependent() {
    use game_parallel_dependent_care::{Policy as CarePolicy, Rules as CareRules};
    use game_parallel_economy_claims::Claims;
    use game_parallel_families::{Families, FamilyRules, FamilyTotals, Genome, Member, Sex};
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.people[2].hunger = 5;
    initial.people[2].seek_meals = true;
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(Place::Person(1), 10, vec![]).unwrap();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        Place::Stock { region: 1, good: 1 },
        10,
        vec![Lot {
            good: 1,
            quantity: 1,
            expires_at: Some(8),
        }],
    )
    .unwrap();
    let members = (1..=4)
        .map(|id| Member {
            id,
            born: None,
            age: if id == 3 { 1 } else { 20 },
            lifespan_loss: 0,
            genome: Genome {
                sex: if id == 1 { Sex::Female } else { Sex::Male },
                alleles: [[500; 2]; 4],
            },
            parents: if id == 3 {
                vec![1, 2].into()
            } else {
                Default::default()
            },
            partner: None,
            pregnancy: None,
            next_conception: 0,
            alive: true,
            reproduction_allowed: false,
            reconsider_partnership: false,
        })
        .collect();
    initial.family = Some(Families {
        rules: FamilyRules {
            adult_age: 10,
            min_work_age: 10,
            fertile_until: 50,
            gestation: 2,
            birth_spacing: 3,
            lifespan_min: 60,
            lifespan_max: 100,
            conception_one_in: 100,
            minimum_health: 1,
            maximum_hunger: 100,
            newborn_health: 100,
            mutation_per_million: 0,
            mutation_step: 0,
            enroll_births: true,
        },
        members,
        next_id: 5,
        last_tick: 0,
        totals: FamilyTotals::default(),
        events: vec![],
    });
    initial.care = Some(DependentCare {
        rules: CareRules {
            dependent_until: 5,
            max_caregiver_hunger: 100,
            neglect_damage: 0,
            feed_from_cargo: true,
            fetch_local_food: true,
            pressure_threshold: None,
        },
        inherit_births: false,
        policies: vec![CarePolicy {
            person: 1,
            willing: true,
        }],
        events: vec![],
    });
    let mut dynamics = infection_from(1);
    dynamics.claims = Some(Claims::default());
    initial.dynamics = Some(dynamics);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.claim_stock(4, 1, 1).unwrap();
                world.advance().unwrap();
                assert!(
                    world
                        .snapshot()
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .disease
                        .as_ref()
                        .unwrap()
                        .cases
                        .iter()
                        .any(|case| case.person == 3 && case.source == Some(1))
                );
                assert_eq!(
                    world
                        .snapshot()
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(3))
                        .unwrap()
                        .used,
                    0
                );
                world.set_claim_choice(1, true).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let holder = state
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|c| c.place == Place::Person(3))
                    .unwrap();
                assert_eq!((holder.used, holder.lots[0].expires_at), (1, Some(8)));
                let uses = &state
                    .dynamics
                    .as_ref()
                    .unwrap()
                    .claims
                    .as_ref()
                    .unwrap()
                    .events;
                assert_eq!(uses.len(), 1);
                assert_eq!((uses[0].person, uses[0].claimant), (1, 4));
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn personal_output_claims_arise_only_after_physical_delivery() {
    use game_parallel_economy_claims::Claims;
    let mut initial = fixture();
    initial.agreements = None;
    initial.people[0].seek_meals = false;
    initial.dynamics = Some(Dynamics {
        personality: None,
        social: None,
        disease: None,
        treatment: vec![],
        claims: Some(Claims::default()),
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.set_output_claim_choice(2, true).unwrap();
                world.advance().unwrap();
                assert!(
                    world
                        .snapshot()
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .claims
                        .as_ref()
                        .unwrap()
                        .claims
                        .is_empty()
                );
                world.advance().unwrap();
                assert!(
                    world
                        .snapshot()
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .claims
                        .as_ref()
                        .unwrap()
                        .claims
                        .is_empty()
                );
                let serialized = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored =
                    Economy::compile(serde_json::from_str(&serialized).unwrap()).unwrap();
                assert_eq!(world.advance().unwrap().delivered.len(), 1);
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                let claims = &state
                    .dynamics
                    .as_ref()
                    .unwrap()
                    .claims
                    .as_ref()
                    .unwrap()
                    .claims;
                assert!(
                    claims
                        .iter()
                        .any(|c| (c.person, c.region, c.good) == (2, 1, 3))
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
    let mut world = Economy::compile(initial).unwrap();
    world.set_output_claim_choice(2, true).unwrap();
    world.advance().unwrap();
    world.set_output_claim_choice(2, false).unwrap();
    world.advance().unwrap();
    world.advance().unwrap();
    assert!(
        world
            .snapshot()
            .dynamics
            .unwrap()
            .claims
            .unwrap()
            .claims
            .is_empty()
    );
}

#[test]
fn personal_expedition_travels_and_returns_with_real_custody_and_can_be_cancelled() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.people[0].controlled = true;
    for good in 1..=3 {
        initial
            .physical
            .custody
            .containers
            .push(Container::new(Place::Stock { region: 2, good }, 10, vec![]).unwrap());
    }
    initial
        .physical
        .custody
        .containers
        .sort_unstable_by_key(|row| row.place);
    initial.travel = Some(Travel {
        routes: vec![
            Route {
                source: 1,
                destination: 2,
                ticks: 2,
            },
            Route {
                source: 2,
                destination: 1,
                ticks: 1,
            },
        ],
        journeys: vec![],
    });
    initial.missions = Some(Missions {
        next_id: 1,
        pressure_threshold: None,
        entries: vec![],
        events: vec![],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(
                    world
                        .start_mission(MissionStart {
                            person: 1,
                            destination: 9,
                            visit_ticks: 1
                        })
                        .is_err()
                );
                let id = world
                    .start_mission(MissionStart {
                        person: 1,
                        destination: 2,
                        visit_ticks: 1,
                    })
                    .unwrap();
                assert_eq!(id, 1);
                world.advance().unwrap();
                let saved = world.snapshot();
                assert!(saved.people[0].region.is_none());
                assert_eq!(saved.travel.as_ref().unwrap().journeys[0].remaining, 2);
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                for _ in 0..7 {
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(state.people[0].region, Some(1));
                assert_eq!(
                    state.missions.as_ref().unwrap().entries[0].phase,
                    MissionPhase::Completed
                );
                let holder = state
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|c| c.place == Place::Person(1))
                    .unwrap();
                assert_eq!((holder.used, holder.lots[0].expires_at), (1, Some(10)));
                state
            })
    };
    assert_eq!(run(1), run(4));
    let mut world = Economy::compile(initial).unwrap();
    let id = world
        .start_mission(MissionStart {
            person: 1,
            destination: 2,
            visit_ticks: 1,
        })
        .unwrap();
    world.advance().unwrap();
    assert!(world.cancel_mission(2, id).is_err());
    world.cancel_mission(1, id).unwrap();
    assert!(world.snapshot().people[0].region.is_none());
    world.advance().unwrap();
    world.advance().unwrap();
    assert_eq!(world.snapshot().people[0].region, Some(2));
    assert_eq!(
        world.snapshot().missions.unwrap().entries[0].phase,
        MissionPhase::Failed
    );
}

#[test]
fn autonomous_hauling_moves_real_goods_and_can_be_abandoned() {
    let mut initial = expedition_fixture();
    initial.dynamics = None;
    initial.missions = None;
    initial.people[1].willing_to_work = false;
    initial.hauling = Some(Hauling {
        asking: vec![],
        requests: vec![],
        policies: vec![HaulingPolicy {
            person: 2,
            source: 1,
            destination: 2,
            good: 2,
            quantity: 2,
            willing: true,
            requires_request: false,
            request_pressure_threshold: None,
            pressured_by: None,
            pressure_threshold: None,
        }],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let loaded = world.snapshot();
                assert_eq!(
                    loaded
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap()
                        .used,
                    2
                );
                assert_eq!(
                    loaded
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Stock { region: 1, good: 2 })
                        .unwrap()
                        .used,
                    0
                );
                let stale = world.prepare().unwrap();
                world.set_hauling_choice(2, false).unwrap();
                assert!(world.commit(stale).is_err());
                world.advance().unwrap();
                assert_eq!(world.snapshot().people[1].region, Some(1));
                world.set_hauling_choice(2, true).unwrap();
                world.advance().unwrap();
                let saved = world.snapshot();
                assert_eq!(saved.travel.as_ref().unwrap().journeys[0].destination, 2);
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                for _ in 0..3 {
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(state.people[1].region, Some(2));
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Stock { region: 2, good: 2 })
                        .unwrap()
                        .used,
                    2
                );
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap()
                        .used,
                    0
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
    let mut pressured = initial;
    let policy = &mut pressured.hauling.as_mut().unwrap().policies[0];
    policy.willing = false;
    policy.pressured_by = Some(1);
    policy.pressure_threshold = Some(3);
    pressured.opinions.as_mut().unwrap().records = vec![Opinion {
        observer: 2,
        target: 1,
        trust: 0,
        fear: 5,
        obligation: 0,
        affinity: 0,
    }];
    let mut world = Economy::compile(pressured).unwrap();
    world.advance().unwrap();
    assert_eq!(
        world
            .snapshot()
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(2))
            .unwrap()
            .used,
        2
    );
}

#[test]
fn hauling_requests_can_be_refused_followed_or_abandoned_without_a_contract() {
    let mut initial = expedition_fixture();
    initial.missions = None;
    initial.people[0].controlled = true;
    initial.hauling = Some(Hauling {
        asking: vec![],
        requests: vec![],
        policies: vec![HaulingPolicy {
            person: 2,
            source: 1,
            destination: 2,
            good: 2,
            quantity: 2,
            willing: false,
            requires_request: true,
            request_pressure_threshold: Some(3),
            pressured_by: None,
            pressure_threshold: None,
        }],
    });
    initial.opinions.as_mut().unwrap().records.clear();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.ask_haul(1, 2, 1, 2, 2, 10).unwrap();
                assert_eq!(world.hauling_requests().len(), 1);
                world.advance().unwrap();
                assert_eq!(
                    world
                        .snapshot()
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap()
                        .used,
                    0
                );
                world.set_hauling_choice(2, true).unwrap();
                world.advance().unwrap();
                let saved = world.snapshot();
                assert_eq!(
                    saved
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap()
                        .used,
                    2
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.set_hauling_choice(2, false).unwrap();
                restored.set_hauling_choice(2, false).unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(state.people[1].region, Some(1));
                assert!(state.agreements.is_none());
                state
            })
    };
    assert_eq!(run(1), run(4));
    let mut feared = initial.clone();
    feared.opinions.as_mut().unwrap().records.push(Opinion {
        observer: 2,
        target: 1,
        trust: 0,
        fear: 5,
        obligation: 0,
        affinity: 0,
    });
    let mut world = Economy::compile(feared).unwrap();
    world.ask_haul(1, 2, 1, 2, 2, 3).unwrap();
    world.advance().unwrap();
    assert_eq!(
        world
            .snapshot()
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(2))
            .unwrap()
            .used,
        2
    );
    let mut world = Economy::compile(initial).unwrap();
    world.ask_haul(1, 2, 1, 2, 2, 1).unwrap();
    world.advance().unwrap();
    world.advance().unwrap();
    assert!(world.hauling_requests().is_empty());
    world.set_hauling_choice(2, true).unwrap();
    world.advance().unwrap();
    assert_eq!(
        world
            .snapshot()
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(2))
            .unwrap()
            .used,
        0
    );
}

#[test]
fn autonomous_people_can_request_hauling_without_compelling_the_carrier() {
    let mut initial = expedition_fixture();
    initial.missions = None;
    initial.dynamics = None;
    initial.hauling = Some(Hauling {
        policies: vec![HaulingPolicy {
            person: 2,
            source: 1,
            destination: 2,
            good: 2,
            quantity: 1,
            willing: true,
            requires_request: true,
            request_pressure_threshold: None,
            pressured_by: None,
            pressure_threshold: None,
        }],
        asking: vec![HaulingAsking {
            person: 1,
            carrier: 2,
            source: 1,
            destination: 2,
            good: 2,
            enabled: true,
            retry_ticks: 5,
            request_lifetime: 4,
            next_review: 0,
        }],
        requests: vec![],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                assert_eq!(world.hauling_requests().len(), 1);
                assert_eq!(
                    world
                        .snapshot()
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap()
                        .used,
                    0
                );
                let saved = world.snapshot();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap()
                        .used,
                    1
                );
                assert!(state.agreements.is_none());
                state
            })
    };
    assert_eq!(run(1), run(4));
    let mut world = Economy::compile(initial).unwrap();
    world
        .depart(&[Departure {
            person: 1,
            destination: 2,
        }])
        .unwrap();
    world.advance().unwrap();
    assert!(world.hauling_requests().is_empty());
}

fn hauling_workload(count: u64) -> State {
    let mut state = field_workload(count);
    state.agriculture = None;
    state
        .physical
        .custody
        .containers
        .push(Container::new(Place::Stock { region: 2, good: 2 }, count * 2, vec![]).unwrap());
    state
        .physical
        .custody
        .containers
        .sort_unstable_by_key(|c| c.place);
    state.travel = Some(Travel {
        routes: vec![
            Route {
                source: 1,
                destination: 2,
                ticks: 2,
            },
            Route {
                source: 2,
                destination: 1,
                ticks: 2,
            },
        ],
        journeys: vec![],
    });
    state.hauling = Some(Hauling {
        asking: vec![],
        requests: vec![],
        policies: (1..=count)
            .map(|person| HaulingPolicy {
                person,
                source: 1,
                destination: 2,
                good: 2,
                quantity: 1,
                willing: true,
                requires_request: false,
                request_pressure_threshold: None,
                pressured_by: None,
                pressure_threshold: None,
            })
            .collect(),
    });
    state
}

#[test]
fn concentrated_hauling_arbitrates_actual_stock_across_workers() {
    let count = 4096;
    let initial = hauling_workload(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let events = world.advance().unwrap();
                assert_eq!(
                    events
                        .physical_actions
                        .iter()
                        .filter(|event| event.accepted)
                        .count(),
                    count as usize / 2
                );
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state.travel.as_ref().unwrap().journeys.len(),
                    count as usize / 2
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

fn hauling_requests_workload(count: u64) -> State {
    let mut state = hauling_workload(count);
    let hauling = state.hauling.as_mut().unwrap();
    hauling.policies.truncate(1);
    hauling.policies[0].requires_request = true;
    hauling.requests = (2..=count)
        .map(|requester| HaulingRequest {
            requester,
            carrier: 1,
            source: 1,
            destination: 2,
            good: 2,
            expires_at: 10,
        })
        .collect();
    state
}

#[test]
fn concentrated_hauling_requests_leave_one_person_in_control_of_their_action() {
    let count = 4096;
    let initial = hauling_requests_workload(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let events = world.advance().unwrap();
                assert_eq!(
                    events
                        .physical_actions
                        .iter()
                        .filter(|e| e.accepted)
                        .count(),
                    1
                );
                let state = world.snapshot();
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(1))
                        .unwrap()
                        .used,
                    2
                );
                assert_eq!(
                    state.hauling.as_ref().unwrap().requests.len(),
                    count as usize - 1
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn a_carrier_considers_a_requester_they_fear_among_other_requests() {
    let mut initial = hauling_requests_workload(4);
    let policy = &mut initial.hauling.as_mut().unwrap().policies[0];
    policy.willing = false;
    policy.request_pressure_threshold = Some(3);
    let mut opinions = fixture().opinions.unwrap();
    opinions.records = vec![Opinion {
        observer: 1,
        target: 3,
        trust: 0,
        fear: 5,
        obligation: 0,
        affinity: 0,
    }];
    initial.opinions = Some(opinions);
    let mut world = Economy::compile(initial.clone()).unwrap();
    assert_eq!(
        world
            .advance()
            .unwrap()
            .physical_actions
            .iter()
            .filter(|e| e.accepted)
            .count(),
        1
    );
    initial.opinions.as_mut().unwrap().records.clear();
    let mut world = Economy::compile(initial).unwrap();
    assert!(world.advance().unwrap().physical_actions.is_empty());
}

fn autonomous_hauling_requests_workload(count: u64) -> State {
    let mut state = hauling_requests_workload(count);
    let hauling = state.hauling.as_mut().unwrap();
    hauling.requests.clear();
    hauling.asking = (2..=count)
        .map(|person| HaulingAsking {
            person,
            carrier: 1,
            source: 1,
            destination: 2,
            good: 2,
            enabled: true,
            retry_ticks: 5,
            request_lifetime: 4,
            next_review: 0,
        })
        .collect();
    state
}

#[test]
fn many_autonomous_requests_are_independent_and_worker_deterministic() {
    let count = 4096;
    let initial = autonomous_hauling_requests_workload(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                assert_eq!(world.hauling_requests().len(), count as usize - 1);
                assert_eq!(
                    world
                        .advance()
                        .unwrap()
                        .physical_actions
                        .iter()
                        .filter(|e| e.accepted)
                        .count(),
                    1
                );
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn autonomous_hauling_requests_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = autonomous_hauling_requests_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                for _ in 0..2 {
                    world.advance().unwrap();
                }
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=2 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
#[ignore]
fn concentrated_hauling_requests_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = hauling_requests_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn hauling_respects_claims_and_does_not_deliver_spoiled_cargo() {
    use game_parallel_economy_claims::{Claim, Claims};
    let mut initial = expedition_fixture();
    initial.missions = None;
    initial.nutrition = None;
    initial.travel.as_mut().unwrap().routes[0].ticks = 3;
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![Lot {
            good: 1,
            quantity: 1,
            expires_at: Some(4),
        }],
    )
    .unwrap();
    initial.dynamics = Some(Dynamics {
        personality: None,
        social: None,
        disease: None,
        treatment: vec![],
        claims: Some(Claims {
            inherit_births: false,
            site_claims: vec![],
            site_events: vec![],
            claims: vec![Claim {
                person: 1,
                region: 1,
                good: 1,
            }],
            ..Default::default()
        }),
    });
    initial.hauling = Some(Hauling {
        asking: vec![],
        requests: vec![],
        policies: vec![HaulingPolicy {
            person: 2,
            source: 1,
            destination: 2,
            good: 1,
            quantity: 1,
            willing: true,
            requires_request: false,
            request_pressure_threshold: None,
            pressured_by: None,
            pressure_threshold: None,
        }],
    });
    let mut world = Economy::compile(initial).unwrap();
    world.advance().unwrap();
    assert_eq!(
        world
            .snapshot()
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(2))
            .unwrap()
            .used,
        0
    );
    world.set_claim_choice(2, true).unwrap();
    world.advance().unwrap();
    assert_eq!(
        world
            .snapshot()
            .dynamics
            .as_ref()
            .unwrap()
            .claims
            .as_ref()
            .unwrap()
            .events
            .len(),
        1
    );
    world.advance().unwrap();
    assert_eq!(
        world.snapshot().travel.as_ref().unwrap().journeys[0].destination,
        2
    );
    let spoiled = world.advance().unwrap();
    assert!(!spoiled.spoiled.is_empty());
    world.advance().unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    assert_eq!(state.people[1].region, Some(2));
    assert_eq!(
        state
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Stock { region: 2, good: 1 })
            .unwrap()
            .used,
        0
    );
}

#[test]
#[ignore]
fn concentrated_hauling_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = hauling_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

fn expedition_fixture() -> State {
    use game_parallel_social::{Participant, Rules as SocialRules, Social};
    let mut state = fixture();
    state.agreements = None;
    state.autowork = None;
    for person in &mut state.people {
        person.seek_meals = false;
    }
    for good in 1..=3 {
        state
            .physical
            .custody
            .containers
            .push(Container::new(Place::Stock { region: 2, good }, 10, vec![]).unwrap());
    }
    state
        .physical
        .custody
        .containers
        .sort_unstable_by_key(|row| row.place);
    state.travel = Some(Travel {
        routes: vec![
            Route {
                source: 1,
                destination: 2,
                ticks: 2,
            },
            Route {
                source: 2,
                destination: 1,
                ticks: 1,
            },
        ],
        journeys: vec![],
    });
    state.missions = Some(Missions {
        next_id: 1,
        pressure_threshold: Some(3),
        entries: vec![],
        events: vec![],
    });
    state.dynamics = Some(Dynamics {
        personality: None,
        social: Some(Social {
            rules: SocialRules {
                interval: 1,
                min_health: 1,
                max_hunger: 100,
                familiarity_gain: 1,
                familiarity_max: 10,
                relationship_retention: None,
            },
            participants: vec![
                Participant {
                    person: 1,
                    willing: true,
                },
                Participant {
                    person: 2,
                    willing: false,
                },
            ],
            pressures: vec![],
            relationships: vec![],
            events: vec![],
            needs: None,
        }),
        disease: None,
        treatment: vec![],
        claims: None,
    });
    state
}

#[test]
fn pressured_companion_can_leave_when_fear_or_obligation_no_longer_motivates_them() {
    let mut initial = expedition_fixture();
    initial.people[0].controlled = true;
    initial.travel.as_mut().unwrap().routes[0].ticks = 4;
    let opinions = initial.opinions.as_mut().unwrap();
    opinions.rules.fear_decay_per_tick = 2;
    opinions.records = vec![Opinion {
        observer: 2,
        target: 1,
        trust: 0,
        fear: 5,
        obligation: 0,
        affinity: 0,
    }];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let id = world
                    .start_mission(MissionStart {
                        person: 1,
                        destination: 2,
                        visit_ticks: 1,
                    })
                    .unwrap();
                let companion = world.invite_mission(1, id, 2).unwrap().unwrap();
                let state = world.snapshot();
                let other = state
                    .missions
                    .as_ref()
                    .unwrap()
                    .entries
                    .iter()
                    .find(|m| m.id == companion)
                    .unwrap();
                assert!(other.party_compliant && !other.party_opt_in);
                world.advance().unwrap();
                let saved = world.snapshot();
                assert!(saved.people[0].region.is_none() && saved.people[1].region.is_none());
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(
                    state
                        .missions
                        .as_ref()
                        .unwrap()
                        .entries
                        .iter()
                        .find(|m| m.id == companion)
                        .unwrap()
                        .phase,
                    MissionPhase::Failed
                );
                assert!(state.people[1].region.is_none());
                state
            })
    };
    assert_eq!(run(1), run(4));
    initial.opinions.as_mut().unwrap().records.clear();
    let mut world = Economy::compile(initial).unwrap();
    let id = world
        .start_mission(MissionStart {
            person: 1,
            destination: 2,
            visit_ticks: 1,
        })
        .unwrap();
    assert_eq!(world.invite_mission(1, id, 2).unwrap(), None);
    assert_eq!(world.snapshot().missions.unwrap().entries.len(), 1);
}

#[test]
fn initially_willing_companion_can_change_their_mind() {
    let mut initial = expedition_fixture();
    initial.people[0].controlled = true;
    initial.opinions.as_mut().unwrap().records.clear();
    initial
        .dynamics
        .as_mut()
        .unwrap()
        .social
        .as_mut()
        .unwrap()
        .participants[1]
        .willing = true;
    let mut world = Economy::compile(initial).unwrap();
    let id = world
        .start_mission(MissionStart {
            person: 1,
            destination: 2,
            visit_ticks: 1,
        })
        .unwrap();
    let companion = world.invite_mission(1, id, 2).unwrap().unwrap();
    world.advance().unwrap();
    world.set_social_choice(2, false).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    assert_eq!(
        state
            .missions
            .unwrap()
            .entries
            .iter()
            .find(|m| m.id == companion)
            .unwrap()
            .phase,
        MissionPhase::Failed
    );
    assert!(state.people[1].region.is_none());
}

#[test]
fn expedition_meeting_goal_requires_a_real_local_encounter() {
    use game_parallel_social::Participant;
    let mut initial = expedition_fixture();
    initial.people[2].region = Some(2);
    let social = initial.dynamics.as_mut().unwrap().social.as_mut().unwrap();
    social.participants.retain(|p| p.person != 2);
    social.participants.push(Participant {
        person: 3,
        willing: true,
    });
    let mut world = Economy::compile(initial).unwrap();
    let id = world
        .start_mission(MissionStart {
            person: 1,
            destination: 2,
            visit_ticks: 1,
        })
        .unwrap();
    world.set_mission_meeting(1, id, 3).unwrap();
    for _ in 0..8 {
        world.advance().unwrap();
    }
    let state = world.snapshot();
    let mission = &state.missions.unwrap().entries[0];
    assert!(mission.objective_met);
    assert_eq!(mission.phase, MissionPhase::Completed);
}

#[test]
fn explicit_meeting_respects_refusal_or_personal_pressure_even_off_cadence() {
    use game_parallel_social::PressurePolicy;
    let mut initial = expedition_fixture();
    initial.people[0].controlled = true;
    initial.opinions.as_mut().unwrap().records = vec![Opinion {
        observer: 2,
        target: 1,
        trust: 0,
        fear: 5,
        obligation: 0,
        affinity: 0,
    }];
    let mut disease = infection_from(1).disease;
    let dynamics = initial.dynamics.as_mut().unwrap();
    dynamics.disease = disease.take();
    let social = dynamics.social.as_mut().unwrap();
    social.rules.interval = 5;
    social.pressures.push(PressurePolicy {
        person: 2,
        fear_at_least: Some(3),
        obligation_at_least: None,
        enabled: true,
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.meet(1, 2).unwrap());
                let state = world.snapshot();
                assert_eq!(world.tick(), 1);
                assert_eq!(
                    state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .social
                        .as_ref()
                        .unwrap()
                        .events[0]
                        .pressured,
                    Some(2)
                );
                assert!(
                    state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .disease
                        .as_ref()
                        .unwrap()
                        .cases
                        .iter()
                        .any(|case| case.person == 2 && case.source == Some(1))
                );
                world.set_social_pressure(2, false).unwrap();
                assert!(!world.meet(1, 2).unwrap());
                assert_eq!(world.tick(), 2);
                assert!(
                    world
                        .snapshot()
                        .dynamics
                        .unwrap()
                        .social
                        .unwrap()
                        .events
                        .is_empty()
                );
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn controlled_explorer_can_explicitly_meet_their_objective() {
    use game_parallel_social::Participant;
    let mut initial = expedition_fixture();
    initial.people[0].controlled = true;
    initial.people[2].region = Some(2);
    let social = initial.dynamics.as_mut().unwrap().social.as_mut().unwrap();
    social.rules.interval = 10;
    social.participants.retain(|p| p.person != 2);
    social.participants.push(Participant {
        person: 3,
        willing: true,
    });
    let mut world = Economy::compile(initial).unwrap();
    let id = world
        .start_mission(MissionStart {
            person: 1,
            destination: 2,
            visit_ticks: 1,
        })
        .unwrap();
    world.set_mission_meeting(1, id, 3).unwrap();
    for _ in 0..3 {
        world.advance().unwrap();
    }
    assert_eq!(world.snapshot().people[0].region, Some(2));
    assert!(world.meet(1, 3).unwrap());
    assert!(world.snapshot().missions.as_ref().unwrap().entries[0].objective_met);
    for _ in 0..4 {
        world.advance().unwrap();
    }
    assert_eq!(
        world.snapshot().missions.unwrap().entries[0].phase,
        MissionPhase::Completed
    );
}

fn campaign_fixture() -> State {
    use game_parallel_economy_campaign::{Campaign, Guardian, Guardians, Life, Lineage};
    use game_parallel_families::{
        Families, FamilyRules, FamilyTotals, Genome, Member, Pregnancy, Sex,
    };
    let mut state = expedition_fixture();
    state.dynamics = None;
    state.missions = None;
    state.people[3].health = 0;
    state.family = Some(Families {
        rules: FamilyRules {
            adult_age: 10,
            min_work_age: 10,
            fertile_until: 50,
            gestation: 2,
            birth_spacing: 3,
            lifespan_min: 60,
            lifespan_max: 100,
            conception_one_in: 100,
            minimum_health: 1,
            maximum_hunger: 100,
            newborn_health: 100,
            mutation_per_million: 0,
            mutation_step: 0,
            enroll_births: true,
        },
        members: (1..=4)
            .map(|id| Member {
                id,
                born: None,
                age: 20,
                lifespan_loss: 0,
                genome: Genome {
                    sex: if id == 1 { Sex::Female } else { Sex::Male },
                    alleles: [[500; 2]; 4],
                },
                parents: Default::default(),
                partner: match id {
                    1 => Some(2),
                    2 => Some(1),
                    _ => None,
                },
                pregnancy: (id == 1).then_some(Pregnancy { father: 2, due: 2 }),
                next_conception: 0,
                alive: id != 4,
                reproduction_allowed: true,
                reconsider_partnership: false,
            })
            .collect(),
        next_id: 5,
        last_tick: 0,
        totals: FamilyTotals::default(),
        events: vec![],
    });
    state.pending_food = vec![FoodEffect {
        person: 3,
        at: 1,
        health: -100,
        nutrients: [0; 6],
    }];
    state.campaign = Some(Campaign {
        lineage: Lineage {
            current: Some(3),
            started: 0,
            waiting_since: None,
            history: vec![Life {
                person: 4,
                started: 0,
                ended: 0,
            }],
            events: vec![],
            invocation: None,
        },
        guardians: Some(Guardians {
            recovery_per_tick: 10,
            max_health: 100,
            mentorship_ticks: 2,
            guardians: vec![Guardian {
                id: 42,
                region: 2,
                health: 80,
                confined: false,
                journey: None,
                student: None,
                mentorship: 0,
                released: false,
            }],
        }),
    });
    state
}

#[test]
fn newborn_farming_inclinations_are_optional_and_do_not_inherit_pressure() {
    let mut initial = campaign_fixture();
    initial.campaign = None;
    initial.pending_food.clear();
    initial.autowork = None;
    initial.agreements = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.agriculture = Some(Agriculture {
        inherit_births: true,
        max_hunger: 100,
        crops: vec![Crop {
            id: 1,
            seed: 2,
            seed_quantity: 1,
            output: 1,
            output_quantity: 1,
            growth_ticks: 2,
        }],
        fields: vec![Field {
            id: 1,
            region: 1,
            crop: 1,
            phase: CropPhase::Fallow,
            remaining: 0,
        }],
        policies: vec![
            FarmPolicy {
                person: 1,
                willing: false,
                pressured_by: Some(2),
                pressure_threshold: Some(3),
            },
            FarmPolicy {
                person: 2,
                willing: false,
                pressured_by: None,
                pressure_threshold: None,
            },
        ],
    });
    let run = |workers, inherit| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = initial.clone();
                state.agriculture.as_mut().unwrap().inherit_births = inherit;
                let mut world = Economy::compile(state).unwrap();
                world.advance().unwrap();
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored = Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                let policy = state
                    .agriculture
                    .as_ref()
                    .unwrap()
                    .policies
                    .iter()
                    .find(|policy| policy.person == 5);
                assert_eq!(policy.is_some(), inherit);
                if let Some(policy) = policy {
                    assert!(!policy.willing);
                    assert_eq!(
                        (policy.pressured_by, policy.pressure_threshold),
                        (None, None)
                    );
                    world.set_farming_choice(5, true).unwrap();
                    world.advance().unwrap();
                    assert_eq!(
                        world.snapshot().agriculture.unwrap().fields[0].phase,
                        CropPhase::Fallow
                    );
                }
                state
            })
    };
    assert_eq!(run(1, true), run(4, true));
    assert_eq!(run(1, false), run(4, false));
}

#[test]
fn newborn_hunting_inclinations_do_not_inherit_targets_or_pressure() {
    let mut initial = campaign_fixture();
    initial.campaign = None;
    initial.pending_food.clear();
    initial.autowork = None;
    initial.agreements = None;
    for person in &mut initial.people {
        person.seek_meals = false;
        person.controlled = false;
    }
    initial.fauna = hunting_fixture().fauna;
    initial.fauna.as_mut().unwrap().animals[0].pregnancy = None;
    initial.hunting = Some(Hunting {
        inherit_births: true,
        policies: vec![HuntingPolicy {
            person: 1,
            animal: Some(1),
            willing: false,
            affinity_at_least: None,
            affinity_at_most: None,
            hunger_at_least: Some(5),
            reserve_good: Some(2),
            reserve_below: Some(3),
            pressured_by: Some(2),
            pressure_threshold: Some(3),
        }],
    });
    let run = |workers, inherit| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = initial.clone();
                state.hunting.as_mut().unwrap().inherit_births = inherit;
                let mut world = Economy::compile(state).unwrap();
                world.advance().unwrap();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&world.snapshot()).unwrap())
                        .unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                let policy = state
                    .hunting
                    .as_ref()
                    .unwrap()
                    .policies
                    .iter()
                    .find(|p| p.person == 5);
                assert_eq!(policy.is_some(), inherit);
                if let Some(policy) = policy {
                    assert!(!policy.willing);
                    assert_eq!(
                        (
                            policy.animal,
                            policy.pressured_by,
                            policy.pressure_threshold
                        ),
                        (None, None, None)
                    );
                    assert_eq!(
                        (
                            policy.hunger_at_least,
                            policy.reserve_good,
                            policy.reserve_below
                        ),
                        (Some(5), Some(2), Some(3))
                    );
                    assert!(
                        state
                            .opinions
                            .as_ref()
                            .unwrap()
                            .records
                            .iter()
                            .all(|record| record.observer != 5)
                    );
                }
                state
            })
    };
    assert_eq!(run(1, true), run(4, true));
    assert_eq!(run(1, false), run(4, false));
}

#[test]
fn newborn_claim_preferences_do_not_inherit_assertions_or_feelings() {
    use game_parallel_economy_claims::{Claim, Claims, Preference, SiteClaim};
    let mut initial = campaign_fixture();
    initial.campaign = None;
    initial.pending_food.clear();
    initial.autowork = None;
    initial.agreements = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.work_sites = Some(WorkSites {
        next_id: 8,
        sites: vec![WorkSite {
            id: 7,
            region: 1,
            kind: "workshop".into(),
            capacity: 1,
        }],
        requirements: vec![],
        bindings: vec![],
    });
    initial.dynamics = Some(Dynamics {
        social: None,
        personality: None,
        disease: None,
        treatment: vec![],
        claims: Some(Claims {
            inherit_births: true,
            claims: vec![Claim {
                person: 1,
                region: 1,
                good: 2,
            }],
            preferences: vec![Preference {
                person: 1,
                disregard_others: true,
                assert_outputs: true,
                assert_built_sites: true,
                desperate_hunger: Some(20),
            }],
            site_claims: vec![SiteClaim { person: 1, site: 7 }],
            ..Default::default()
        }),
    });
    let run = |workers, inherit| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = initial.clone();
                state
                    .dynamics
                    .as_mut()
                    .unwrap()
                    .claims
                    .as_mut()
                    .unwrap()
                    .inherit_births = inherit;
                let mut world = Economy::compile(state).unwrap();
                world.advance().unwrap();
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored = Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                let claims = state.dynamics.as_ref().unwrap().claims.as_ref().unwrap();
                assert!(claims.claims.iter().all(|claim| claim.person != 5));
                assert!(claims.site_claims.iter().all(|claim| claim.person != 5));
                assert!(
                    state
                        .opinions
                        .as_ref()
                        .unwrap()
                        .records
                        .iter()
                        .all(|row| row.observer != 5 && row.target != 5)
                );
                let preference = claims
                    .preferences
                    .iter()
                    .find(|preference| preference.person == 5);
                assert_eq!(preference.is_some(), inherit);
                if let Some(preference) = preference {
                    assert!(
                        preference.disregard_others
                            && preference.assert_outputs
                            && preference.assert_built_sites
                    );
                    assert_eq!(preference.desperate_hunger, Some(20));
                    world.set_claim_choice(5, false).unwrap();
                    assert!(
                        !world
                            .snapshot()
                            .dynamics
                            .unwrap()
                            .claims
                            .unwrap()
                            .preferences
                            .iter()
                            .find(|p| p.person == 5)
                            .unwrap()
                            .disregard_others
                    );
                }
                state
            })
    };
    assert_eq!(run(1, true), run(4, true));
    assert_eq!(run(1, false), run(4, false));
}

#[test]
fn newborn_helping_inclinations_do_not_inherit_memories_or_targets() {
    let mut initial = campaign_fixture();
    initial.campaign = None;
    initial.pending_food.clear();
    initial.autowork = None;
    initial.agreements = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.giving = Some(Giving {
        practices: None,
        inherit_births: true,
        policies: vec![],
        responses: vec![FoodResponse {
            person: 1,
            good: 1,
            quantity: 1,
            recipient_hunger_at_least: 3,
            willing: false,
            minimum_trust: None,
            affinity_threshold: Some(2),
            affinity_at_most: None,
            affinity_weight: None,
            pressure_threshold: Some(3),
        }],
        learning: vec![SharingLearning {
            person: 1,
            enabled: true,
            threshold: 2,
            capacity: 8,
            experiences: 2,
            last_helper: Some(2),
            reciprocate_hunger: None,
            ask_helper: Some(HelperAsking {
                min_hunger: 99,
                retry_ticks: 3,
                request_lifetime: 4,
                next_review: 0,
            }),
        }],
        asking: vec![FoodAsking {
            person: 1,
            target: 2,
            alternatives: vec![],
            next_target: 0,
            min_hunger: 99,
            enabled: true,
            retry_ticks: 3,
            request_lifetime: 4,
            next_review: 0,
        }],
        requests: vec![],
        acquaintance_asking: vec![AcquaintanceAsking {
            person: 1,
            enabled: true,
            min_hunger: 99,
            retry_ticks: 3,
            request_lifetime: 4,
            min_familiarity: 1,
            min_affinity: Some(-2),
            prefer_affinity: true,
            max_affinity: None,
            affinity_weight: None,
            avoid_last: None,
            next_review: 0,
            last_target: Some(2),
        }],
    });
    let run = |workers, inherit| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = initial.clone();
                state.giving.as_mut().unwrap().inherit_births = inherit;
                let mut world = Economy::compile(state).unwrap();
                world.advance().unwrap();
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored = Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                let giving = state.giving.as_ref().unwrap();
                let response = giving.responses.iter().find(|row| row.person == 5);
                assert_eq!(response.is_some(), inherit);
                assert!(giving.asking.iter().all(|row| row.person != 5));
                if let Some(response) = response {
                    assert!(!response.willing);
                    assert_eq!(
                        (response.affinity_threshold, response.pressure_threshold),
                        (Some(2), Some(3))
                    );
                    let learned = giving.learning.iter().find(|row| row.person == 5).unwrap();
                    assert_eq!((learned.experiences, learned.last_helper), (0, None));
                    assert_eq!(learned.ask_helper.as_ref().unwrap().next_review, 2);
                    let asking = giving
                        .acquaintance_asking
                        .iter()
                        .find(|row| row.person == 5)
                        .unwrap();
                    assert_eq!((asking.last_target, asking.next_review), (None, 2));
                }
                state
            })
    };
    assert_eq!(run(1, true), run(4, true));
    assert_eq!(run(1, false), run(4, false));
}

#[test]
fn newborn_work_cooperation_preferences_do_not_inherit_work_memories() {
    use game_parallel_autowork::requests::{Asker, Memory, Preference, Requests};
    use game_parallel_dreams::{Dream, Learning};
    let mut initial = campaign_fixture();
    initial.campaign = None;
    initial.pending_food.clear();
    initial.agreements = None;
    initial.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: false,
        people: vec![],
        events: vec![],
    });
    initial.dreams = Some(Dreams {
        inherit_births: true,
        people: vec![Dream {
            person: 1,
            recipe: 1,
            priority: 1,
            batch: 1,
            review_interval: 2,
            next_review: 0,
            enabled: false,
            recognize_help: false,
        }],
        learning: vec![Learning {
            person: 1,
            max_aspirations: 2,
            priority: 4,
            batch: 1,
            review_interval: 3,
        }],
        events: vec![],
    });
    for person in &mut initial.people {
        person.seek_meals = false;
        person.willing_to_work = false;
    }
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: Some(Requests {
            next_id: 1,
            inherit_births: true,
            memory_limit: 4,
            work_links: vec![],
            memories: vec![Memory {
                observer: 1,
                worker: 2,
                recipe: 1,
                successes: 1,
                failures: 0,
                last_tick: 0,
                last_success_at: Some(0),
                last_success: None,
                last_refusal_at: None,
            }],
            preferences: vec![Preference {
                person: 1,
                effort: 5,
                help_value: 2,
                affinity_weight: -3,
                trust_weight: 0,
                offer_value: None,
                disregard_claims: true,
            }],
            askers: vec![Asker {
                person: 1,
                interval: 3,
                lifetime: 4,
                retry_after_offer: false,
            }],
            pending: vec![],
            events: vec![],
        }),
    });
    let run = |workers, inherit| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = initial.clone();
                state.dreams.as_mut().unwrap().inherit_births = inherit;
                state
                    .autowork
                    .as_mut()
                    .unwrap()
                    .requests
                    .as_mut()
                    .unwrap()
                    .inherit_births = inherit;
                let mut world = Economy::compile(state).unwrap();
                world.advance().unwrap();
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored = Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                let requests = state.autowork.as_ref().unwrap().requests.as_ref().unwrap();
                let preference = requests.preferences.iter().find(|row| row.person == 5);
                assert_eq!(preference.is_some(), inherit);
                assert_eq!(requests.askers.iter().any(|row| row.person == 5), inherit);
                assert!(requests.memories.iter().all(|row| row.observer != 5));
                let dreams = state.dreams.as_ref().unwrap();
                let learning = dreams.learning.iter().find(|row| row.person == 5);
                assert_eq!(learning.is_some(), inherit);
                assert!(dreams.people.iter().all(|row| row.person != 5));
                if let Some(learning) = learning {
                    assert_eq!(
                        (
                            learning.max_aspirations,
                            learning.priority,
                            learning.review_interval
                        ),
                        (2, 4, 3)
                    );
                }
                assert!(
                    requests
                        .pending
                        .iter()
                        .all(|row| row.requester != 5 && row.worker != 5)
                );
                if let Some(preference) = preference {
                    assert_eq!(
                        (
                            preference.effort,
                            preference.help_value,
                            preference.affinity_weight,
                            preference.disregard_claims
                        ),
                        (5, 2, -3, true)
                    );
                }
                state
            })
    };
    assert_eq!(run(1, true), run(4, true));
    assert_eq!(run(1, false), run(4, false));
}

#[test]
fn successor_is_an_actual_later_birth_and_guardian_travels_before_mentorship() {
    use game_parallel_economy_campaign::ReleaseDecision;
    let initial = campaign_fixture();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.snapshot().people[2].controlled);
                world.advance().unwrap();
                let after_death = world.snapshot();
                assert_eq!(after_death.campaign.as_ref().unwrap().lineage.current, None);
                assert!(!after_death.people[2].controlled);
                assert_eq!(after_death.people.len(), 4);
                world.advance().unwrap();
                let successor = world.snapshot();
                assert_eq!(
                    successor.campaign.as_ref().unwrap().lineage.current,
                    Some(5)
                );
                assert!(successor.people.iter().any(|p| p.id == 5 && p.controlled));
                assert!(
                    successor
                        .physical
                        .custody
                        .containers
                        .iter()
                        .any(|c| c.place == Place::Person(5))
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&successor).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(
                    world.request_guardian_release(42).unwrap(),
                    ReleaseDecision::NotReady
                );
                assert_eq!(
                    restored.request_guardian_release(42).unwrap(),
                    ReleaseDecision::NotReady
                );
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(
                    world.request_guardian_release(42).unwrap(),
                    ReleaseDecision::Accepted
                );
                assert_eq!(
                    restored.request_guardian_release(42).unwrap(),
                    ReleaseDecision::Accepted
                );
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn invocation_warnings_pause_the_world_and_accepted_loss_releases_control() {
    use game_parallel_economy_campaign::{Invocation, InvocationRules, InvocationWarningKind};
    let mut initial = campaign_fixture();
    initial.pending_food.clear();
    initial.people[2].willing_to_work = true;
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![WorkPolicy {
            person: 3,
            recipe: 1,
            enabled: true,
            disregard_claims: false,
        }],
        requests: None,
    });
    initial.campaign.as_mut().unwrap().lineage.invocation = Some(Invocation {
        rules: InvocationRules {
            rate_per_life: 5,
            recovery_per_tick: 1,
            strain_threshold: 5,
            loss_threshold: 10,
            strain_consent_ticks: 2,
            control_loss_ticks: 2,
            strain_damage_per_tick: 3,
            permanent_exposure_threshold: 0,
            lifespan_loss_per_tick: 0,
        },
        owner: Some(3),
        active: vec![],
        burden: 0,
        exposure: 0,
        strain_accepted_until: None,
        lifespan_accepted_until: None,
        loss_approved: false,
        loss_triggered: false,
        control_until: None,
        pending: None,
        revision: 0,
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.invoke_lives(&[4]).unwrap();
                let before = world.snapshot();
                assert!(world.advance().is_err());
                let paused = world.snapshot();
                assert_eq!(world.tick(), 0);
                assert_eq!(paused.physical, before.physical);
                assert_eq!(paused.people, before.people);
                let warning = paused
                    .campaign
                    .as_ref()
                    .unwrap()
                    .lineage
                    .invocation
                    .as_ref()
                    .unwrap()
                    .pending
                    .as_ref()
                    .unwrap();
                assert_eq!(warning.kind, InvocationWarningKind::Strain);
                let revision = warning.revision;
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&paused).unwrap()).unwrap(),
                )
                .unwrap();
                assert!(world.accept_invocation_warning(revision + 1).is_err());
                world.accept_invocation_warning(revision).unwrap();
                restored.accept_invocation_warning(revision).unwrap();
                assert!(world.advance().unwrap().started.is_empty());
                restored.advance().unwrap();
                assert_eq!(world.snapshot().people[2].health, 97);
                assert!(world.advance().is_err());
                assert!(restored.advance().is_err());
                let paused = world.snapshot();
                let warning = paused
                    .campaign
                    .as_ref()
                    .unwrap()
                    .lineage
                    .invocation
                    .as_ref()
                    .unwrap()
                    .pending
                    .as_ref()
                    .unwrap();
                assert_eq!(warning.kind, InvocationWarningKind::ControlLoss);
                world.accept_invocation_warning(warning.revision).unwrap();
                restored
                    .accept_invocation_warning(warning.revision)
                    .unwrap();
                assert!(!world.snapshot().people[2].controlled);
                assert_eq!(world.advance().unwrap().started.len(), 1);
                restored.advance().unwrap();
                assert!(!world.snapshot().people[2].controlled);
                world.advance().unwrap();
                restored.advance().unwrap();
                assert!(world.snapshot().people[2].controlled);
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn accepted_permanent_invocation_cost_affects_real_lifespan_and_succession() {
    use game_parallel_economy_campaign::{Invocation, InvocationRules, InvocationWarningKind};
    let mut initial = campaign_fixture();
    initial.pending_food.clear();
    initial.campaign.as_mut().unwrap().lineage.invocation = Some(Invocation {
        rules: InvocationRules {
            rate_per_life: 5,
            recovery_per_tick: 1,
            strain_threshold: 5,
            loss_threshold: 100,
            strain_consent_ticks: 2,
            control_loss_ticks: 2,
            strain_damage_per_tick: 0,
            permanent_exposure_threshold: 1,
            lifespan_loss_per_tick: 100,
        },
        owner: Some(3),
        active: vec![],
        burden: 0,
        exposure: 0,
        strain_accepted_until: None,
        lifespan_accepted_until: None,
        loss_approved: false,
        loss_triggered: false,
        control_until: None,
        pending: None,
        revision: 0,
    });
    let mut world = Economy::compile(initial).unwrap();
    world.invoke_lives(&[4]).unwrap();
    assert!(world.advance().is_err());
    let warning = world
        .snapshot()
        .campaign
        .unwrap()
        .lineage
        .invocation
        .unwrap()
        .pending
        .unwrap();
    assert_eq!(warning.kind, InvocationWarningKind::Strain);
    world.accept_invocation_warning(warning.revision).unwrap();
    assert!(world.advance().is_err());
    let warning = world
        .snapshot()
        .campaign
        .unwrap()
        .lineage
        .invocation
        .unwrap()
        .pending
        .unwrap();
    assert_eq!(warning.kind, InvocationWarningKind::Lifespan);
    assert_eq!(world.tick(), 0);
    world.accept_invocation_warning(warning.revision).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    let member = state
        .family
        .as_ref()
        .unwrap()
        .members
        .iter()
        .find(|m| m.id == 3)
        .unwrap();
    assert_eq!(member.lifespan_loss, 100);
    assert!(!member.alive);
    assert_eq!(state.people[2].health, 0);
    assert_eq!(state.campaign.unwrap().lineage.current, None);
}

fn training_rules(work_per_level: u32) -> Skills {
    use game_parallel_economy_skills::Rule;
    Skills {
        rules: vec![Rule {
            recipe: 1,
            completions_per_level: 1,
            max_level: 1,
            work_per_level,
        }],
        aliases: vec![],
        catalog_training: None,
        records: vec![],
        events: vec![],
        lessons: vec![],
        teachers: vec![],
        inherit_teachers: false,
    }
}

#[test]
fn concentrated_work_learning_is_identical_across_workers() {
    let mut initial = concentrated(4096);
    initial.skills = Some(training_rules(1));
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(state.skills.as_ref().unwrap().records.len(), 1024);
                state
            })
    };
    assert_eq!(run(1), run(4));
}

fn knowledge_workload(count: u64) -> State {
    use game_parallel_economy_knowledge::{Knowledge, Mind};
    let mut state = concentrated(count);
    state.autowork = None;
    state.nutrition = None;
    for person in &mut state.people {
        person.seek_meals = false;
        person.hunger = 0;
    }
    state.knowledge = Some(Knowledge {
        max_records: 4,
        minds: (1..=count)
            .map(|person| Mind {
                person,
                misreport: None,
                records: vec![],
            })
            .collect(),
    });
    state
}

#[test]
fn concentrated_personal_observations_match_across_workers() {
    let initial = knowledge_workload(4096);
    let run =
        |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut world = Economy::compile(initial.clone()).unwrap();
                    for _ in 0..3 {
                        world.advance().unwrap();
                    }
                    let state = world.snapshot();
                    assert!(state.knowledge.as_ref().unwrap().minds.iter().all(
                        |m| m.records.len() == 1
                            && m.records[0].observed_at == 2
                            && m.records[0].witness == m.person
                    ));
                    state
                })
        };
    assert_eq!(run(1), run(4));
}

fn discrepancy_workload(count: u64) -> State {
    let mut state = knowledge_workload(count);
    for mind in &mut state.knowledge.as_mut().unwrap().minds {
        let speaker = if mind.person == 1 { 2 } else { 1 };
        mind.records.push(game_parallel_economy_knowledge::Record {
            region: 1,
            food: 99,
            observed_at: 0,
            witness: speaker,
            learned_from: Some(speaker),
        });
    }
    let mut opinions = fixture().opinions.unwrap();
    opinions.records.clear();
    opinions.rules.affinity = Some(game_parallel_opinions::AffinityRules {
        limit: 10,
        encounter_change: 0,
        pressured_encounter_change: 0,
        help_change: 0,
        harm_change_per_damage: 0,
    });
    opinions.sensitivities = (1..=count)
        .map(|person| OpinionSensitivity {
            person,
            affinity_report_discrepancy: Some(1),
            ..Default::default()
        })
        .collect::<Vec<_>>()
        .into();
    state.opinions = Some(opinions);
    state
}

#[test]
fn concentrated_personal_report_reactions_are_deterministic_and_happen_once() {
    let mut initial = discrepancy_workload(20_000);
    for sensitivity in initial.opinions.as_mut().unwrap().sensitivities_mut() {
        sensitivity.trust_report_discrepancy = Some(1);
    }
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                world.advance().unwrap();
                let result = world.snapshot();
                let opinions = result.opinions.as_ref().unwrap();
                assert_eq!(opinions.records.len(), 20_000);
                assert!(opinions.records.iter().all(|r| r.affinity == 1));
                assert!(opinions.records.iter().all(|r| r.trust == 1));
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_report_discrepancy_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let mut initial = discrepancy_workload(count);
    if std::env::var_os("LEGACY_REPORT_TRUST").is_some() {
        for sensitivity in initial.opinions.as_mut().unwrap().sensitivities_mut() {
            sensitivity.trust_report_discrepancy = Some(1);
        }
    }
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let start = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(start.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
            #[cfg(feature = "profile")]
            for (name, measurement) in game_diagnostics::snapshot() {
                println!(
                    "{name}: {:.3} ms/call",
                    measurement.nanoseconds as f64 / measurement.calls as f64 / 1_000_000.0
                );
            }
        });
}

#[test]
fn controlled_work_uses_actual_inputs_and_advances_only_when_chosen() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.people[0].controlled = true;
    initial.people[0].seek_meals = false;
    initial.autowork.as_mut().unwrap().policies.clear();
    initial.physical.recipes[0].ticks = 2;
    initial.skills = Some(training_rules(1));
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let stale = world.prepare().unwrap();
                assert_eq!(world.start_work(1, 1, 1).unwrap().started, vec![1]);
                assert!(world.commit(stale).is_err());
                let state = world.snapshot();
                let stock = state
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|c| c.place == Place::Stock { region: 1, good: 2 })
                    .unwrap();
                assert_eq!(stock.used, 1);
                world.advance().unwrap();
                assert_eq!(world.snapshot().physical.work[0].job.remaining, 2);
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored = Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                assert!(world.continue_work(1).unwrap().finished.is_empty());
                restored.continue_work(1).unwrap();
                assert_eq!(world.continue_work(1).unwrap().finished, vec![1]);
                restored.continue_work(1).unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                assert_eq!(world.snapshot().skills.unwrap().records[0].person, 1);
                world.advance().unwrap();
                assert!(world.snapshot().physical.work.is_empty());
                assert_eq!(world.tick(), 5);
                assert_eq!(world.start_work(1, 1, 3).unwrap().start_denied, vec![2]);
                assert_eq!(world.tick(), 6);
                assert!(world.start_work(2, 1, 1).is_err());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn controlled_and_autonomous_work_share_the_same_contested_inputs() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.people[0].controlled = true;
    initial.people[0].seek_meals = false;
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 2 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        10,
        vec![Lot {
            good: 2,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let result = world.start_work(1, 1, 1).unwrap();
                assert_eq!(result.started.len(), 1);
                assert_eq!(result.start_denied.len(), 1);
                assert_eq!(world.snapshot().physical.work.len(), 1);
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn controlled_meal_consumes_carried_food_and_applies_delayed_effects() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    initial.people[0].controlled = true;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.eat(1, 1).unwrap());
                let state = world.snapshot();
                assert_eq!(state.people[0].hunger, 1);
                assert_eq!(state.people[0].health, 100);
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(1))
                        .unwrap()
                        .used,
                    0
                );
                assert_eq!(state.pending_food[0].at, 2);
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                assert!(!world.eat(1, 1).unwrap());
                assert!(!restored.eat(1, 1).unwrap());
                assert_eq!(world.snapshot(), restored.snapshot());
                assert_eq!(world.snapshot().people[0].health, 99);
                assert!(world.eat(2, 1).is_err());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
    let container = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(1))
        .unwrap();
    *container = Container::new(
        Place::Person(1),
        10,
        vec![Lot {
            good: 1,
            quantity: 1,
            expires_at: Some(1),
        }],
    )
    .unwrap();
    let mut world = Economy::compile(initial).unwrap();
    assert!(!world.eat(1, 1).unwrap());
    assert_eq!(world.snapshot().people[0].hunger, 6);
}

#[test]
fn controlled_transfers_obey_physical_custody_and_generate_observed_claim_use() {
    use game_parallel_economy_claims::{Claim, Claims};
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    initial.people[0].controlled = true;
    initial.people[0].seek_meals = false;
    initial.dynamics = Some(Dynamics {
        personality: None,
        social: None,
        disease: None,
        treatment: vec![],
        claims: Some(Claims {
            inherit_births: false,
            site_claims: vec![],
            site_events: vec![],
            claims: vec![Claim {
                person: 2,
                region: 1,
                good: 2,
            }],
            ..Default::default()
        }),
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.transfer_stock(1, 2, 1, true).unwrap());
                let state = world.snapshot();
                let claim = &state
                    .dynamics
                    .as_ref()
                    .unwrap()
                    .claims
                    .as_ref()
                    .unwrap()
                    .events[0];
                assert_eq!((claim.person, claim.claimant, claim.good), (1, 2, 2));
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(1))
                        .unwrap()
                        .used,
                    2
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                assert!(world.transfer_stock(1, 2, 1, false).unwrap());
                assert!(restored.transfer_stock(1, 2, 1, false).unwrap());
                assert!(!world.transfer_stock(1, 2, 3, true).unwrap());
                assert!(!restored.transfer_stock(1, 2, 3, true).unwrap());
                assert_eq!(world.snapshot(), restored.snapshot());
                assert!(world.transfer_stock(2, 2, 1, true).is_err());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn local_deceased_possessions_can_be_recovered_only_through_physical_transfer() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    initial.nutrition = None;
    initial.people[1].controlled = true;
    initial.people[3].health = 0;
    initial.people[0].seek_meals = false;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.recover_possessions(2, 3, 1, 1).is_err());
                assert_eq!(world.tick(), 0);
                assert!(world.recover_possessions(2, 4, 1, 1).unwrap());
                let saved = world.snapshot();
                let holder = |id| {
                    saved
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(id))
                        .unwrap()
                        .used
                };
                assert_eq!((holder(2), holder(4)), (1, 0));
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                assert!(!world.recover_possessions(2, 4, 1, 1).unwrap());
                assert!(!restored.recover_possessions(2, 4, 1, 1).unwrap());
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_knowledge_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = knowledge_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=3 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
#[ignore]
fn concentrated_skills_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let mut initial = concentrated(count);
    initial.skills = Some(training_rules(1));
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                let elapsed = started.elapsed().as_secs_f64();
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=3 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn completed_work_teaches_only_its_worker_and_speeds_later_work() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.people[0].seek_meals = false;
    initial.physical.recipes[0].ticks = 3;
    initial.skills = Some(training_rules(1));
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert_eq!(world.advance().unwrap().started.len(), 1);
                world.advance().unwrap();
                world.advance().unwrap();
                assert!(world.snapshot().skills.as_ref().unwrap().records.is_empty());
                assert_eq!(world.advance().unwrap().finished.len(), 1);
                let saved = world.snapshot();
                assert_eq!(saved.skills.as_ref().unwrap().records[0].person, 2);
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                assert_eq!(world.advance().unwrap().started.len(), 1);
                restored.advance().unwrap();
                assert!(world.advance().unwrap().finished.is_empty());
                restored.advance().unwrap();
                assert_eq!(world.advance().unwrap().finished.len(), 1);
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn invoked_past_skill_improves_work_without_copying_a_record() {
    use game_parallel_economy_campaign::{Invocation, InvocationRules};
    use game_parallel_economy_skills::Record;
    let mut initial = campaign_fixture();
    initial.pending_food.clear();
    initial.people[2].willing_to_work = true;
    initial.physical.recipes[0].ticks = 5;
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![WorkPolicy {
            person: 3,
            recipe: 1,
            enabled: true,
            disregard_claims: false,
        }],
        requests: None,
    });
    let mut skills = training_rules(3);
    skills.records.push(Record {
        person: 4,
        recipe: 1,
        completions: 1,
        credited_jobs: vec![],
    });
    initial.skills = Some(skills);
    initial.campaign.as_mut().unwrap().lineage.invocation = Some(Invocation {
        rules: InvocationRules {
            rate_per_life: 1,
            recovery_per_tick: 1,
            strain_threshold: 100,
            loss_threshold: 200,
            strain_consent_ticks: 2,
            control_loss_ticks: 2,
            strain_damage_per_tick: 0,
            permanent_exposure_threshold: 0,
            lifespan_loss_per_tick: 0,
        },
        owner: Some(3),
        active: vec![4],
        burden: 0,
        exposure: 0,
        strain_accepted_until: None,
        lifespan_accepted_until: None,
        loss_approved: false,
        loss_triggered: true,
        control_until: Some(5),
        pending: None,
        revision: 0,
    });
    let mut world = Economy::compile(initial).unwrap();
    world.advance().unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    assert_eq!(state.physical.work[0].job.remaining, 1);
    assert!(
        state
            .skills
            .as_ref()
            .unwrap()
            .records
            .iter()
            .all(|r| r.person != 3)
    );
    assert_eq!(world.advance().unwrap().finished.len(), 1);
    assert!(
        world
            .snapshot()
            .skills
            .unwrap()
            .records
            .iter()
            .any(|r| r.person == 3)
    );
}

#[test]
fn skilled_person_can_teach_from_fear_despite_not_volunteering() {
    use game_parallel_economy_skills::{Record, TeacherPolicy};
    use game_parallel_social::{Participant, PressurePolicy, Rules as SocialRules, Social};
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.opinions.as_mut().unwrap().records = vec![Opinion {
        observer: 2,
        target: 1,
        trust: 0,
        fear: 5,
        obligation: 0,
        affinity: 0,
    }];
    initial.dynamics = Some(Dynamics {
        personality: None,
        social: Some(Social {
            rules: SocialRules {
                interval: 1,
                min_health: 1,
                max_hunger: 100,
                familiarity_gain: 1,
                familiarity_max: 10,
                relationship_retention: None,
            },
            participants: vec![
                Participant {
                    person: 1,
                    willing: true,
                },
                Participant {
                    person: 2,
                    willing: false,
                },
            ],
            pressures: vec![PressurePolicy {
                person: 2,
                fear_at_least: Some(3),
                obligation_at_least: None,
                enabled: true,
            }],
            relationships: vec![],
            events: vec![],
            needs: None,
        }),
        disease: None,
        treatment: vec![],
        claims: None,
    });
    let mut skills = training_rules(2);
    skills.records.push(Record {
        person: 2,
        recipe: 1,
        completions: 1,
        credited_jobs: vec![],
    });
    skills.teachers.push(TeacherPolicy {
        person: 2,
        recipe: 1,
        enabled: false,
        eligible_at: 0,
    });
    initial.skills = Some(skills);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let skills = state.skills.as_ref().unwrap();
                assert_eq!(skills.lessons.len(), 1);
                assert_eq!(
                    (skills.lessons[0].teacher, skills.lessons[0].student),
                    (2, 1)
                );
                assert!(!skills.teachers[0].enabled);
                assert_eq!(skills.index().rate(1, 1), 3);
                Economy::compile(state.clone()).unwrap();
                state
            })
    };
    assert_eq!(run(1), run(4));
}

fn expedition_workload(count: u64) -> State {
    use game_parallel_economy_expeditions::Mission;
    let mut state = concentrated(count);
    state.autowork = None;
    state.nutrition = None;
    for person in &mut state.people {
        person.seek_meals = false;
        person.hunger = 0;
    }
    for good in 1..=3 {
        state
            .physical
            .custody
            .containers
            .push(Container::new(Place::Stock { region: 2, good }, count * 2, vec![]).unwrap());
    }
    state
        .physical
        .custody
        .containers
        .sort_unstable_by_key(|row| row.place);
    state.travel = Some(Travel {
        routes: vec![
            Route {
                source: 1,
                destination: 2,
                ticks: 2,
            },
            Route {
                source: 2,
                destination: 1,
                ticks: 2,
            },
        ],
        journeys: vec![],
    });
    state.missions = Some(Missions {
        next_id: count + 1,
        pressure_threshold: None,
        entries: (1..=count)
            .map(|person| Mission {
                id: person,
                person,
                party: None,
                party_opt_in: false,
                party_compliant: false,
                origin: 1,
                destination: 2,
                visit_ticks: 1,
                remaining: 0,
                phase: MissionPhase::Ready,
                objective: None,
                objective_met: false,
            })
            .collect(),
        events: vec![],
    });
    state
}

#[test]
fn concentrated_expeditions_move_real_people_deterministically() {
    let initial = expedition_workload(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state.travel.as_ref().unwrap().journeys.len(), 4096);
                assert!(
                    state
                        .missions
                        .as_ref()
                        .unwrap()
                        .entries
                        .iter()
                        .all(|m| m.phase == MissionPhase::Outbound)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_expeditions_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = expedition_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                let elapsed = started.elapsed().as_secs_f64();
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

fn claim_workload(count: u64) -> State {
    use game_parallel_economy_claims::{Claim, Claims, Preference};
    let mut initial = concentrated(count);
    initial.autowork = None;
    initial.nutrition = None;
    for container in &mut initial.physical.custody.containers {
        if container.place == (Place::Stock { region: 1, good: 1 }) {
            *container = Container::new(
                container.place,
                count * 2,
                vec![Lot {
                    good: 1,
                    quantity: count / 2,
                    expires_at: Some(10),
                }],
            )
            .unwrap();
        } else if matches!(container.place, Place::Person(id) if id <= count / 2) {
            *container = Container::new(container.place, 10, vec![]).unwrap();
        }
    }
    initial.dynamics = Some(Dynamics {
        personality: None,
        social: None,
        disease: None,
        treatment: vec![],
        claims: Some(Claims {
            inherit_births: false,
            site_claims: vec![],
            site_events: vec![],
            claims: (count / 2 + 1..=count)
                .map(|person| Claim {
                    person,
                    region: 1,
                    good: 1,
                })
                .collect(),
            preferences: (1..=count / 2)
                .map(|person| Preference {
                    person,
                    disregard_others: true,
                    assert_outputs: false,
                    assert_built_sites: false,
                    desperate_hunger: None,
                })
                .collect(),
            events: vec![],
        }),
    });
    initial
}

#[test]
fn concentrated_claims_have_bounded_observers_and_match_across_workers() {
    let initial = claim_workload(2048);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let events = &state
                    .dynamics
                    .as_ref()
                    .unwrap()
                    .claims
                    .as_ref()
                    .unwrap()
                    .events;
                assert_eq!(events.len(), 1024 * 4);
                assert!(events.iter().all(|event| event.witnessed));
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_claims_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = claim_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                let elapsed = started.elapsed().as_secs_f64();
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

fn disease_workload(count: u64) -> State {
    use game_parallel_disease::{Case, Disease, Strain};
    use game_parallel_social::{Participant, Rules as SocialRules, Social};
    let mut state = concentrated(count);
    state.autowork = None;
    state.nutrition = None;
    for person in &mut state.people {
        person.seek_meals = false;
        person.hunger = 0;
    }
    state.dynamics = Some(Dynamics {
        personality: None,
        social: Some(Social {
            rules: SocialRules {
                interval: 1,
                min_health: 1,
                max_hunger: 100,
                familiarity_gain: 1,
                familiarity_max: 10,
                relationship_retention: Some(4),
            },
            participants: (1..=count)
                .map(|person| Participant {
                    person,
                    willing: true,
                })
                .collect(),
            pressures: vec![],
            relationships: vec![],
            events: vec![],
            needs: None,
        }),
        disease: Some(Disease {
            strains: vec![Strain {
                id: 1,
                name: "fever".into(),
                incubation: 0,
                infectious_ticks: 10,
                immunity_ticks: 2,
                damage_per_tick: 1,
                transmission_per_million: 1_000_000,
                combat_transmission_per_million: 0,
            }],
            cases: (1..=count / 2)
                .map(|person| Case {
                    person,
                    strain: 1,
                    infected_at: 0,
                    source: None,
                })
                .collect(),
            treatments: vec![],
            events: vec![],
        }),
        treatment: vec![],
        claims: None,
    });
    state
}

#[test]
fn concentrated_disease_matches_across_workers() {
    let initial = disease_workload(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

fn social_disposition_workload(count: u64) -> State {
    use game_parallel_economy_dynamics::personality::{Person as Disposition, Personality};
    use game_parallel_social::{NeedRules, Needs};
    let mut state = disease_workload(count);
    let dynamics = state.dynamics.as_mut().unwrap();
    dynamics.disease = None;
    dynamics.personality = Some(Personality {
        social_urgency_per_isolation: 20,
        inherit_births: false,
        people: (1..=count)
            .map(|person| Disposition {
                person,
                sociability: 500,
                diligence: 500,
            })
            .collect(),
    });
    dynamics.social.as_mut().unwrap().needs = Some(Needs {
        rules: NeedRules {
            capacity: 100,
            isolation_gain: 1,
            encounter_relief: 5,
        },
        people: vec![],
    });
    state
}

#[test]
fn concentrated_personality_and_social_needs_match_across_workers() {
    let initial = social_disposition_workload(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_personality_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = social_disposition_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=3 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn concentrated_diligence_and_work_match_across_workers() {
    let initial = diligence_workload(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

fn diligence_workload(count: u64) -> State {
    use game_parallel_economy_dynamics::personality::{Person as Disposition, Personality};
    let mut state = concentrated(count);
    state.dynamics = Some(Dynamics {
        personality: Some(Personality {
            social_urgency_per_isolation: 0,
            inherit_births: false,
            people: (1..=count)
                .map(|person| Disposition {
                    person,
                    sociability: 500,
                    diligence: 500,
                })
                .collect(),
        }),
        social: None,
        disease: None,
        treatment: vec![],
        claims: None,
    });
    state
}

#[test]
#[ignore]
fn concentrated_diligence_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = diligence_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=3 elapsed={:.6}",
                samples[1]
            );
        });
}

fn purpose_workload(count: u64) -> State {
    use game_parallel_purpose::{Person as PurposePerson, Rules as PurposeRules};
    let mut state = concentrated(count);
    state.purpose = Some(Purpose {
        rules: PurposeRules {
            max: 100,
            per_tick: 1,
            blocked_increase: 2,
            completion_relief: 5,
            damage_threshold: 50,
            damage: 1,
        },
        people: (1..=count)
            .map(|person| PurposePerson {
                person,
                deprivation: 0,
                eligible_at: 0,
            })
            .collect(),
    });
    state
}

#[test]
fn concentrated_purpose_and_work_match_across_workers() {
    let initial = purpose_workload(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_purpose_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = purpose_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=3 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn concentrated_pressure_resolves_disjoint_encounters_deterministically() {
    use game_parallel_opinions::Opinion;
    use game_parallel_social::PressurePolicy;
    let count = 4096;
    let mut initial = disease_workload(count);
    initial.dynamics.as_mut().unwrap().disease = None;
    let social = initial.dynamics.as_mut().unwrap().social.as_mut().unwrap();
    for participant in &mut social.participants {
        participant.willing = participant.person <= count / 2;
    }
    social.pressures = (count / 2 + 1..=count)
        .map(|person| PressurePolicy {
            person,
            fear_at_least: Some(1),
            obligation_at_least: None,
            enabled: true,
        })
        .collect();
    let mut opinions = fixture().opinions.unwrap();
    opinions.records = (count / 2 + 1..=count)
        .map(|observer| Opinion {
            observer,
            target: (observer % 64) + 1,
            trust: 0,
            fear: 5,
            obligation: 0,
            affinity: 0,
        })
        .collect();
    initial.opinions = Some(opinions);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let events = &state
                    .dynamics
                    .as_ref()
                    .unwrap()
                    .social
                    .as_ref()
                    .unwrap()
                    .events;
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| event.pressured.is_some())
                        .count(),
                    64
                );
                let mut people: Vec<_> =
                    events.iter().flat_map(|event| [event.a, event.b]).collect();
                people.sort_unstable();
                assert!(people.windows(2).all(|pair| pair[0] != pair[1]));
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_disease_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = disease_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                let elapsed = started.elapsed().as_secs_f64();
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=3 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
#[ignore]
fn concentrated_pressure_benchmark() {
    use game_parallel_opinions::Opinion;
    use game_parallel_social::PressurePolicy;
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let mut initial = disease_workload(count);
    initial.dynamics.as_mut().unwrap().disease = None;
    let social = initial.dynamics.as_mut().unwrap().social.as_mut().unwrap();
    for participant in &mut social.participants {
        participant.willing = participant.person <= count / 2;
    }
    social.pressures = (count / 2 + 1..=count)
        .map(|person| PressurePolicy {
            person,
            fear_at_least: Some(1),
            obligation_at_least: None,
            enabled: true,
        })
        .collect();
    let mut opinions = fixture().opinions.unwrap();
    opinions.records = (count / 2 + 1..=count)
        .map(|observer| Opinion {
            observer,
            target: observer % 1024 + 1,
            trust: 0,
            fear: 5,
            obligation: 0,
            affinity: 0,
        })
        .collect();
    initial.opinions = Some(opinions);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                let elapsed = started.elapsed().as_secs_f64();
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=3 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn autonomous_request_can_be_fulfilled_from_fear_using_actual_goods() {
    use game_parallel_market::{Market, Policy, Value};
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.people[0].controlled = true;
    initial.dynamics = Some(infection_from(4));
    initial.opinions.as_mut().unwrap().records = vec![Opinion {
        observer: 4,
        target: 3,
        trust: 0,
        fear: 5,
        obligation: 0,
        affinity: 0,
    }];
    initial.market = Some(Market {
        next_id: 1,
        policies: vec![
            Policy {
                person: 3,
                enabled: true,
                propose: false,
                request_help: true,
                min_gain: 1,
                help_value: 0,
                values: vec![Value { good: 1, value: 4 }],
                influences: vec![],
            },
            Policy {
                person: 4,
                enabled: true,
                propose: false,
                request_help: false,
                min_gain: 0,
                help_value: 0,
                values: vec![Value { good: 1, value: 4 }],
                influences: vec![],
            },
        ],
        offers: vec![],
        events: vec![],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let saved = world.snapshot();
                assert!(
                    !saved
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .disease
                        .as_ref()
                        .unwrap()
                        .cases
                        .iter()
                        .any(|case| case.person == 3)
                );
                let offer = &saved.market.as_ref().unwrap().offers[0];
                assert_eq!((offer.proposer, offer.recipient), (3, 4));
                let mut restored = Economy::compile(saved).unwrap();
                restored.advance().unwrap();
                let result = restored.snapshot();
                assert!(
                    result
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .disease
                        .as_ref()
                        .unwrap()
                        .cases
                        .iter()
                        .any(|case| case.person == 3 && case.source == Some(4))
                );
                assert!(result.market.as_ref().unwrap().offers.is_empty());
                let holding = result
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|row| row.place == Place::Person(3))
                    .unwrap();
                assert_eq!(holding.lots[0].quantity, 1);
                assert_eq!(holding.lots[0].expires_at, Some(10));
                result
            })
    };
    assert_eq!(run(1), run(4));
}

fn practice_mind(person: u64, invent: bool) -> PracticeMind {
    PracticeMind {
        person,
        invent,
        approve: true,
        willing: true,
        ask_willing: invent,
        adjust_after_failure: true,
        pressure_threshold: None,
        shortage_threshold: 5,
        minimum_shortages: 2,
        review_interval: 2,
        proposal_lifetime: 8,
        reserve_portions: 1,
        quantity_weight: 0,
        reserve_weight: 0,
        giving_experience_weight: 0,
        shortage_experience_weight: 0,
        proposal_affinity_weight: 0,
        acceptance_threshold: 0,
        value_learning: None,
        precedent: None,
        work: None,
        reporting: Default::default(),
        report_attitude: Default::default(),
        unanswered_report_belief: None,
        require_believed_ready: false,
        next_review: 0,
        sequence: 0,
        cases: vec![].into(),
        authored: vec![].into(),
        inbox: vec![].into(),
        sent_reports: vec![].into(),
        reports: vec![].into(),
    }
}

#[test]
fn controlled_person_can_offer_unbacked_work_terms_and_ask_for_the_work() {
    use game_parallel_autowork::requests::{OfferValue, Preference, Requests};
    use game_parallel_practices::work::Mind as WorkMind;
    use game_parallel_social::{Participant, Rules as SocialRules, Social};
    let mut initial = fixture();
    initial.agreements = None;
    initial.nutrition = None;
    initial.people[0].controlled = true;
    for person in &mut initial.people {
        person.seek_meals = false;
        person.willing_to_work = false;
    }
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|holder| holder.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(Place::Person(1), 10, vec![]).unwrap();
    let mut author = practice_mind(1, false);
    let mut worker = practice_mind(2, false);
    let work = WorkMind {
        invent: false,
        approve: true,
        evaluation: None,
        outcome_policy: None,
        willing: false,
        ask_willing: false,
        review_interval: 1,
        proposal_lifetime: 8,
        reserve: 0,
        pressure_threshold: None,
        next_review: 0,
        sequence: 0,
        authored: None,
        engagement: None,
        gifts: vec![],
        experiences: vec![],
        inbox: vec![],
    };
    author.work = Some(std::sync::Arc::new(work.clone()));
    worker.work = Some(std::sync::Arc::new(work));
    initial.giving = Some(Giving {
        practices: Some(PersonalPractices {
            minds: vec![author, worker],
        }),
        inherit_births: false,
        acquaintance_asking: vec![],
        responses: vec![],
        learning: vec![],
        asking: vec![],
        requests: vec![],
        policies: vec![],
    });
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: Some(Requests {
            next_id: 1,
            inherit_births: false,
            memory_limit: 2,
            work_links: vec![],
            memories: vec![],
            askers: vec![],
            events: vec![],
            pending: vec![],
            preferences: vec![Preference {
                person: 2,
                effort: 1,
                help_value: 0,
                affinity_weight: 0,
                trust_weight: 0,
                offer_value: Some(OfferValue {
                    good: 1,
                    value_per_unit: 1,
                    max_quantity: 1,
                }),
                disregard_claims: false,
            }],
        }),
    });
    initial.dynamics = Some(Dynamics {
        personality: None,
        disease: None,
        treatment: vec![],
        claims: None,
        social: Some(Social {
            rules: SocialRules {
                interval: 1,
                min_health: 1,
                max_hunger: 100,
                familiarity_gain: 1,
                familiarity_max: 10,
                relationship_retention: None,
            },
            participants: vec![
                Participant {
                    person: 1,
                    willing: true,
                },
                Participant {
                    person: 2,
                    willing: true,
                },
            ],
            pressures: vec![],
            relationships: vec![],
            events: vec![],
            needs: None,
        }),
    });
    let run_reaction = |workers, trust_change, affinity_change, funded, via_promise| {
        let mut state = initial.clone();
        if via_promise {
            let mut agreements = fixture().agreements.unwrap();
            agreements.contracts.clear();
            agreements.events.clear();
            agreements.policies.clear();
            agreements.offering.clear();
            agreements.work_offering.clear();
            agreements.bulk_debtors.clear();
            agreements.next_id = 1;
            state.agreements = Some(agreements);
            state.opinions.as_mut().unwrap().rules.fulfilled_gain = 0;
        }
        let worker = &mut state
            .giving
            .as_mut()
            .unwrap()
            .practices
            .as_mut()
            .unwrap()
            .minds[1];
        std::sync::Arc::make_mut(worker.work.as_mut().unwrap()).outcome_policy =
            Some(game_parallel_practices::work::outcome::Policy {
                wait_ticks: 2,
                missing_trust: trust_change,
                missing_affinity: affinity_change,
            });
        state.opinions.as_mut().unwrap().rules.affinity =
            Some(game_parallel_opinions::AffinityRules {
                limit: 10,
                encounter_change: 0,
                pressured_encounter_change: 0,
                help_change: 0,
                harm_change_per_damage: 0,
            });
        if funded {
            let holder = state
                .physical
                .custody
                .containers
                .iter_mut()
                .find(|holder| holder.place == Place::Person(1))
                .unwrap();
            *holder = Container::new(
                Place::Person(1),
                10,
                vec![Lot {
                    good: 1,
                    quantity: 1,
                    expires_at: Some(20),
                }],
            )
            .unwrap();
        }
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(state).unwrap();
                assert!(world.propose_work_practice(1, 2, 1, 1, 1, 8).unwrap());
                world.advance().unwrap();
                let request = world.ask_work(1, 2, 1, 5).unwrap();
                let mut finished = None;
                for _ in 0..5 {
                    world.advance().unwrap();
                    let saved = world.snapshot();
                    let work = saved
                        .giving
                        .as_ref()
                        .unwrap()
                        .practices
                        .as_ref()
                        .unwrap()
                        .minds[1]
                        .work
                        .as_ref()
                        .unwrap();
                    if work.experiences.iter().any(|experience| {
                        experience.request == request && experience.completed.is_some()
                    }) {
                        finished = Some(saved);
                        break;
                    }
                }
                let finished =
                    finished.expect("the worker remembers their own completed requested job");
                let experience = &finished
                    .giving
                    .as_ref()
                    .unwrap()
                    .practices
                    .as_ref()
                    .unwrap()
                    .minds[1]
                    .work
                    .as_ref()
                    .unwrap()
                    .experiences[0];
                assert_eq!(
                    (
                        experience.request,
                        experience.proposal.good,
                        experience.proposal.quantity
                    ),
                    (request, 1, 1)
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&finished).unwrap()).unwrap(),
                )
                .unwrap();
                if via_promise {
                    for candidate in [&mut world, &mut restored] {
                        let id = candidate
                            .propose_promises(&[PromiseProposal {
                                debtor: 1,
                                creditor: 2,
                                good: 1,
                                quantity: 1,
                                due: candidate.tick() + 5,
                            }])
                            .unwrap()[0];
                        candidate
                            .decide_promises(&[PromiseDecision {
                                contract: id,
                                actor: 2,
                                accept: true,
                            }])
                            .unwrap();
                        assert_eq!(candidate.fulfill_promise(1, id).unwrap(), funded);
                    }
                } else if funded {
                    assert!(world.give(1, 2, 1, 1).unwrap());
                    assert!(restored.give(1, 2, 1, 1).unwrap());
                } else {
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                let state = world.snapshot();
                let work = state
                    .giving
                    .as_ref()
                    .unwrap()
                    .practices
                    .as_ref()
                    .unwrap()
                    .minds[1]
                    .work
                    .as_ref()
                    .unwrap();
                assert!(work.experiences[0].assessed);
                assert_eq!(work.experiences[0].received, u64::from(funded));
                let opinion = state
                    .opinions
                    .as_ref()
                    .unwrap()
                    .records
                    .iter()
                    .find(|row| row.observer == 2 && row.target == 1);
                let actual = opinion.map_or((0, 0), |row| (row.trust, row.affinity));
                assert_eq!(
                    actual,
                    if funded {
                        (0, 0)
                    } else {
                        (trust_change, affinity_change)
                    }
                );
                world.advance().unwrap();
                let repeated = world.snapshot();
                let opinion = repeated
                    .opinions
                    .as_ref()
                    .unwrap()
                    .records
                    .iter()
                    .find(|row| row.observer == 2 && row.target == 1);
                assert_eq!(
                    opinion.map_or((0, 0), |row| (row.trust, row.affinity)),
                    actual,
                    "one personal assessment cannot be applied twice"
                );
                state
            })
    };
    assert_eq!(
        run_reaction(1, -2, 3, false, false),
        run_reaction(4, -2, 3, false, false)
    );
    run_reaction(1, 2, -3, false, false);
    run_reaction(1, -2, 3, true, false);
    run_reaction(1, 0, 0, false, false);
    assert_eq!(
        run_reaction(1, -2, 3, true, true),
        run_reaction(4, -2, 3, true, true)
    );
    run_reaction(1, -2, 3, false, true);
    let mut absent = initial.clone();
    let worker = &mut absent
        .giving
        .as_mut()
        .unwrap()
        .practices
        .as_mut()
        .unwrap()
        .minds[1];
    std::sync::Arc::make_mut(worker.work.as_mut().unwrap()).outcome_policy =
        Some(game_parallel_practices::work::outcome::Policy {
            wait_ticks: 1,
            missing_trust: -1,
            missing_affinity: 0,
        });
    let mut absent = Economy::compile(absent).unwrap();
    assert!(absent.propose_work_practice(1, 2, 1, 1, 1, 8).unwrap());
    absent.advance().unwrap();
    let request = absent.ask_work(1, 2, 1, 5).unwrap();
    assert!(!absent.advance().unwrap().started.is_empty());
    let mut state = absent.snapshot();
    state.people[0].health = 0;
    let mut absent = Economy::compile(state).unwrap();
    for _ in 0..3 {
        absent.advance().unwrap();
    }
    let state = absent.snapshot();
    assert!(
        state
            .giving
            .as_ref()
            .unwrap()
            .practices
            .as_ref()
            .unwrap()
            .minds[1]
            .work
            .as_ref()
            .unwrap()
            .experiences
            .iter()
            .any(|experience| experience.request == request
                && experience.completed.is_some()
                && experience.assessed)
    );
    assert!(
        !state
            .autowork
            .as_ref()
            .unwrap()
            .requests
            .as_ref()
            .unwrap()
            .memories
            .iter()
            .any(|memory| memory.observer == 1
                && memory
                    .last_success
                    .is_some_and(|receipt| receipt.request == request)),
        "the absent requester did not personally witness completion"
    );
    let mut funded = initial.clone();
    let holder = funded
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|holder| holder.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(
        Place::Person(1),
        10,
        vec![Lot {
            good: 1,
            quantity: 3,
            expires_at: Some(20),
        }],
    )
    .unwrap();
    let mut funded = Economy::compile(funded).unwrap();
    assert!(funded.propose_work_practice(1, 2, 1, 1, 2, 8).unwrap());
    funded.advance().unwrap();
    let request = funded.ask_work(1, 2, 1, 5).unwrap();
    assert!(funded.give(1, 2, 1, 1).unwrap());
    assert_eq!(
        funded
            .snapshot()
            .giving
            .as_ref()
            .unwrap()
            .practices
            .as_ref()
            .unwrap()
            .minds[0]
            .work
            .as_ref()
            .unwrap()
            .engagement
            .as_ref()
            .unwrap()
            .delivered,
        0,
        "a gift before personally observing the job is not attributed to it"
    );
    let mut saw_job = false;
    for _ in 0..4 {
        funded.advance().unwrap();
        saw_job |= funded
            .snapshot()
            .autowork
            .as_ref()
            .unwrap()
            .requests
            .as_ref()
            .unwrap()
            .memories
            .iter()
            .any(|memory| {
                memory
                    .last_success
                    .is_some_and(|receipt| receipt.request == request)
            });
        if saw_job {
            break;
        }
    }
    assert!(saw_job);
    assert!(funded.give(1, 2, 1, 1).unwrap());
    let partial = funded.snapshot();
    assert_eq!(
        partial
            .giving
            .as_ref()
            .unwrap()
            .practices
            .as_ref()
            .unwrap()
            .minds[0]
            .work
            .as_ref()
            .unwrap()
            .engagement
            .as_ref()
            .unwrap()
            .delivered,
        1
    );
    let mut funded =
        Economy::compile(serde_json::from_str(&serde_json::to_string(&partial).unwrap()).unwrap())
            .unwrap();
    assert!(funded.give(1, 2, 1, 1).unwrap());
    assert!(
        funded
            .snapshot()
            .giving
            .as_ref()
            .unwrap()
            .practices
            .as_ref()
            .unwrap()
            .minds[0]
            .work
            .as_ref()
            .unwrap()
            .engagement
            .is_none(),
        "separate accepted gifts can complete the person's remembered intention"
    );
    let mut material = initial.clone();
    material
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .preferences[0]
        .offer_value
        .as_mut()
        .unwrap()
        .good = 2;
    let mut material = Economy::compile(material).unwrap();
    assert!(material.propose_work_practice(1, 2, 1, 2, 1, 8).unwrap());
    let saved = material.snapshot();
    let mut material =
        Economy::compile(serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap())
            .unwrap();
    material.advance().unwrap();
    material.ask_work(1, 2, 1, 5).unwrap();
    assert!(
        !material.advance().unwrap().started.is_empty(),
        "a worker can value an unbacked non-food offer"
    );
    let mut refusal = initial.clone();
    refusal
        .dynamics
        .as_mut()
        .unwrap()
        .social
        .as_mut()
        .unwrap()
        .participants[1]
        .willing = false;
    let mut refusal = Economy::compile(refusal).unwrap();
    assert!(!refusal.propose_work_practice(1, 2, 1, 1, 1, 8).unwrap());
    assert!(
        refusal
            .snapshot()
            .giving
            .as_ref()
            .unwrap()
            .practices
            .as_ref()
            .unwrap()
            .minds[1]
            .work
            .as_ref()
            .unwrap()
            .inbox
            .is_empty()
    );
    assert_eq!(refusal.tick(), 1, "a refused meeting still takes time");
    let mut world = Economy::compile(initial).unwrap();
    let before = world.snapshot();
    assert!(world.propose_work_practice(1, 2, 1, 1, 0, 8).is_err());
    assert_eq!(world.snapshot(), before);
    assert!(world.propose_work_practice(1, 2, 1, 1, 1, 8).unwrap());
    let after_offer = world.snapshot();
    let minds = &after_offer
        .giving
        .as_ref()
        .unwrap()
        .practices
        .as_ref()
        .unwrap()
        .minds;
    assert_eq!(
        minds[1].work.as_ref().unwrap().inbox[0].decision,
        PracticeDecision::Pending
    );
    world.advance().unwrap();
    let request = world.ask_work(1, 2, 1, 5).unwrap();
    let saved = world.snapshot();
    assert_eq!(
        saved
            .giving
            .as_ref()
            .unwrap()
            .practices
            .as_ref()
            .unwrap()
            .minds[0]
            .work
            .as_ref()
            .unwrap()
            .engagement
            .as_ref()
            .unwrap()
            .request,
        request
    );
    let mut restored =
        Economy::compile(serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap())
            .unwrap();
    assert!(!world.advance().unwrap().started.is_empty());
    restored.advance().unwrap();
    assert_eq!(world.snapshot(), restored.snapshot());
    for _ in 0..3 {
        world.advance().unwrap();
    }
    assert_eq!(
        world
            .snapshot()
            .physical
            .custody
            .containers
            .iter()
            .find(|holder| holder.place == Place::Person(2))
            .unwrap()
            .used,
        0
    );
}

#[test]
fn witnessed_work_and_a_real_gift_can_inspire_a_personal_work_proposal() {
    use game_parallel_autowork::requests::{Preference, Request, Requests};
    use game_parallel_practices::work::Mind as WorkMind;
    use game_parallel_social::{Participant, Rules as SocialRules, Social};
    let mut initial = fixture();
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|holder| holder.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(
        Place::Person(1),
        10,
        vec![Lot {
            good: 1,
            quantity: 2,
            expires_at: Some(20),
        }],
    )
    .unwrap();
    initial.agreements = None;
    initial.nutrition = None;
    for person in &mut initial.people {
        person.seek_meals = false;
        person.willing_to_work = false;
    }
    initial.people[2].hunger = 5;
    let make_work = |invent, approve| WorkMind {
        invent,
        approve,
        evaluation: None,
        outcome_policy: None,
        willing: false,
        ask_willing: false,
        review_interval: 2,
        proposal_lifetime: 8,
        reserve: 0,
        pressure_threshold: None,
        next_review: 0,
        sequence: 0,
        authored: None,
        engagement: None,
        gifts: vec![],
        experiences: vec![],
        inbox: vec![],
    };
    let mut author = practice_mind(1, false);
    author.work = Some(std::sync::Arc::new(make_work(true, false)));
    let mut worker = practice_mind(2, false);
    worker.work = Some(std::sync::Arc::new(make_work(false, true)));
    initial.giving = Some(Giving {
        practices: Some(PersonalPractices {
            minds: vec![author, worker],
        }),
        inherit_births: false,
        acquaintance_asking: vec![],
        responses: vec![],
        learning: vec![],
        asking: vec![],
        requests: vec![],
        policies: vec![GivingPolicy {
            person: 1,
            recipient: 3,
            good: 1,
            quantity: 1,
            recipient_hunger_at_least: 5,
            willing: true,
            requires_request: false,
            pressured_by: None,
            pressure_threshold: None,
        }],
    });
    initial.goals = Some(Goals {
        retry_interval: 2,
        plan_inputs: false,
        events: vec![],
        people: vec![Goal {
            person: 1,
            recipe: 1,
            priority: 1,
            target: 5,
            progress: 0,
            next_review: 0,
            enabled: true,
            recognize_help: false,
        }],
    });
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: Some(Requests {
            next_id: 2,
            inherit_births: false,
            memory_limit: 2,
            work_links: vec![],
            memories: vec![],
            askers: vec![],
            events: vec![],
            preferences: vec![Preference {
                person: 2,
                effort: 1,
                help_value: 1,
                affinity_weight: 0,
                trust_weight: 0,
                offer_value: None,
                disregard_claims: false,
            }],
            pending: vec![Request {
                id: 1,
                requester: 1,
                worker: 2,
                region: 1,
                recipe: 1,
                created_at: 0,
                expires: 5,
            }],
        }),
    });
    initial.dynamics = Some(Dynamics {
        personality: None,
        disease: None,
        treatment: vec![],
        claims: None,
        social: Some(Social {
            rules: SocialRules {
                interval: 1,
                min_health: 1,
                max_hunger: 100,
                familiarity_gain: 1,
                familiarity_max: 10,
                relationship_retention: None,
            },
            participants: vec![
                Participant {
                    person: 1,
                    willing: true,
                },
                Participant {
                    person: 2,
                    willing: true,
                },
            ],
            pressures: vec![],
            relationships: vec![],
            events: vec![],
            needs: None,
        }),
    });
    let mut material_experience = initial.clone();
    material_experience.giving.as_mut().unwrap().policies[0].good = 2;
    let holder = material_experience
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|holder| holder.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(
        Place::Person(1),
        10,
        vec![Lot {
            good: 2,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let run_material = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(material_experience.clone()).unwrap();
                for _ in 0..8 {
                    world.advance().unwrap();
                    let state = world.snapshot();
                    let minds = &state
                        .giving
                        .as_ref()
                        .unwrap()
                        .practices
                        .as_ref()
                        .unwrap()
                        .minds;
                    if let Some(received) = minds[1].work.as_ref().unwrap().inbox.first() {
                        assert_eq!(received.proposal.good, 2);
                        assert!(
                            minds[0]
                                .work
                                .as_ref()
                                .unwrap()
                                .gifts
                                .iter()
                                .any(|gift| gift.good == 2 && gift.given && gift.other == 3)
                        );
                        assert!(
                            !minds[0]
                                .cases
                                .iter()
                                .any(|case| matches!(case, PracticeCase::Gave { good: 2, .. }))
                        );
                        let mut restored = Economy::compile(
                            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                        )
                        .unwrap();
                        world.advance().unwrap();
                        restored.advance().unwrap();
                        assert_eq!(world.snapshot(), restored.snapshot());
                        return state;
                    }
                }
                panic!("a personally witnessed non-food gift can inspire work terms")
            })
    };
    assert_eq!(run_material(1), run_material(4));
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let mut delivered = None;
                for _ in 0..8 {
                    world.advance().unwrap();
                    let state = world.snapshot();
                    let minds = &state
                        .giving
                        .as_ref()
                        .unwrap()
                        .practices
                        .as_ref()
                        .unwrap()
                        .minds;
                    if !minds[1].work.as_ref().unwrap().inbox.is_empty() {
                        delivered = Some(state);
                        break;
                    }
                }
                let delivered =
                    delivered.expect("proposal delivered after real work, gift, and contact");
                let evaluate = |trust| {
                    let mut state = delivered.clone();
                    let worker = &mut state
                        .giving
                        .as_mut()
                        .unwrap()
                        .practices
                        .as_mut()
                        .unwrap()
                        .minds[1];
                    std::sync::Arc::make_mut(worker.work.as_mut().unwrap()).evaluation =
                        Some(game_parallel_practices::work::Evaluation {
                            base: 0,
                            affinity_weight: 0,
                            trust_weight: 1,
                            threshold: 1,
                        });
                    state.opinions.as_mut().unwrap().records.push(Opinion {
                        observer: 2,
                        target: 1,
                        trust,
                        fear: 0,
                        obligation: 0,
                        affinity: 0,
                    });
                    let mut world = Economy::compile(state).unwrap();
                    world.advance().unwrap();
                    world.snapshot().giving.unwrap().practices.unwrap().minds[1]
                        .work
                        .as_ref()
                        .unwrap()
                        .inbox[0]
                        .approved
                };
                assert!(evaluate(2));
                assert!(!evaluate(-2));
                let minds = &delivered
                    .giving
                    .as_ref()
                    .unwrap()
                    .practices
                    .as_ref()
                    .unwrap()
                    .minds;
                assert!(minds[0].cases.iter().any(|case| matches!(
                    case,
                    PracticeCase::Gave {
                        recipient: 3,
                        good: 1,
                        quantity: 1,
                        ..
                    }
                )));
                let proposal = &minds[1].work.as_ref().unwrap().inbox[0].proposal;
                assert_eq!(
                    (
                        proposal.author,
                        proposal.worker,
                        proposal.recipe,
                        proposal.good,
                        proposal.quantity
                    ),
                    (1, 2, 1, 1, 1)
                );
                assert_eq!(
                    minds[1].work.as_ref().unwrap().inbox[0].decision,
                    PracticeDecision::Pending
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&delivered).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(
                    state
                        .giving
                        .as_ref()
                        .unwrap()
                        .practices
                        .as_ref()
                        .unwrap()
                        .minds[1]
                        .work
                        .as_ref()
                        .unwrap()
                        .inbox[0]
                        .decision,
                    PracticeDecision::Adopted
                );
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap()
                        .used,
                    0
                );
                // The author can now ask; the worker's own assessment controls
                // whether the communicated terms contribute to motivation.
                let mut offered = state.clone();
                let minds = &mut offered
                    .giving
                    .as_mut()
                    .unwrap()
                    .practices
                    .as_mut()
                    .unwrap()
                    .minds;
                std::sync::Arc::make_mut(minds[0].work.as_mut().unwrap()).ask_willing = true;
                std::sync::Arc::make_mut(minds[0].work.as_mut().unwrap()).willing = true;
                let preference = &mut offered
                    .autowork
                    .as_mut()
                    .unwrap()
                    .requests
                    .as_mut()
                    .unwrap()
                    .preferences[0];
                preference.effort = 5;
                preference.help_value = 0;
                preference.offer_value = Some(game_parallel_autowork::requests::OfferValue {
                    good: 1,
                    value_per_unit: 5,
                    max_quantity: 1,
                });
                let mut rejected = offered.clone();
                let recipient = &mut rejected
                    .giving
                    .as_mut()
                    .unwrap()
                    .practices
                    .as_mut()
                    .unwrap()
                    .minds[1];
                let received =
                    &mut std::sync::Arc::make_mut(recipient.work.as_mut().unwrap()).inbox[0];
                received.decision = PracticeDecision::Declined;
                received.approved = false;
                let mut refuses_gift = offered.clone();
                let mind = &mut refuses_gift
                    .giving
                    .as_mut()
                    .unwrap()
                    .practices
                    .as_mut()
                    .unwrap()
                    .minds[0];
                std::sync::Arc::make_mut(mind.work.as_mut().unwrap()).willing = false;
                let mut no_stock = offered.clone();
                let holder = no_stock
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|holder| holder.place == Place::Person(1))
                    .unwrap();
                *holder = Container::new(Place::Person(1), 10, vec![]).unwrap();
                let mut keeps_reserve = offered.clone();
                let mind = &mut keeps_reserve
                    .giving
                    .as_mut()
                    .unwrap()
                    .practices
                    .as_mut()
                    .unwrap()
                    .minds[0];
                std::sync::Arc::make_mut(mind.work.as_mut().unwrap()).reserve = 1;
                let mut material_terms = offered.clone();
                let minds = &mut material_terms
                    .giving
                    .as_mut()
                    .unwrap()
                    .practices
                    .as_mut()
                    .unwrap()
                    .minds;
                std::sync::Arc::make_mut(minds[0].work.as_mut().unwrap())
                    .authored
                    .as_mut()
                    .unwrap()
                    .good = 2;
                std::sync::Arc::make_mut(minds[1].work.as_mut().unwrap()).inbox[0]
                    .proposal
                    .good = 2;
                material_terms
                    .autowork
                    .as_mut()
                    .unwrap()
                    .requests
                    .as_mut()
                    .unwrap()
                    .preferences[0]
                    .offer_value
                    .as_mut()
                    .unwrap()
                    .good = 2;
                let holder = material_terms
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|holder| holder.place == Place::Person(1))
                    .unwrap();
                *holder = Container::new(
                    Place::Person(1),
                    10,
                    vec![Lot {
                        good: 2,
                        quantity: 1,
                        expires_at: None,
                    }],
                )
                .unwrap();
                let mut observation_world = Economy::compile(offered.clone()).unwrap();
                let mut willing_world = Economy::compile(offered).unwrap();
                let mut unwilling_world = Economy::compile(rejected).unwrap();
                let mut refuses_world = Economy::compile(refuses_gift).unwrap();
                let mut broke_world = Economy::compile(no_stock).unwrap();
                let mut reserve_world = Economy::compile(keeps_reserve).unwrap();
                let mut material_world = Economy::compile(material_terms).unwrap();
                let mut willing_started = false;
                let mut unwilling_started = false;
                let mut broke_started = false;
                for _ in 0..5 {
                    willing_started |= !willing_world.advance().unwrap().started.is_empty();
                    unwilling_started |= !unwilling_world.advance().unwrap().started.is_empty();
                    refuses_world.advance().unwrap();
                    broke_started |= !broke_world.advance().unwrap().started.is_empty();
                    reserve_world.advance().unwrap();
                    material_world.advance().unwrap();
                }
                assert!(
                    willing_started,
                    "an assessed offer can motivate actual work"
                );
                assert!(
                    !unwilling_started,
                    "an unaccepted offer does not motivate work"
                );
                assert!(
                    broke_started,
                    "a claim can motivate work even when its maker has no goods"
                );
                let held = |world: &Economy| {
                    world
                        .snapshot()
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|holder| holder.place == Place::Person(2))
                        .unwrap()
                        .used
                };
                assert_eq!(
                    held(&willing_world),
                    1,
                    "a willing person can give real goods after witnessing the work"
                );
                assert_eq!(
                    held(&refuses_world),
                    0,
                    "the maker can choose not to follow through"
                );
                assert_eq!(held(&broke_world), 0, "a false claim cannot create goods");
                assert_eq!(
                    held(&reserve_world),
                    0,
                    "a personal reserve can prevent the gift"
                );
                assert!(
                    material_world
                        .snapshot()
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|holder| holder.place == Place::Person(2))
                        .unwrap()
                        .lots
                        .iter()
                        .any(|lot| lot.good == 2 && lot.quantity == 1),
                    "a non-food good can be genuinely transferred after the work"
                );
                let mut witnessed = None;
                for _ in 0..5 {
                    observation_world.advance().unwrap();
                    let snapshot = observation_world.snapshot();
                    let entry = snapshot
                        .giving
                        .as_ref()
                        .unwrap()
                        .practices
                        .as_ref()
                        .unwrap()
                        .minds[0]
                        .work
                        .as_ref()
                        .unwrap()
                        .engagement
                        .as_ref();
                    let memory = &snapshot
                        .autowork
                        .as_ref()
                        .unwrap()
                        .requests
                        .as_ref()
                        .unwrap()
                        .memories[0];
                    if entry.is_some_and(|entry| {
                        memory
                            .last_success
                            .is_some_and(|receipt| receipt.request == entry.request)
                    }) {
                        witnessed = Some(snapshot);
                        break;
                    }
                }
                let witnessed =
                    witnessed.expect("the requester personally observes the requested job");
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&witnessed).unwrap()).unwrap(),
                )
                .unwrap();
                let mut different_job = witnessed.clone();
                different_job
                    .autowork
                    .as_mut()
                    .unwrap()
                    .requests
                    .as_mut()
                    .unwrap()
                    .memories[0]
                    .last_success
                    .as_mut()
                    .unwrap()
                    .request = 1;
                let mut different_job = Economy::compile(different_job).unwrap();
                observation_world
                    .set_work_practice_choice(1, "willing", false)
                    .unwrap();
                restored
                    .set_work_practice_choice(1, "willing", false)
                    .unwrap();
                observation_world.advance().unwrap();
                restored.advance().unwrap();
                different_job.advance().unwrap();
                assert_eq!(
                    observation_world.snapshot(),
                    restored.snapshot(),
                    "a pending personal decision survives save and load"
                );
                assert_eq!(
                    held(&observation_world),
                    0,
                    "the maker can change their mind after work is completed"
                );
                observation_world
                    .set_work_practice_choice(1, "willing", true)
                    .unwrap();
                observation_world.advance().unwrap();
                assert_eq!(held(&observation_world), 1);
                assert_eq!(
                    held(&different_job),
                    0,
                    "another witnessed job cannot trigger this gift"
                );
                assert!(
                    observation_world
                        .snapshot()
                        .giving
                        .as_ref()
                        .unwrap()
                        .practices
                        .as_ref()
                        .unwrap()
                        .minds[0]
                        .work
                        .as_ref()
                        .unwrap()
                        .engagement
                        .is_none(),
                    "an accepted gift is not repeated"
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn people_invent_and_independently_act_on_a_conditional_food_practice() {
    use game_parallel_social::{Participant, Rules as SocialRules, Social};
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    initial.nutrition = None;
    for person in &mut initial.people {
        person.seek_meals = false;
        person.willing_to_work = false;
    }
    initial.people[0].seek_meals = true;
    initial.people[0].hunger = 5;
    for holder in &mut initial.physical.custody.containers {
        if holder.place == Place::Person(1) {
            *holder = Container::new(holder.place, 10, vec![]).unwrap();
        }
        if holder.place == Place::Person(2) {
            *holder = Container::new(
                holder.place,
                10,
                vec![Lot {
                    good: 1,
                    quantity: 6,
                    expires_at: Some(30),
                }],
            )
            .unwrap();
        }
    }
    initial.giving = Some(Giving {
        practices: Some(PersonalPractices {
            minds: vec![
                practice_mind(1, true),
                practice_mind(2, false),
                practice_mind(3, false),
            ],
        }),
        inherit_births: false,
        acquaintance_asking: vec![],
        learning: vec![],
        responses: vec![],
        policies: vec![GivingPolicy {
            person: 2,
            recipient: 1,
            good: 1,
            quantity: 2,
            recipient_hunger_at_least: 5,
            willing: true,
            requires_request: false,
            pressured_by: None,
            pressure_threshold: None,
        }],
        asking: vec![],
        requests: vec![],
    });
    initial.dynamics = Some(Dynamics {
        personality: None,
        treatment: vec![],
        claims: None,
        disease: None,
        social: Some(Social {
            rules: SocialRules {
                interval: 1,
                min_health: 1,
                max_hunger: 100,
                familiarity_gain: 1,
                familiarity_max: 10,
                relationship_retention: None,
            },
            participants: vec![
                Participant {
                    person: 1,
                    willing: true,
                },
                Participant {
                    person: 2,
                    willing: true,
                },
            ],
            pressures: vec![],
            relationships: vec![],
            events: vec![],
            needs: None,
        }),
    });
    initial.opinions.as_mut().unwrap().rules.fear_decay_per_tick = 0;
    let run = |workers, pressured: bool, decline: bool, poor: bool, stop: bool| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = initial.clone();
                let minds = &mut state
                    .giving
                    .as_mut()
                    .unwrap()
                    .practices
                    .as_mut()
                    .unwrap()
                    .minds;
                minds[0].value_learning = Some(std::sync::Arc::new(PracticeValueLearning {
                    value: 0,
                    minimum: -10,
                    maximum: 10,
                    received: 2,
                    gave: 0,
                    unanswered: 0,
                }));
                minds[1].value_learning = Some(std::sync::Arc::new(PracticeValueLearning {
                    value: 0,
                    minimum: -10,
                    maximum: 10,
                    received: 0,
                    gave: -1,
                    unanswered: 0,
                }));
                if poor {
                    let holder = state
                        .physical
                        .custody
                        .containers
                        .iter_mut()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap();
                    *holder = Container::new(
                        holder.place,
                        10,
                        vec![Lot {
                            good: 1,
                            quantity: 3,
                            expires_at: Some(30),
                        }],
                    )
                    .unwrap();
                }
                let receiver = &mut state
                    .giving
                    .as_mut()
                    .unwrap()
                    .practices
                    .as_mut()
                    .unwrap()
                    .minds[1];
                receiver.approve = !decline && !pressured;
                receiver.willing = !pressured;
                if !decline && !pressured {
                    receiver.proposal_affinity_weight = 1;
                    receiver.acceptance_threshold = 2;
                    state.opinions.as_mut().unwrap().records.insert(
                        0,
                        Opinion {
                            observer: 2,
                            target: 1,
                            trust: 0,
                            fear: 0,
                            obligation: 0,
                            affinity: 3,
                        },
                    );
                }
                if poor {
                    receiver.reporting = PracticeReporting::Lie;
                }
                if pressured {
                    receiver.pressure_threshold = Some(3);
                    state.opinions.as_mut().unwrap().records = vec![Opinion {
                        observer: 2,
                        target: 1,
                        trust: 0,
                        fear: 3,
                        obligation: 0,
                        affinity: 0,
                    }];
                }
                if poor {
                    let author = &mut state
                        .giving
                        .as_mut()
                        .unwrap()
                        .practices
                        .as_mut()
                        .unwrap()
                        .minds[0];
                    author.report_attitude = PracticeReportAttitude::TrustAtLeast(3);
                    author.require_believed_ready = true;
                    author.unanswered_report_belief = Some(PracticeBelief::Disbelieves);
                    state.opinions.as_mut().unwrap().records.insert(
                        0,
                        Opinion {
                            observer: 1,
                            target: 2,
                            trust: 4,
                            fear: 0,
                            obligation: 0,
                            affinity: 0,
                        },
                    );
                }
                let mut world = Economy::compile(state).unwrap();
                world.advance().unwrap();
                world.set_giving_choice(2, false).unwrap();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&world.snapshot()).unwrap())
                        .unwrap(),
                )
                .unwrap();
                let mut second_delivery = false;
                let mut reconsidered = false;
                let mut request_tick = None;
                for _ in 0..24 {
                    world.advance().unwrap();
                    restored.advance().unwrap();
                    assert_eq!(world.snapshot(), restored.snapshot());
                    let state = world.snapshot();
                    let practices = state.giving.as_ref().unwrap().practices.as_ref().unwrap();
                    if poor
                        && state
                            .giving
                            .as_ref()
                            .unwrap()
                            .requests
                            .iter()
                            .any(|request| request.requester == 1 && request.target == 2)
                    {
                        request_tick.get_or_insert(state.physical.custody.tick);
                    }
                    if stop
                        && !reconsidered
                        && practices.minds[1]
                            .inbox
                            .iter()
                            .any(|item| item.decision == PracticeDecision::Adopted)
                    {
                        world.set_practice_willingness(2, false).unwrap();
                        restored.set_practice_willingness(2, false).unwrap();
                        reconsidered = true;
                    }
                    if practices.minds[0]
                        .cases
                        .iter()
                        .filter(|case| matches!(case, PracticeCase::Received { helper: 2, .. }))
                        .count()
                        >= 2
                    {
                        second_delivery = true;
                        break;
                    }
                    if poor && request_tick.is_some_and(|tick| state.physical.custody.tick > tick) {
                        break;
                    }
                    if (decline || stop)
                        && practices.minds[1].inbox.iter().any(|item| {
                            item.decision != PracticeDecision::Pending
                                && state.physical.custody.tick >= item.delivered + 2
                        })
                    {
                        break;
                    }
                }
                let state = world.snapshot();
                let practices = state.giving.as_ref().unwrap().practices.as_ref().unwrap();
                assert!(
                    !practices.minds[0].authored.is_empty(),
                    "{:?}",
                    practices.minds[0]
                );
                assert!(
                    practices.minds[1]
                        .cases
                        .iter()
                        .any(|case| matches!(case, PracticeCase::Gave { recipient: 1, .. }))
                );
                assert!(practices.minds[2].inbox.is_empty());
                let received = &practices.minds[1].inbox[0];
                assert_eq!(
                    received.decision,
                    if decline {
                        PracticeDecision::Declined
                    } else {
                        PracticeDecision::Adopted
                    }
                );
                assert_eq!(received.approved, !decline && !pressured);
                assert_eq!(second_delivery, !decline && !poor && !stop);
                assert_eq!(
                    practices.minds[0].value_learning.as_ref().unwrap().value,
                    if second_delivery { 2 } else { 0 }
                );
                assert_eq!(
                    practices.minds[1].value_learning.as_ref().unwrap().value,
                    if second_delivery { -1 } else { 0 }
                );
                if poor {
                    assert!(request_tick.is_some());
                    let report = &practices.minds[0].reports[0];
                    assert!(report.statement.claimed_ready);
                    assert_eq!(report.belief, PracticeBelief::Believes);
                    let holder = state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap();
                    assert_eq!(holder.used, 1);
                    assert!(practices.minds[2].reports.is_empty());
                }
                state
            })
    };
    assert_eq!(
        run(1, false, false, false, false),
        run(4, false, false, false, false)
    );
    assert_eq!(
        run(1, true, false, false, false),
        run(4, true, false, false, false)
    );
    let declined = run(1, false, true, false, false);
    assert_eq!(declined, run(4, false, true, false, false));
    // The helper's actual earlier gift becomes a precedent for a later personal decision.
    for (use_precedent, expected) in [
        (false, PracticeDecision::Declined),
        (true, PracticeDecision::Adopted),
    ] {
        let mut state = declined.clone();
        let mind = &mut state
            .giving
            .as_mut()
            .unwrap()
            .practices
            .as_mut()
            .unwrap()
            .minds[1];
        mind.approve = true;
        mind.willing = false;
        mind.acceptance_threshold = 2;
        mind.precedent = use_precedent.then(|| {
            std::sync::Arc::new(PracticePrecedent {
                same_person: 2,
                same_good: 2,
                quantity_difference: -1,
                gave: 3,
                ..Default::default()
            })
        });
        let item = std::sync::Arc::make_mut(&mut mind.inbox)
            .first_mut()
            .unwrap();
        item.decision = PracticeDecision::Pending;
        item.approved = false;
        let mut world = Economy::compile(state).unwrap();
        world.advance().unwrap();
        assert_eq!(
            world.personal_practice(2).unwrap().inbox[0].decision,
            expected
        );
        assert!(!world.personal_practice(2).unwrap().willing);
    }
    let mut coerced = declined;
    let mind = &mut coerced
        .giving
        .as_mut()
        .unwrap()
        .practices
        .as_mut()
        .unwrap()
        .minds[1];
    mind.willing = false;
    mind.pressure_threshold = Some(3);
    coerced.opinions.as_mut().unwrap().records = vec![Opinion {
        observer: 2,
        target: 1,
        trust: 0,
        fear: 3,
        obligation: 0,
        affinity: 0,
    }];
    let mut coerced = Economy::compile(coerced).unwrap();
    coerced.advance().unwrap();
    assert!(
        coerced
            .personal_practice(1)
            .unwrap()
            .cases
            .iter()
            .filter(|case| matches!(case, PracticeCase::Received { helper: 2, .. }))
            .count()
            >= 2
    );
    assert_eq!(
        coerced.personal_practice(2).unwrap().inbox[0].decision,
        PracticeDecision::Declined
    );
    assert_eq!(
        coerced
            .personal_practice(1)
            .unwrap()
            .value_learning
            .as_ref()
            .unwrap()
            .value,
        2
    );
    assert_eq!(
        coerced
            .personal_practice(2)
            .unwrap()
            .value_learning
            .as_ref()
            .unwrap()
            .value,
        -1
    );
    let poor = run(1, false, false, true, false);
    assert_eq!(poor, run(4, false, false, true, false));
    let mut revising = Economy::compile(poor).unwrap();
    for _ in 0..15 {
        if revising
            .personal_practice(1)
            .unwrap()
            .authored
            .iter()
            .any(|proposal| proposal.sequence == 2)
        {
            break;
        }
        revising.advance().unwrap();
    }
    let author = revising.personal_practice(1).unwrap();
    assert!(
        author
            .cases
            .iter()
            .any(|case| matches!(case, PracticeCase::Unanswered { target: 2, .. }))
    );
    assert_eq!(author.reports[0].belief, PracticeBelief::Disbelieves);
    let revised = author
        .authored
        .iter()
        .find(|proposal| proposal.sequence == 2)
        .unwrap();
    assert_eq!((revised.recipient, revised.quantity), (2, 1));
    assert_eq!(
        run(1, false, false, false, true),
        run(4, false, false, false, true)
    );
    let mut overflow = initial.clone();
    overflow.giving.as_mut().unwrap().policies[0].willing = false;
    let mind = &mut overflow
        .giving
        .as_mut()
        .unwrap()
        .practices
        .as_mut()
        .unwrap()
        .minds[0];
    mind.sequence = u64::MAX;
    mind.cases = vec![
        PracticeCase::Shortage { tick: 0, hunger: 5 },
        PracticeCase::Shortage { tick: 0, hunger: 6 },
        PracticeCase::Received {
            tick: 0,
            helper: 2,
            good: 1,
            quantity: 1,
        },
    ]
    .into();
    let mut failing = Economy::compile(overflow).unwrap();
    let before = failing.snapshot();
    assert!(failing.advance().is_err());
    assert_eq!(failing.snapshot(), before);
    let mut state = initial;
    let holder = state
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(2))
        .unwrap();
    *holder = Container::new(
        holder.place,
        10,
        vec![Lot {
            good: 1,
            quantity: 3,
            expires_at: Some(30),
        }],
    )
    .unwrap();
    state
        .giving
        .as_mut()
        .unwrap()
        .practices
        .as_mut()
        .unwrap()
        .minds[0]
        .require_believed_ready = true;
    let mut world = Economy::compile(state).unwrap();
    world.advance().unwrap();
    world.set_giving_choice(2, false).unwrap();
    for _ in 0..20 {
        if world
            .personal_practice(2)
            .unwrap()
            .inbox
            .iter()
            .any(|item| item.decision == PracticeDecision::Adopted)
        {
            break;
        }
        world.advance().unwrap();
    }
    let mut state = world.snapshot();
    state.people[1].controlled = true;
    let mut forgotten = state.clone();
    let author = &mut forgotten
        .giving
        .as_mut()
        .unwrap()
        .practices
        .as_mut()
        .unwrap()
        .minds[0];
    std::sync::Arc::make_mut(&mut author.authored).clear();
    author.invent = false;
    let holder = forgotten
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|holder| holder.place == Place::Person(2))
        .unwrap();
    *holder = Container::new(Place::Person(2), holder.capacity, vec![]).unwrap();
    let report_after_forgetting = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(forgotten.clone()).unwrap();
                assert!(world.report_practice(2, 1, 1, true).unwrap());
                assert!(
                    world.personal_practice(1).unwrap().reports[0]
                        .statement
                        .claimed_ready
                );
                assert!(world.personal_practice(1).unwrap().authored.is_empty());
                assert_eq!(
                    world
                        .snapshot()
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|holder| holder.place == Place::Person(2))
                        .unwrap()
                        .used,
                    0
                );
                let saved = world.snapshot();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world
                    .assess_practice_report(1, 2, 1, PracticeBelief::Believes)
                    .unwrap();
                restored
                    .assess_practice_report(1, 2, 1, PracticeBelief::Believes)
                    .unwrap();
                assert!(world.report_practice(2, 1, 1, false).unwrap());
                assert!(restored.report_practice(2, 1, 1, false).unwrap());
                let report = &world.personal_practice(1).unwrap().reports[0];
                assert!(!report.statement.claimed_ready);
                assert_eq!(report.belief, PracticeBelief::Unassessed);
                assert_eq!(world.personal_practice(2).unwrap().sent_reports.len(), 1);
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(report_after_forgetting(1), report_after_forgetting(4));
    let mut world = Economy::compile(state).unwrap();
    let before = world.snapshot();
    assert!(world.report_practice(2, 1, 999, true).is_err());
    assert_eq!(world.snapshot(), before);
    assert!(world.report_practice(2, 1, 1, true).unwrap());
    let report = &world.personal_practice(1).unwrap().reports[0];
    assert!(report.statement.claimed_ready);
    assert_eq!(report.belief, PracticeBelief::Unassessed);
    world
        .assess_practice_report(1, 2, 1, PracticeBelief::Disbelieves)
        .unwrap();
    assert_eq!(
        world.personal_practice(1).unwrap().reports[0].belief,
        PracticeBelief::Disbelieves
    );
    world.withdraw_practice(2, 1, 1).unwrap();
    assert_eq!(
        world.personal_practice(2).unwrap().inbox[0].decision,
        PracticeDecision::Declined
    );
    assert!(
        world.personal_practice(1).unwrap().reports[0]
            .statement
            .claimed_ready
    );
    assert_eq!(
        world
            .snapshot()
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(2))
            .unwrap()
            .used,
        1
    );
}

#[test]
fn many_people_record_only_their_own_shortages_across_workers() {
    let initial = concentrated_practice_shortages(20_000);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                let state = world.snapshot();
                assert!(
                    state
                        .giving
                        .as_ref()
                        .unwrap()
                        .practices
                        .as_ref()
                        .unwrap()
                        .minds
                        .iter()
                        .all(|mind| mind.cases.len() == 3
                            && mind
                                .cases
                                .iter()
                                .all(|case| matches!(case, PracticeCase::Shortage { .. })))
                );
                let restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                assert_eq!(state, restored.snapshot());
                state
            })
    };
    assert_eq!(run(1), run(4));
}

fn concentrated_practice_shortages(count: u64) -> State {
    let mut state = field_workload(count);
    state.autowork = None;
    state.agriculture = None;
    state.opinions = None;
    state.nutrition = None;
    for person in &mut state.people {
        person.hunger = 6;
        person.seek_meals = false;
    }
    for holder in &mut state.physical.custody.containers {
        if matches!(holder.place, Place::Person(_)) {
            *holder = Container::new(holder.place, 10, vec![]).unwrap();
        }
    }
    state.giving = Some(Giving {
        practices: Some(PersonalPractices {
            minds: (1..=count)
                .map(|person| practice_mind(person, false))
                .collect(),
        }),
        inherit_births: false,
        policies: vec![],
        responses: vec![],
        learning: vec![],
        asking: vec![],
        requests: vec![],
        acquaintance_asking: vec![],
    });
    state
}

#[test]
#[ignore = "performance measurement"]
fn concentrated_practice_shortage_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = concentrated_practice_shortages(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                let elapsed = started.elapsed().as_secs_f64();
                assert!(
                    world
                        .snapshot()
                        .giving
                        .unwrap()
                        .practices
                        .unwrap()
                        .minds
                        .iter()
                        .all(|mind| mind.cases.len() == 3)
                );
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=3 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn personal_giving_moves_goods_and_fear_can_motivate_help() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    initial.nutrition = None;
    for p in &mut initial.people {
        p.seek_meals = false;
    }
    initial.people[2].hunger = 5;
    let opinions = initial.opinions.as_mut().unwrap();
    opinions.rules.gratitude_gain = 2;
    opinions.records = vec![Opinion {
        observer: 4,
        target: 1,
        trust: 0,
        fear: 5,
        obligation: 0,
        affinity: 0,
    }];
    initial.giving = Some(Giving {
        practices: None,
        inherit_births: false,
        acquaintance_asking: vec![],
        learning: vec![],
        responses: vec![],
        policies: vec![GivingPolicy {
            person: 4,
            recipient: 3,
            good: 1,
            quantity: 1,
            recipient_hunger_at_least: 5,
            willing: false,
            requires_request: false,
            pressured_by: Some(1),
            pressure_threshold: Some(3),
        }],
        asking: vec![],
        requests: vec![],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let saved = world.snapshot();
                let held = |id| {
                    saved
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(id))
                        .unwrap()
                        .used
                };
                assert_eq!((held(4), held(3)), (0, 1));
                assert!(
                    saved
                        .opinions
                        .as_ref()
                        .unwrap()
                        .records
                        .iter()
                        .any(|o| o.observer == 3 && o.target == 4 && o.obligation == 2)
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
    initial.opinions.as_mut().unwrap().records[0].fear = 0;
    let mut world = Economy::compile(initial).unwrap();
    let stale = world.prepare().unwrap();
    world.set_giving_choice(4, true).unwrap();
    assert!(world.commit(stale).is_err());
    world.set_giving_choice(4, false).unwrap();
    world.advance().unwrap();
    assert_eq!(
        world
            .snapshot()
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(3))
            .unwrap()
            .used,
        0
    );
}

#[test]
fn controlled_person_can_give_without_a_promise_and_only_real_delivery_counts() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    initial.nutrition = None;
    for p in &mut initial.people {
        p.seek_meals = false;
    }
    initial.people[0].controlled = true;
    initial.people[1].hunger = 5;
    initial.opinions.as_mut().unwrap().rules.gratitude_gain = 2;
    let mut world = Economy::compile(initial.clone()).unwrap();
    assert!(world.give(1, 2, 1, 1).unwrap());
    let state = world.snapshot();
    assert_eq!(
        state
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(2))
            .unwrap()
            .used,
        1
    );
    assert!(
        state
            .opinions
            .as_ref()
            .unwrap()
            .records
            .iter()
            .any(|o| o.observer == 2 && o.target == 1 && o.obligation == 2)
    );
    assert!(!world.give(1, 2, 1, 1).unwrap());
    assert_eq!(
        world
            .snapshot()
            .opinions
            .unwrap()
            .records
            .iter()
            .find(|o| o.observer == 2 && o.target == 1)
            .unwrap()
            .obligation,
        2
    );
    let source = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(1))
        .unwrap();
    *source = Container::new(
        source.place,
        source.capacity,
        vec![Lot {
            good: 1,
            quantity: 1,
            expires_at: Some(1),
        }],
    )
    .unwrap();
    let mut world = Economy::compile(initial).unwrap();
    assert!(!world.give(1, 2, 1, 1).unwrap());
    assert!(
        !world
            .snapshot()
            .opinions
            .unwrap()
            .records
            .iter()
            .any(|o| o.observer == 2 && o.target == 1)
    );
}

fn giving_workload(count: u64) -> State {
    let mut state = field_workload(count);
    state.agriculture = None;
    state.rules.carrying_capacity = count / 2;
    for p in &mut state.people {
        p.seek_meals = false;
        p.hunger = if p.id == 1 { 5 } else { 0 };
    }
    for holder in &mut state.physical.custody.containers {
        if let Place::Person(id) = holder.place {
            let lots = if id == 1 {
                vec![]
            } else {
                vec![Lot {
                    good: 1,
                    quantity: 1,
                    expires_at: Some(10),
                }]
            };
            *holder = Container::new(holder.place, count / 2, lots).unwrap();
        }
    }
    state.giving = Some(Giving {
        practices: None,
        inherit_births: false,
        acquaintance_asking: vec![],
        learning: vec![],
        responses: vec![],
        policies: (2..=count)
            .map(|person| GivingPolicy {
                person,
                recipient: 1,
                good: 1,
                quantity: 1,
                recipient_hunger_at_least: 5,
                willing: true,
                requires_request: false,
                pressured_by: None,
                pressure_threshold: None,
            })
            .collect(),
        asking: vec![],
        requests: vec![],
    });
    state
}

#[test]
fn concentrated_gifts_share_actual_recipient_capacity_across_workers() {
    let count = 4096;
    let initial = giving_workload(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let events = world.advance().unwrap();
                assert_eq!(
                    events
                        .physical_actions
                        .iter()
                        .filter(|d| d.accepted)
                        .count(),
                    count as usize / 2
                );
                let state = world.snapshot();
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(1))
                        .unwrap()
                        .used,
                    count / 2
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn every_successful_gift_can_expose_its_giver_to_an_infectious_recipient() {
    let mut initial = giving_workload(64);
    initial.dynamics = Some(infection_from(1));
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let events = world.advance().unwrap();
                let mut exposed: Vec<_> = events
                    .physical_movements
                    .iter()
                    .filter_map(|receipt| {
                        match (
                            receipt.movement.accepted,
                            receipt.request.from,
                            receipt.request.to,
                        ) {
                            (true, Place::Person(giver), Some(Place::Person(1))) => Some(giver),
                            _ => None,
                        }
                    })
                    .collect();
                exposed.sort_unstable();
                assert_eq!(exposed.len(), 32);
                let state = world.snapshot();
                let cases = &state
                    .dynamics
                    .as_ref()
                    .unwrap()
                    .disease
                    .as_ref()
                    .unwrap()
                    .cases;
                let mut infected: Vec<_> = cases
                    .iter()
                    .filter(|case| case.source == Some(1))
                    .map(|case| case.person)
                    .collect();
                infected.sort_unstable();
                assert_eq!(infected, exposed);
                let restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                assert_eq!(state, restored.snapshot());
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn food_request_can_be_refused_then_independently_fulfilled_and_survives_reload() {
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    initial.nutrition = None;
    for p in &mut initial.people {
        p.seek_meals = false;
    }
    initial.people[2].hunger = 5;
    initial.giving = Some(Giving {
        practices: None,
        inherit_births: false,
        acquaintance_asking: vec![],
        learning: vec![],
        responses: vec![],
        policies: vec![GivingPolicy {
            person: 4,
            recipient: 3,
            good: 1,
            quantity: 1,
            recipient_hunger_at_least: 5,
            willing: false,
            requires_request: true,
            pressured_by: None,
            pressure_threshold: None,
        }],
        asking: vec![FoodAsking {
            person: 3,
            target: 4,
            alternatives: vec![],
            next_target: 0,
            min_hunger: 5,
            enabled: true,
            retry_ticks: 8,
            request_lifetime: 4,
            next_review: 0,
        }],
        requests: vec![],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                assert_eq!(
                    world.food_requests(),
                    &[FoodRequest {
                        requester: 3,
                        target: 4,
                        expires_at: 5
                    }]
                );
                let saved = world.snapshot();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                assert_eq!(world.food_requests().len(), 1);
                let draft = world.prepare().unwrap();
                world.set_giving_choice(4, true).unwrap();
                assert!(world.commit(draft).is_err());
                world.advance().unwrap();
                assert!(world.food_requests().is_empty());
                let saved = world.snapshot();
                let recipient = saved
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|c| c.place == Place::Person(3))
                    .unwrap();
                assert_eq!(recipient.lots[0].good, 1);
                assert_eq!(recipient.lots[0].quantity, 1);
                saved
            })
    };
    assert_eq!(run(1), run(4));
    initial.giving.as_mut().unwrap().asking[0].request_lifetime = 1;
    let mut world = Economy::compile(initial).unwrap();
    world.advance().unwrap();
    world.advance().unwrap();
    world.advance().unwrap();
    assert!(world.food_requests().is_empty());
    world.set_giving_choice(4, true).unwrap();
    world.advance().unwrap();
    assert!(
        world
            .snapshot()
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(3))
            .unwrap()
            .lots
            .is_empty()
    );
}

#[test]
fn an_unanswered_food_request_leads_to_another_known_person() {
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    initial.nutrition = None;
    for p in &mut initial.people {
        p.seek_meals = false;
    }
    initial.people[2].hunger = 5;
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(2))
        .unwrap();
    *holder = Container::new(
        holder.place,
        holder.capacity,
        vec![Lot {
            good: 1,
            quantity: 1,
            expires_at: Some(10),
        }],
    )
    .unwrap();
    initial.giving = Some(Giving {
        practices: None,
        inherit_births: false,
        acquaintance_asking: vec![],
        learning: vec![],
        policies: vec![],
        responses: vec![FoodResponse {
            person: 2,
            good: 1,
            quantity: 1,
            recipient_hunger_at_least: 5,
            willing: true,
            minimum_trust: None,
            pressure_threshold: None,
            affinity_threshold: None,
            affinity_at_most: None,
            affinity_weight: None,
        }],
        asking: vec![FoodAsking {
            person: 3,
            target: 4,
            alternatives: vec![2],
            next_target: 0,
            min_hunger: 5,
            enabled: true,
            retry_ticks: 1,
            request_lifetime: 1,
            next_review: 0,
        }],
        requests: vec![],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                assert_eq!(world.food_requests()[0].target, 4);
                world.advance().unwrap();
                let saved = world.snapshot();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.food_requests()[0].target, 2);
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert!(world.food_requests().is_empty());
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(3))
                        .unwrap()
                        .used,
                    1
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

fn food_asking_workload(count: u64) -> State {
    let mut state = field_workload(count);
    state.agriculture = None;
    for p in &mut state.people {
        p.hunger = 5;
    }
    for holder in &mut state.physical.custody.containers {
        if matches!(holder.place, Place::Person(_)) {
            *holder = Container::new(holder.place, holder.capacity, vec![]).unwrap();
        }
    }
    state.giving = Some(Giving {
        practices: None,
        inherit_births: false,
        acquaintance_asking: vec![],
        learning: vec![],
        responses: vec![],
        policies: vec![],
        requests: vec![],
        asking: (2..=count)
            .map(|person| FoodAsking {
                person,
                target: 1,
                alternatives: vec![],
                next_target: 0,
                min_hunger: 5,
                enabled: true,
                retry_ticks: 8,
                request_lifetime: 4,
                next_review: 0,
            })
            .collect(),
    });
    state
}

#[test]
fn concentrated_food_requests_remain_individual_and_deterministic() {
    let initial = food_asking_workload(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                assert_eq!(world.food_requests().len(), 4095);
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

fn food_response_workload(count: u64) -> State {
    let mut state = food_asking_workload(count);
    let giving = state.giving.as_mut().unwrap();
    giving.asking.clear();
    giving.requests = (2..=count)
        .map(|requester| FoodRequest {
            requester,
            target: 1,
            expires_at: 10,
        })
        .collect();
    giving.responses = vec![FoodResponse {
        person: 1,
        good: 1,
        quantity: 1,
        recipient_hunger_at_least: 5,
        willing: true,
        minimum_trust: None,
        pressure_threshold: None,
        affinity_threshold: None,
        affinity_at_most: None,
        affinity_weight: None,
    }];
    let holder = state
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(
        holder.place,
        holder.capacity,
        vec![Lot {
            good: 1,
            quantity: 2,
            expires_at: Some(10),
        }],
    )
    .unwrap();
    state
}

#[test]
fn a_helper_independently_chooses_between_many_actual_requests() {
    let initial = food_response_workload(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let events = world.advance().unwrap();
                assert_eq!(
                    events
                        .physical_actions
                        .iter()
                        .filter(|e| e.accepted)
                        .count(),
                    1
                );
                assert_eq!(world.food_requests().len(), 4094);
                let saved = world.snapshot();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                assert_eq!(world.food_requests().len(), 4093);
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
    let mut declining = Economy::compile(initial).unwrap();
    let stale = declining.prepare().unwrap();
    declining.set_giving_choice(1, false).unwrap();
    assert!(declining.commit(stale).is_err());
    declining.advance().unwrap();
    assert_eq!(declining.food_requests().len(), 4095);
}

#[test]
fn fear_of_one_requester_can_motivate_help_without_general_willingness() {
    let mut initial = food_response_workload(4);
    let response = &mut initial.giving.as_mut().unwrap().responses[0];
    response.willing = false;
    response.minimum_trust = Some(5);
    response.pressure_threshold = Some(3);
    let mut opinions = fixture().opinions.unwrap();
    opinions.records = vec![Opinion {
        observer: 1,
        target: 2,
        trust: 0,
        fear: 5,
        obligation: 0,
        affinity: 0,
    }];
    initial.opinions = Some(opinions);
    let mut world = Economy::compile(initial).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    assert_eq!(
        state
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(2))
            .unwrap()
            .used,
        1
    );
    assert_eq!(world.food_requests().len(), 2);
    world.advance().unwrap();
    assert_eq!(world.food_requests().len(), 2);
}

#[test]
#[ignore]
fn concentrated_food_response_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = food_response_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn giving_nonfood_does_not_satisfy_a_food_request() {
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    initial.people[3].controlled = true;
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(4))
        .unwrap();
    *holder = Container::new(
        holder.place,
        holder.capacity,
        vec![Lot {
            good: 3,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    initial.giving = Some(Giving {
        practices: None,
        inherit_births: false,
        acquaintance_asking: vec![],
        learning: vec![],
        responses: vec![],
        policies: vec![],
        asking: vec![],
        requests: vec![FoodRequest {
            requester: 3,
            target: 4,
            expires_at: 5,
        }],
    });
    let mut world = Economy::compile(initial).unwrap();
    assert!(world.give(4, 3, 3, 1).unwrap());
    assert_eq!(world.food_requests().len(), 1);
}

#[test]
#[ignore]
fn concentrated_food_requests_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = food_asking_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
#[ignore]
fn concentrated_giving_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let mut initial = giving_workload(count);
    let disease = std::env::var("LEGACY_DISEASE").is_ok_and(|value| value == "1");
    if disease {
        initial.dynamics = Some(infection_from(1));
    }
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                let elapsed = started.elapsed().as_secs_f64();
                if disease {
                    assert_eq!(
                        world
                            .snapshot()
                            .dynamics
                            .unwrap()
                            .disease
                            .unwrap()
                            .cases
                            .len(),
                        count as usize / 2 + 1
                    );
                }
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} disease={disease} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

fn giving_with_gratitude_workload(count: u64) -> State {
    let mut state = giving_workload(count);
    let mut opinions = fixture().opinions.unwrap();
    opinions.rules.gratitude_gain = 1;
    opinions.records.clear();
    state.opinions = Some(opinions);
    state
}

#[test]
fn concentrated_gifts_learn_only_from_actual_delivery_across_workers() {
    let count = 4096;
    let initial = giving_with_gratitude_workload(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state.opinions.as_ref().unwrap().records.len(),
                    count as usize / 2
                );
                assert!(
                    state
                        .opinions
                        .as_ref()
                        .unwrap()
                        .records
                        .iter()
                        .all(|o| o.observer == 1 && o.obligation == 1)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_giving_with_gratitude_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = giving_with_gratitude_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn hungry_people_compete_for_local_food_and_eat_only_after_receiving_it() {
    let mut initial = fixture();
    initial.market = None;
    initial.agreements = None;
    initial.autowork = None;
    initial.people[0].hunger = 5;
    initial.people[1].hunger = 5;
    initial.people[0].seek_meals = true;
    initial.people[1].seek_meals = true;
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        Place::Stock { region: 1, good: 1 },
        10,
        vec![Lot {
            good: 1,
            quantity: 1,
            expires_at: Some(3),
        }],
    )
    .unwrap();
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(Place::Person(1), 10, vec![]).unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let carriers: Vec<_> = state
                    .physical
                    .custody
                    .containers
                    .iter()
                    .filter(|row| matches!(row.place, Place::Person(1 | 2)) && !row.lots.is_empty())
                    .collect();
                assert_eq!(carriers.len(), 1);
                assert_eq!(carriers[0].lots[0].expires_at, Some(3));
                assert_eq!(state.people[0].hunger, 6);
                assert_eq!(state.people[1].hunger, 6);
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state
                        .people
                        .iter()
                        .take(2)
                        .filter(|row| row.hunger == 2)
                        .count(),
                    1
                );
                assert_eq!(
                    state
                        .people
                        .iter()
                        .take(2)
                        .filter(|row| row.hunger == 7)
                        .count(),
                    1
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn journeys_keep_possessions_and_expiry_and_resume_without_teleporting() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    initial.people[0].seek_meals = false;
    let carrying = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Person(1))
        .unwrap();
    *carrying = Container::new(
        Place::Person(1),
        10,
        vec![Lot {
            good: 1,
            quantity: 1,
            expires_at: Some(2),
        }],
    )
    .unwrap();
    initial
        .physical
        .custody
        .containers
        .push(Container::new(Place::Stock { region: 2, good: 1 }, 10, vec![]).unwrap());
    initial
        .physical
        .custody
        .containers
        .sort_unstable_by_key(|row| row.place);
    initial.travel = Some(Travel {
        routes: vec![Route {
            source: 1,
            destination: 2,
            ticks: 3,
        }],
        journeys: vec![],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let prepared = world.prepare().unwrap();
                let before = world.snapshot();
                assert!(
                    world
                        .depart(&[
                            Departure {
                                person: 1,
                                destination: 2
                            },
                            Departure {
                                person: 2,
                                destination: 99
                            }
                        ])
                        .is_err()
                );
                assert_eq!(before, world.snapshot());
                world
                    .depart(&[Departure {
                        person: 1,
                        destination: 2,
                    }])
                    .unwrap();
                assert!(world.commit(prepared).is_err());
                world.advance().unwrap();
                let saved = world.snapshot();
                assert_eq!(saved.people[0].region, None);
                assert_eq!(saved.travel.as_ref().unwrap().journeys[0].remaining, 2);
                let mut world = Economy::compile(saved).unwrap();
                assert_eq!(world.advance().unwrap().spoiled.len(), 1);
                assert_eq!(world.snapshot().people[0].region, None);
                world.advance().unwrap();
                let result = world.snapshot();
                assert_eq!(result.people[0].region, Some(2));
                assert!(result.travel.as_ref().unwrap().journeys.is_empty());
                assert!(
                    result
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Person(1))
                        .unwrap()
                        .lots
                        .is_empty()
                );
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn hungry_person_travels_on_their_own_stale_report_and_learns_on_arrival() {
    use game_parallel_economy_knowledge::{Mind, Record};
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    initial.nutrition = None;
    initial.people[0].seek_meals = false;
    initial.people[1].hunger = 5;
    initial.people[1].seek_meals = true;
    initial
        .physical
        .custody
        .containers
        .push(Container::new(Place::Stock { region: 2, good: 1 }, 10, vec![]).unwrap());
    initial
        .physical
        .custody
        .containers
        .sort_unstable_by_key(|c| c.place);
    initial.travel = Some(Travel {
        routes: vec![Route {
            source: 1,
            destination: 2,
            ticks: 2,
        }],
        journeys: vec![],
    });
    initial.knowledge = Some(Knowledge {
        max_records: 4,
        minds: vec![Mind {
            person: 2,
            misreport: None,
            records: vec![Record {
                region: 2,
                food: 10,
                observed_at: 0,
                witness: 1,
                learned_from: Some(1),
            }],
        }],
    });
    initial.food_search = Some(FoodSearch {
        inherit_births: false,
        min_hunger: 5,
        min_report_food: 1,
        max_report_age: 20,
        retry_ticks: 3,
        people: vec![FoodSearchPreference {
            person: 2,
            enabled: true,
            next_review: 0,
            target: None,
        }],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let saved = world.snapshot();
                assert_eq!(saved.people[1].region, None);
                assert_eq!(saved.travel.as_ref().unwrap().journeys[0].remaining, 2);
                assert_eq!(saved.food_search.as_ref().unwrap().people[0].next_review, 4);
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                for _ in 0..3 {
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(state.people[1].region, Some(2));
                assert!(state.people[1].hunger > 5);
                assert_eq!(
                    state.knowledge.as_ref().unwrap().minds[0]
                        .records
                        .iter()
                        .find(|r| r.region == 2)
                        .unwrap()
                        .food,
                    0
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
    let mut overflow = initial.clone();
    overflow.food_search.as_mut().unwrap().retry_ticks = u64::MAX;
    let mut world = Economy::compile(overflow).unwrap();
    let before = world.snapshot();
    assert!(world.advance().is_err());
    assert_eq!(world.snapshot(), before);
    let mut changed = Economy::compile(initial.clone()).unwrap();
    let stale = changed.prepare().unwrap();
    changed.set_food_search_choice(2, false).unwrap();
    assert!(changed.commit(stale).is_err());
    changed.advance().unwrap();
    assert_eq!(changed.snapshot().people[1].region, Some(1));
    initial.knowledge.as_mut().unwrap().minds[0].records.clear();
    let remote = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 2, good: 1 })
        .unwrap();
    *remote = Container::new(
        remote.place,
        remote.capacity,
        vec![Lot {
            good: 1,
            quantity: 5,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut with_report = initial.clone();
    with_report.knowledge.as_mut().unwrap().minds[0]
        .records
        .push(Record {
            region: 2,
            food: 5,
            observed_at: 0,
            witness: 1,
            learned_from: Some(1),
        });
    let mut world = Economy::compile(initial).unwrap();
    world.advance().unwrap();
    assert_eq!(world.snapshot().people[1].region, Some(1));
    let mut world = Economy::compile(with_report).unwrap();
    for _ in 0..4 {
        world.advance().unwrap();
    }
    assert_eq!(
        world
            .snapshot()
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(2))
            .unwrap()
            .used,
        1
    );
    let before = world.snapshot().people[1].hunger;
    world.advance().unwrap();
    assert!(world.snapshot().people[1].hunger < before);
}

fn food_search_workload(count: u64) -> State {
    use game_parallel_economy_knowledge::{Mind, Record};
    let mut state = field_workload(count);
    state.agriculture = None;
    for person in &mut state.people {
        person.hunger = 5;
        person.seek_meals = true;
    }
    for container in &mut state.physical.custody.containers {
        if matches!(container.place, Place::Person(_)) {
            *container = Container::new(container.place, container.capacity, vec![]).unwrap();
        }
    }
    state
        .physical
        .custody
        .containers
        .push(Container::new(Place::Stock { region: 2, good: 1 }, count * 2, vec![]).unwrap());
    state
        .physical
        .custody
        .containers
        .sort_unstable_by_key(|c| c.place);
    state.travel = Some(Travel {
        routes: vec![Route {
            source: 1,
            destination: 2,
            ticks: 2,
        }],
        journeys: vec![],
    });
    state.knowledge = Some(Knowledge {
        max_records: 4,
        minds: (1..=count)
            .map(|person| Mind {
                person,
                misreport: None,
                records: vec![Record {
                    region: 2,
                    food: 10,
                    observed_at: 0,
                    witness: person,
                    learned_from: None,
                }],
            })
            .collect(),
    });
    state.food_search = Some(FoodSearch {
        inherit_births: false,
        min_hunger: 5,
        min_report_food: 1,
        max_report_age: 20,
        retry_ticks: 3,
        people: (1..=count)
            .map(|person| FoodSearchPreference {
                person,
                enabled: true,
                next_review: 0,
                target: None,
            })
            .collect(),
    });
    state
}

#[test]
fn concentrated_personal_food_search_matches_across_workers() {
    let count = 4096;
    let initial = food_search_workload(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state.travel.as_ref().unwrap().journeys.len(),
                    count as usize
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn food_search_follows_multiple_legs_and_remembers_its_personal_destination() {
    let mut initial = food_search_workload(4);
    initial.physical.custody.containers.push(
        Container::new(
            Place::Stock { region: 3, good: 1 },
            10,
            vec![Lot {
                good: 1,
                quantity: 4,
                expires_at: Some(20),
            }],
        )
        .unwrap(),
    );
    initial
        .physical
        .custody
        .containers
        .sort_unstable_by_key(|c| c.place);
    initial.travel.as_mut().unwrap().routes.push(Route {
        source: 2,
        destination: 3,
        ticks: 1,
    });
    for mind in &mut initial.knowledge.as_mut().unwrap().minds {
        mind.records[0].region = 3;
    }
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert!(
                    state
                        .travel
                        .as_ref()
                        .unwrap()
                        .journeys
                        .iter()
                        .all(|j| j.destination == 2)
                );
                assert!(
                    state
                        .food_search
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .all(|p| p.target == Some(3))
                );
                world.advance().unwrap();
                world.advance().unwrap();
                assert!(world.snapshot().people.iter().all(|p| p.region == Some(2)));
                let saved = world.snapshot();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert!(
                    world
                        .snapshot()
                        .travel
                        .as_ref()
                        .unwrap()
                        .journeys
                        .iter()
                        .all(|j| j.destination == 3)
                );
                for _ in 0..2 {
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert!(state.people.iter().all(|p| p.region == Some(3)));
                assert!(
                    state
                        .food_search
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .all(|p| p.target.is_none())
                );
                assert!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .filter(|c| matches!(c.place, Place::Person(_)))
                        .all(|c| c.used == 1)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
    let mut no_report = initial;
    no_report.knowledge.as_mut().unwrap().minds[0]
        .records
        .clear();
    let mut world = Economy::compile(no_report).unwrap();
    world.advance().unwrap();
    assert_eq!(world.snapshot().people[0].region, Some(1));
}

#[test]
#[ignore]
fn concentrated_food_search_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = food_search_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn newborn_gets_custody_in_the_same_generation_as_contested_food() {
    use game_parallel_dependent_care::{Policy as CarePolicy, Rules as CareRules};
    use game_parallel_families::{
        Families, FamilyRules, FamilyTotals, Genome, Member, Pregnancy, Sex,
    };
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![WorkPolicy {
            person: 3,
            recipe: 1,
            enabled: true,
            disregard_claims: false,
        }],
        requests: None,
    });
    initial.people[2].willing_to_work = true;
    initial.people[0].controlled = true;
    initial.people[1].seek_meals = true;
    initial.people[1].hunger = 5;
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        Place::Stock { region: 1, good: 1 },
        10,
        vec![Lot {
            good: 1,
            quantity: 1,
            expires_at: Some(9),
        }],
    )
    .unwrap();
    let members = (1..=4)
        .map(|id| Member {
            id,
            born: None,
            age: if id == 3 { 9 } else { 20 },
            lifespan_loss: 0,
            genome: Genome {
                sex: if id == 1 { Sex::Female } else { Sex::Male },
                alleles: [[500; 2]; 4],
            },
            parents: Default::default(),
            partner: match id {
                1 => Some(2),
                2 => Some(1),
                _ => None,
            },
            pregnancy: (id == 1).then_some(Pregnancy { father: 2, due: 1 }),
            next_conception: 0,
            alive: true,
            reproduction_allowed: true,
            reconsider_partnership: id == 2,
        })
        .collect();
    initial.family = Some(Families {
        rules: FamilyRules {
            adult_age: 10,
            min_work_age: 10,
            fertile_until: 50,
            gestation: 2,
            birth_spacing: 3,
            lifespan_min: 60,
            lifespan_max: 100,
            conception_one_in: 100,
            minimum_health: 1,
            maximum_hunger: 100,
            newborn_health: 100,
            mutation_per_million: 0,
            mutation_step: 0,
            enroll_births: true,
        },
        members,
        next_id: 5,
        last_tick: 0,
        totals: FamilyTotals::default(),
        events: vec![],
    });
    initial.care = Some(DependentCare {
        rules: CareRules {
            dependent_until: 5,
            max_caregiver_hunger: 100,
            neglect_damage: 0,
            feed_from_cargo: true,
            fetch_local_food: false,
            pressure_threshold: Some(3),
        },
        inherit_births: false,
        policies: vec![CarePolicy {
            person: 2,
            willing: false,
        }],
        events: vec![],
    });
    initial.dynamics = Some(Dynamics {
        personality: None,
        social: Some(game_parallel_social::Social {
            rules: game_parallel_social::Rules {
                interval: 1,
                min_health: 1,
                max_hunger: 100,
                familiarity_gain: 1,
                familiarity_max: 10,
                relationship_retention: None,
            },
            participants: vec![game_parallel_social::Participant {
                person: 1,
                willing: false,
            }],
            pressures: vec![],
            relationships: vec![],
            events: vec![],
            needs: None,
        }),
        disease: None,
        treatment: vec![],
        claims: None,
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.advance().unwrap().started.is_empty());
                let mut state = world.snapshot();
                assert_eq!(state.family.as_ref().unwrap().totals.births, 1);
                assert_eq!(state.family.as_ref().unwrap().members[1].partner, Some(1));
                assert_eq!(state.people.len(), 5);
                assert!(
                    state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .social
                        .as_ref()
                        .unwrap()
                        .participants
                        .iter()
                        .any(|p| p.person == 5 && !p.willing)
                );
                assert!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .any(|row| row.place == Place::Person(5) && row.lots.is_empty())
                );
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Person(2))
                        .unwrap()
                        .lots[0]
                        .expires_at,
                    Some(9)
                );
                state.people[1].seek_meals = false;
                state.people[1].hunger = 0;
                state.people[4].hunger = 5;
                state.opinions.as_mut().unwrap().records.push(Opinion {
                    observer: 2,
                    target: 5,
                    trust: 0,
                    fear: 5,
                    obligation: 0,
                    affinity: 0,
                });
                let mut restored = Economy::compile(state).unwrap();
                assert_eq!(restored.advance().unwrap().started, vec![1]);
                let fed = restored.snapshot();
                assert_eq!(fed.care.as_ref().unwrap().events[0].caregiver, Some(2));
                assert_eq!(fed.people[4].hunger, 6);
                assert_eq!(
                    fed.physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Person(5))
                        .unwrap()
                        .lots[0]
                        .expires_at,
                    Some(9)
                );
                restored.advance().unwrap();
                assert_eq!(restored.snapshot().people[4].hunger, 2);
                Economy::compile(restored.snapshot()).unwrap().snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}
#[test]
fn personal_meals_work_and_pressured_delivery_share_one_saved_world() {
    let initial = fixture();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut economy = Economy::compile(initial.clone()).unwrap();
                let draft = economy.prepare().unwrap();
                assert_eq!(economy.snapshot(), initial);
                assert_eq!(draft.events().started, vec![1]);
                economy.commit(draft).unwrap();
                let state = economy.snapshot();
                assert_eq!(state.people[0].hunger, 1);
                assert_eq!(
                    state.agreements.as_ref().unwrap().contracts[0].status,
                    Status::Fulfilled
                );
                assert_eq!(state.pending_food.len(), 1);
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Person(1))
                        .unwrap()
                        .used,
                    1
                );
                let saved = serde_json::to_string(&state).unwrap();
                let mut resumed = Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                assert_eq!(economy.advance().unwrap().finished, vec![1]);
                assert_eq!(resumed.advance().unwrap().finished, vec![1]);
                assert_eq!(economy.snapshot().people[0].health, 99);
                assert_eq!(
                    economy.snapshot().nutrition.as_ref().unwrap().people[0].reserves[0],
                    7
                );
                assert_eq!(economy.advance().unwrap().delivered, vec![1]);
                assert_eq!(resumed.advance().unwrap().delivered, vec![1]);
                assert_eq!(economy.snapshot(), resumed.snapshot());
                economy.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}
#[test]
fn changing_a_personal_work_choice_invalidates_a_prepared_generation() {
    let mut economy = Economy::compile(fixture()).unwrap();
    let draft = economy.prepare().unwrap();
    economy.set_work_choice(2, false).unwrap();
    let before = economy.snapshot();
    assert!(economy.commit(draft).is_err());
    assert_eq!(economy.snapshot(), before);
    assert!(economy.advance().unwrap().started.is_empty());
}

#[test]
fn privately_changing_promise_intent_does_not_communicate_a_withdrawal_at_a_distance() {
    let mut initial = expedition_fixture();
    initial.dynamics = None;
    initial.missions = None;
    initial.opinions = None;
    initial.nutrition = None;
    let mut agreements = fixture().agreements.unwrap();
    agreements.contracts.clear();
    agreements.events.clear();
    agreements.policies.clear();
    agreements.next_id = 1;
    initial.agreements = Some(agreements);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let id = world
                    .propose_promises(&[PromiseProposal {
                        debtor: 1,
                        creditor: 2,
                        good: 1,
                        quantity: 1,
                        due: 20,
                    }])
                    .unwrap()[0];
                world
                    .depart(&[Departure {
                        person: 2,
                        destination: 2,
                    }])
                    .unwrap();
                world.advance().unwrap();
                world.advance().unwrap();
                assert_eq!(world.snapshot().people[1].region, Some(2));
                let before = world.snapshot();
                assert!(
                    world
                        .decide_promises(&[PromiseDecision {
                            contract: id,
                            actor: 1,
                            accept: false
                        }])
                        .is_err()
                );
                assert_eq!(world.snapshot(), before);
                world.set_promise_choice(1, false).unwrap();
                assert_eq!(
                    world.snapshot().agreements.as_ref().unwrap().contracts[0].status,
                    Status::Proposed
                );
                let saved = world.snapshot();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world
                    .depart(&[Departure {
                        person: 2,
                        destination: 1,
                    }])
                    .unwrap();
                restored
                    .depart(&[Departure {
                        person: 2,
                        destination: 1,
                    }])
                    .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                assert_eq!(world.snapshot().people[1].region, Some(1));
                world
                    .decide_promises(&[PromiseDecision {
                        contract: id,
                        actor: 1,
                        accept: false,
                    }])
                    .unwrap();
                assert_eq!(
                    world.snapshot().agreements.as_ref().unwrap().contracts[0].status,
                    Status::Withdrawn
                );
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn witnessed_promise_outcomes_use_the_receivers_personal_trust_reactions() {
    let mut state = fixture();
    state.people[0].controlled = true;
    for person in &mut state.people {
        person.seek_meals = false;
    }
    state.dynamics = None;
    state.nutrition = None;
    state.autowork = None;
    state.agreements = Some(Agreements {
        offering: vec![],
        work_offering: vec![],
        next_id: 1,
        policies: vec![],
        bulk_debtors: vec![],
        contracts: vec![],
        events: vec![],
    });
    let run = |workers, deliver| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(state.clone()).unwrap();
                world.set_promise_trust_reactions(2, Some((-2, 3))).unwrap();
                let id = world
                    .propose_promises(&[PromiseProposal {
                        debtor: 1,
                        creditor: 2,
                        good: 1,
                        quantity: 1,
                        due: 1,
                    }])
                    .unwrap()[0];
                world
                    .decide_promises(&[PromiseDecision {
                        contract: id,
                        actor: 2,
                        accept: true,
                    }])
                    .unwrap();
                if deliver {
                    assert!(world.fulfill_promise(1, id).unwrap());
                } else {
                    world.advance().unwrap();
                    world.advance().unwrap();
                }
                let snapshot = world.snapshot();
                assert_eq!(
                    snapshot.opinions.as_ref().unwrap().index().trust(2, 1),
                    if deliver { -2 } else { 3 }
                );
                snapshot
            })
    };
    assert_eq!(run(1, true), run(4, true));
    assert_eq!(run(1, false), run(4, false));
}

#[test]
fn people_make_promises_but_can_change_their_minds_before_physical_delivery() {
    let mut state = fixture();
    state.agreements = Some(Agreements {
        offering: vec![],
        work_offering: vec![],
        next_id: 1,
        policies: vec![],
        bulk_debtors: vec![],
        contracts: vec![],
        events: vec![],
    });
    state.autowork = None;
    state.people[0].seek_meals = false;
    let mut world = Economy::compile(state).unwrap();
    let draft = world.prepare().unwrap();
    let before = world.snapshot();
    let bad = PromiseProposal {
        debtor: 1,
        creditor: 99,
        good: 1,
        quantity: 1,
        due: 3,
    };
    assert!(world.propose_promises(&[bad]).is_err());
    assert_eq!(world.snapshot(), before);
    let id = world
        .propose_promises(&[PromiseProposal {
            debtor: 1,
            creditor: 2,
            good: 1,
            quantity: 1,
            due: 3,
        }])
        .unwrap()[0];
    assert!(world.commit(draft).is_err());
    world
        .decide_promises(&[PromiseDecision {
            contract: id,
            actor: 2,
            accept: true,
        }])
        .unwrap();
    world.set_promise_choice(1, false).unwrap();
    world.advance().unwrap();
    assert_eq!(
        world.snapshot().agreements.as_ref().unwrap().contracts[0].delivered,
        0
    );
    let saved = world.snapshot();
    let mut defaulting = Economy::compile(saved.clone()).unwrap();
    for _ in 0..3 {
        defaulting.advance().unwrap();
    }
    assert_eq!(
        defaulting.snapshot().agreements.unwrap().contracts[0].status,
        Status::Defaulted
    );
    let mut honoring = Economy::compile(saved).unwrap();
    honoring.set_promise_choice(1, true).unwrap();
    honoring.advance().unwrap();
    let result = honoring.snapshot();
    assert_eq!(
        result.agreements.unwrap().contracts[0].status,
        Status::Fulfilled
    );
    let holding = result
        .physical
        .custody
        .containers
        .iter()
        .find(|row| row.place == Place::Person(2))
        .unwrap();
    assert_eq!(holding.lots[0].expires_at, Some(10));
}

#[test]
fn controlled_promise_is_a_choice_and_only_physical_delivery_fulfills_it() {
    let mut initial = fixture();
    initial.people[0].controlled = true;
    initial.people[0].seek_meals = false;
    initial.autowork = None;
    initial.agreements = Some(Agreements {
        offering: vec![],
        work_offering: vec![],
        next_id: 1,
        policies: vec![],
        bulk_debtors: vec![],
        contracts: vec![],
        events: vec![],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let id = world
                    .propose_promises(&[PromiseProposal {
                        debtor: 1,
                        creditor: 2,
                        good: 1,
                        quantity: 1,
                        due: 3,
                    }])
                    .unwrap()[0];
                world
                    .decide_promises(&[PromiseDecision {
                        contract: id,
                        actor: 2,
                        accept: true,
                    }])
                    .unwrap();
                world.advance().unwrap();
                assert_eq!(
                    world.snapshot().agreements.as_ref().unwrap().contracts[0].delivered,
                    0
                );
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored = Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                assert!(world.fulfill_promise(1, id).unwrap());
                assert!(restored.fulfill_promise(1, id).unwrap());
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(
                    state.agreements.as_ref().unwrap().contracts[0].status,
                    Status::Fulfilled
                );
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap()
                        .lots[0]
                        .expires_at,
                    Some(10)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
    let mut world = Economy::compile(initial).unwrap();
    let id = world
        .propose_promises(&[PromiseProposal {
            debtor: 1,
            creditor: 2,
            good: 1,
            quantity: 2,
            due: 2,
        }])
        .unwrap()[0];
    world
        .decide_promises(&[PromiseDecision {
            contract: id,
            actor: 2,
            accept: true,
        }])
        .unwrap();
    assert!(!world.fulfill_promise(1, id).unwrap());
    assert_eq!(world.tick(), 1);
    world.advance().unwrap();
    world.advance().unwrap();
    assert_eq!(
        world.snapshot().agreements.unwrap().contracts[0].status,
        Status::Defaulted
    );
}

fn combat_rules() -> Combat {
    use game_parallel_conflict::Conflict;
    use game_parallel_economy_combat::{Reaction, Response};
    use game_parallel_tactics::{Map, Rules, Tactics};
    Combat {
        conflict: Conflict {
            damage: 10,
            next_id: 1,
            encounters: vec![],
            events: vec![],
        },
        tactics: Tactics {
            maps: vec![
                Map::generate(
                    1,
                    7,
                    Rules {
                        width: 10,
                        height: 10,
                        obstacle_count: 0,
                        max_obstacle_size: 1,
                        movement: 4,
                        attack_range: 1,
                    },
                )
                .unwrap(),
            ],
            battles: vec![],
        },
        reactions: vec![Reaction {
            person: 2,
            response: Response::Fight,
            retreat_at_or_below: None,
            pursue: true,
        }],
        protection: vec![],
        weapons: vec![],
    }
}

#[test]
fn simultaneous_confrontations_choose_disjoint_people_and_reserve_their_time() {
    use game_parallel_opinions::{Confrontation, Sensitivity};
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    initial.dynamics = None;
    initial.nutrition = None;
    for person in &mut initial.people {
        person.seek_meals = true;
        person.hunger = 20;
    }
    initial.combat = Some(combat_rules());
    let opinions = initial.opinions.as_mut().unwrap();
    opinions.records = [(1, 2), (3, 4), (4, 2)]
        .into_iter()
        .map(|(observer, target)| Opinion {
            observer,
            target,
            trust: -3,
            fear: 0,
            obligation: 0,
            affinity: 0,
        })
        .collect();
    opinions.sensitivities = [1, 3, 4]
        .into_iter()
        .map(|person| Sensitivity {
            person,
            confrontation: Some(Confrontation {
                trust_at_most: -2,
                fear_at_most: 0,
                interval: 1,
                enabled: true,
            }),
            ..Default::default()
        })
        .collect::<Vec<_>>()
        .into();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let fights = &state.combat.as_ref().unwrap().conflict.encounters;
                assert_eq!(fights.len(), 2);
                assert_eq!(
                    fights
                        .iter()
                        .map(|f| f.fighters.iter().map(|p| p.person).collect::<Vec<_>>())
                        .collect::<Vec<_>>(),
                    vec![vec![1, 2], vec![3, 4]]
                );
                for person in [1, 4] {
                    let holding = state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(person))
                        .unwrap();
                    assert_eq!(
                        holding.used, 1,
                        "selected combatants cannot eat in this boundary"
                    );
                }
                assert!(state.combat.as_ref().unwrap().conflict.events.is_empty());
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn confrontation_uses_sparse_people_and_multiple_personal_opinions() {
    use game_parallel_opinions::{Confrontation, Sensitivity};
    let mut initial = fixture();
    initial.people[3].id = 10;
    initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(4))
        .unwrap()
        .place = Place::Person(10);
    initial.autowork = None;
    initial.agreements = None;
    initial.nutrition = None;
    initial.dynamics = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.combat = Some(combat_rules());
    let opinions = initial.opinions.as_mut().unwrap();
    opinions.records = [(1, 2, -1), (3, 2, -1), (1, 10, -3)]
        .into_iter()
        .map(|(observer, target, trust)| Opinion {
            observer,
            target,
            trust,
            fear: 0,
            obligation: 0,
            affinity: 0,
        })
        .collect();
    opinions.sensitivities = vec![Sensitivity {
        person: 1,
        confrontation: Some(Confrontation {
            trust_at_most: -1,
            fear_at_most: 0,
            interval: 1,
            enabled: true,
        }),
        ..Default::default()
    }]
    .into();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let fights = &state.combat.as_ref().unwrap().conflict.encounters;
                assert_eq!(fights.len(), 1);
                assert_eq!(
                    fights[0]
                        .fighters
                        .iter()
                        .map(|fighter| fighter.person)
                        .collect::<Vec<_>>(),
                    vec![1, 10]
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
    let opinions = initial.opinions.as_mut().unwrap();
    for row in &mut opinions.records {
        if row.observer == 1 {
            row.trust = if row.target == 2 { -3 } else { -1 };
        }
    }
    opinions.sensitivities_mut()[0].person = 2;
    let mut world = Economy::compile(initial).unwrap();
    world.advance().unwrap();
    assert!(
        world
            .snapshot()
            .combat
            .unwrap()
            .conflict
            .encounters
            .is_empty(),
        "a gap in observer IDs must not expose another person's opinions"
    );
}

#[test]
fn many_hostile_people_cannot_assign_one_target_to_multiple_fights() {
    use game_parallel_opinions::{Confrontation, Sensitivity};
    let count = 4096;
    let mut initial = concentrated(count);
    initial.autowork = None;
    initial.nutrition = None;
    initial.dynamics = None;
    for person in &mut initial.people {
        person.seek_meals = false;
        person.hunger = 0;
    }
    initial.combat = Some(combat_rules());
    let mut opinions = fixture().opinions.unwrap();
    opinions.records = (1..count)
        .map(|observer| Opinion {
            observer,
            target: count,
            trust: -3,
            fear: 0,
            obligation: 0,
            affinity: 0,
        })
        .collect();
    opinions.sensitivities = (1..count)
        .map(|person| Sensitivity {
            person,
            confrontation: Some(Confrontation {
                trust_at_most: -2,
                fear_at_most: 0,
                interval: 1,
                enabled: true,
            }),
            ..Default::default()
        })
        .collect::<Vec<_>>()
        .into();
    initial.opinions = Some(opinions);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let fights = &state.combat.as_ref().unwrap().conflict.encounters;
                assert_eq!(fights.len(), 1);
                assert_eq!(
                    fights[0]
                        .fighters
                        .iter()
                        .map(|fighter| fighter.person)
                        .collect::<Vec<_>>(),
                    vec![1, count]
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore = "performance measurement"]
fn concentrated_tactical_confrontation_benchmark() {
    use game_parallel_opinions::{Confrontation, Sensitivity};
    let count: u64 = std::env::var("LEGACY_COUNT")
        .unwrap_or_else(|_| "10000".into())
        .parse()
        .unwrap();
    let workers: usize = std::env::var("LEGACY_WORKERS")
        .unwrap_or_else(|_| "1".into())
        .parse()
        .unwrap();
    let hot_target = std::env::var_os("LEGACY_HOT_TARGET").is_some();
    assert!(count >= 4 && count.is_multiple_of(2));
    let mut initial = concentrated(count);
    initial.autowork = None;
    initial.nutrition = None;
    initial.dynamics = None;
    for person in &mut initial.people {
        person.seek_meals = false;
        person.hunger = 0;
    }
    initial.combat = Some(combat_rules());
    let mut opinions = fixture().opinions.unwrap();
    let initiators: Vec<_> = (1..count)
        .filter(|person| hot_target || person % 2 == 1)
        .collect();
    opinions.records = initiators
        .iter()
        .copied()
        .map(|person| Opinion {
            observer: person,
            target: if hot_target { count } else { person + 1 },
            trust: -3,
            fear: 0,
            obligation: 0,
            affinity: 0,
        })
        .collect();
    opinions.sensitivities = initiators
        .into_iter()
        .map(|person| Sensitivity {
            person,
            confrontation: Some(Confrontation {
                trust_at_most: -2,
                fear_at_most: 0,
                interval: 1,
                enabled: true,
            }),
            ..Default::default()
        })
        .collect::<Vec<_>>()
        .into();
    initial.opinions = Some(opinions);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let start = std::time::Instant::now();
                world.advance().unwrap();
                let elapsed = start.elapsed().as_secs_f64();
                let fights = world.snapshot().combat.unwrap().conflict.encounters;
                assert_eq!(
                    fights.len(),
                    if hot_target { 1 } else { count as usize / 2 }
                );
                if hot_target {
                    assert_eq!(
                        fights[0]
                            .fighters
                            .iter()
                            .map(|f| f.person)
                            .collect::<Vec<_>>(),
                        vec![1, count]
                    );
                }
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} hot_target={hot_target} tick_seconds={:.6}",
                samples[1]
            );
            #[cfg(feature = "profile")]
            for (name, measurement) in game_diagnostics::snapshot() {
                println!(
                    "{name}: {:.3} ms/call",
                    measurement.nanoseconds as f64 / measurement.calls as f64 / 1_000_000.0
                );
            }
        });
}

#[test]
fn an_autonomous_attack_can_interrupt_a_targets_work_without_consent() {
    use game_parallel_opinions::{Confrontation, Sensitivity};
    let mut initial = fixture();
    initial.agreements = None;
    initial.nutrition = None;
    initial.dynamics = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.combat = Some(combat_rules());
    initial.opinions.as_mut().unwrap().records.push(Opinion {
        observer: 1,
        target: 2,
        trust: -3,
        fear: 0,
        obligation: 0,
        affinity: 0,
    });
    let mut world = Economy::compile(initial).unwrap();
    assert_eq!(world.advance().unwrap().started, vec![1]);
    world
        .set_opinion_sensitivity(Sensitivity {
            person: 1,
            confrontation: Some(Confrontation {
                trust_at_most: -2,
                fear_at_most: 0,
                interval: 1,
                enabled: true,
            }),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(world.advance().unwrap().abandoned, vec![1]);
    let state = world.snapshot();
    assert_eq!(state.combat.as_ref().unwrap().conflict.encounters.len(), 1);
    assert!(state.combat.as_ref().unwrap().conflict.events.is_empty());
}

#[test]
fn attack_interrupts_work_and_applies_simultaneous_harm_without_target_consent() {
    use game_parallel_economy_combat::Order;
    let mut initial = fixture();
    initial.agreements = None;
    initial.people[0].controlled = true;
    initial.people[0].seek_meals = false;
    initial.opinions.as_mut().unwrap().rules.fear_per_damage = 1;
    initial.combat = Some(combat_rules());
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert_eq!(world.advance().unwrap().started, vec![1]);
                let stale = world.prepare().unwrap();
                let id = world.attack(1, 2).unwrap();
                assert!(world.commit(stale).is_err());
                assert!(world.eat(1, 1).is_err());
                assert_eq!(world.advance().unwrap().abandoned, vec![1]);
                let state = world.snapshot();
                assert_eq!((state.people[0].health, state.people[1].health), (90, 90));
                assert_eq!(state.combat.as_ref().unwrap().conflict.events.len(), 2);
                assert_eq!(
                    state
                        .opinions
                        .as_ref()
                        .unwrap()
                        .records
                        .iter()
                        .find(|r| r.observer == 2 && r.target == 1)
                        .unwrap()
                        .fear,
                    10
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                world.combat_order(1, id, Order::Retreat).unwrap();
                restored.combat_order(1, id, Order::Retreat).unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                assert!(
                    world
                        .snapshot()
                        .combat
                        .unwrap()
                        .conflict
                        .encounters
                        .is_empty()
                );
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn actual_combat_strikes_can_transmit_only_when_the_strain_allows_it() {
    use game_parallel_economy_combat::Point;
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    initial.people[0].controlled = true;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.combat = Some(combat_rules());
    let mut dynamics = infection_from(2);
    dynamics.disease.as_mut().unwrap().strains[0].transmission_per_million = 0;
    initial.dynamics = Some(dynamics);
    let run = |workers, rate, distant| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = initial.clone();
                state
                    .dynamics
                    .as_mut()
                    .unwrap()
                    .disease
                    .as_mut()
                    .unwrap()
                    .strains[0]
                    .combat_transmission_per_million = rate;
                let mut world = Economy::compile(state).unwrap();
                let fight = world.attack(1, 2).unwrap();
                if distant {
                    world.combat_move(1, fight, Point { x: 3, y: 0 }).unwrap();
                }
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&world.snapshot()).unwrap())
                        .unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                let strikes = &state.combat.as_ref().unwrap().conflict.events;
                assert_eq!(strikes.is_empty(), distant);
                let cases = &state
                    .dynamics
                    .as_ref()
                    .unwrap()
                    .disease
                    .as_ref()
                    .unwrap()
                    .cases;
                assert_eq!(
                    cases
                        .iter()
                        .any(|case| case.person == 1 && case.source == Some(2)),
                    rate > 0 && !distant
                );
                state
            })
    };
    assert_eq!(run(1, 1_000_000, false), run(4, 1_000_000, false));
    assert_eq!(run(1, 0, false), run(4, 0, false));
    assert_eq!(run(1, 1_000_000, true), run(4, 1_000_000, true));
}

#[test]
fn carried_protection_reduces_actual_strikes_and_survives_reload() {
    use game_parallel_economy_combat::Protection;
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    initial.people[0].controlled = true;
    for p in &mut initial.people {
        p.seek_meals = false;
    }
    initial.combat = Some(combat_rules());
    initial.combat.as_mut().unwrap().protection = vec![Protection {
        good: 3,
        reduction: 4,
    }];
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(2))
        .unwrap();
    *holder = Container::new(
        holder.place,
        holder.capacity,
        vec![Lot {
            good: 3,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.attack(1, 2).unwrap();
                let saved = world.snapshot();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!((state.people[0].health, state.people[1].health), (90, 94));
                let events = &state.combat.as_ref().unwrap().conflict.events;
                assert_eq!(events.iter().find(|e| e.target == 2).unwrap().damage, 6);
                state
            })
    };
    assert_eq!(run(1), run(4));
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(2))
        .unwrap();
    *holder = Container::new(holder.place, holder.capacity, vec![]).unwrap();
    let mut world = Economy::compile(initial.clone()).unwrap();
    world.attack(1, 2).unwrap();
    world.advance().unwrap();
    assert_eq!(world.snapshot().people[1].health, 90);
    initial.combat.as_mut().unwrap().protection[0].good = u32::MAX;
    assert!(Economy::compile(initial).is_err());
}

#[test]
fn carried_weapon_and_protection_change_the_real_attributed_strike() {
    use game_parallel_economy_combat::{Protection, Weapon};
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    initial.people[0].controlled = true;
    for p in &mut initial.people {
        p.seek_meals = false;
    }
    initial.combat = Some(combat_rules());
    initial.combat.as_mut().unwrap().protection = vec![Protection {
        good: 3,
        reduction: 4,
    }];
    initial.combat.as_mut().unwrap().weapons = vec![Weapon { good: 2, bonus: 7 }];
    for (id, good) in [(1, 2), (2, 3)] {
        let holder = initial
            .physical
            .custody
            .containers
            .iter_mut()
            .find(|c| c.place == Place::Person(id))
            .unwrap();
        *holder = Container::new(
            holder.place,
            holder.capacity,
            vec![Lot {
                good,
                quantity: 1,
                expires_at: None,
            }],
        )
        .unwrap();
    }
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.attack(1, 2).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!((state.people[0].health, state.people[1].health), (90, 87));
                let events = &state.combat.as_ref().unwrap().conflict.events;
                assert_eq!(events.iter().find(|e| e.attacker == 1).unwrap().damage, 13);
                assert_eq!(events.iter().find(|e| e.attacker == 2).unwrap().damage, 10);
                Economy::compile(state.clone()).unwrap();
                state
            })
    };
    assert_eq!(run(1), run(4));
    initial.combat.as_mut().unwrap().reactions[0].response =
        game_parallel_economy_combat::Response::Defend;
    let mut world = Economy::compile(initial).unwrap();
    world.attack(1, 2).unwrap();
    world.advance().unwrap();
    assert_eq!(world.snapshot().people[1].health, 96);
}

#[test]
fn tactical_range_controls_attacks_from_saved_positions() {
    use game_parallel_economy_combat::Point;
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    initial.people[0].controlled = true;
    initial.people[0].seek_meals = false;
    initial.combat = Some(combat_rules());
    let mut world = Economy::compile(initial).unwrap();
    let id = world.attack(1, 2).unwrap();
    world.combat_move(1, id, Point { x: 3, y: 0 }).unwrap();
    assert!(world.combat_move(1, id, Point { x: 2, y: 0 }).is_err());
    let mut restored = Economy::compile(
        serde_json::from_str(&serde_json::to_string(&world.snapshot()).unwrap()).unwrap(),
    )
    .unwrap();
    world.advance().unwrap();
    restored.advance().unwrap();
    assert_eq!(world.snapshot(), restored.snapshot());
    let state = world.snapshot();
    let combat = state.combat.unwrap();
    assert!(combat.conflict.events.is_empty());
    assert_eq!(
        combat.tactics.battles[0]
            .people
            .iter()
            .find(|p| p.person == 2)
            .unwrap()
            .position,
        Point { x: 3, y: 1 }
    );
    world.advance().unwrap();
    restored.advance().unwrap();
    assert_eq!(world.snapshot(), restored.snapshot());
    assert_eq!(world.snapshot().combat.unwrap().conflict.events.len(), 2);
}

#[test]
fn wounded_fighter_can_change_their_mind_and_retreat() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    initial.people[0].controlled = true;
    initial.people[0].seek_meals = false;
    initial.people[1].health = 20;
    let mut combat = combat_rules();
    combat.reactions[0].retreat_at_or_below = Some(20);
    initial.combat = Some(combat);
    let mut world = Economy::compile(initial).unwrap();
    world.attack(1, 2).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    assert_eq!((state.people[0].health, state.people[1].health), (100, 10));
    assert!(state.combat.unwrap().conflict.encounters.is_empty());
}

#[test]
fn fatal_combat_drives_family_death_and_later_actual_succession() {
    let mut initial = campaign_fixture();
    initial.pending_food.clear();
    initial.people[2].health = 5;
    initial.combat = Some(combat_rules());
    let mut world = Economy::compile(initial).unwrap();
    world.attack(3, 2).unwrap();
    world.advance().unwrap();
    let after = world.snapshot();
    assert_eq!(after.people[2].health, 0);
    assert!(
        !after
            .family
            .as_ref()
            .unwrap()
            .members
            .iter()
            .find(|m| m.id == 3)
            .unwrap()
            .alive
    );
    assert_eq!(after.campaign.unwrap().lineage.current, None);
    world.advance().unwrap();
    assert_eq!(world.snapshot().campaign.unwrap().lineage.current, Some(5));
}

fn concentrated_combat(count: u64) -> State {
    use game_parallel_conflict::{Conflict, Encounter, Fighter, Order};
    use game_parallel_tactics::{Battle, Map, Placement, Point, Rules, Tactics};
    let mut state = knowledge_workload(count);
    state.knowledge = None;
    state.rules.max_health = (count * 4) as u32;
    state.people[0].health = state.rules.max_health;
    let fighters = (1..=count)
        .map(|person| Fighter {
            person,
            side: u32::from(person != 1),
            order: if person == 1 {
                Order::Defend
            } else {
                Order::Attack { target: 1 }
            },
        })
        .collect();
    let people = (1..=count)
        .map(|person| Placement {
            person,
            position: Point {
                x: (person - 1) as u32,
                y: 0,
            },
            last_move: None,
        })
        .collect();
    state.combat = Some(Combat {
        conflict: Conflict {
            damage: 1,
            next_id: 2,
            encounters: vec![Encounter {
                id: 1,
                region: 1,
                fighters,
            }],
            events: vec![],
        },
        tactics: Tactics {
            maps: vec![
                Map::generate(
                    1,
                    7,
                    Rules {
                        width: count as u32 + 1,
                        height: 10,
                        obstacle_count: 0,
                        max_obstacle_size: 1,
                        movement: 1,
                        attack_range: count as u32,
                    },
                )
                .unwrap(),
            ],
            battles: vec![Battle {
                encounter: 1,
                region: 1,
                people,
            }],
        },
        reactions: vec![],
        protection: vec![],
        weapons: vec![],
    });
    state
}

fn concentrated_pursuit(count: u64) -> State {
    use game_parallel_economy_combat::{Reaction, Response};
    use game_parallel_tactics::{Map, Rules};
    let mut state = concentrated_combat(count);
    let combat = state.combat.as_mut().unwrap();
    combat.tactics.maps = vec![
        Map::generate(
            1,
            7,
            Rules {
                width: count as u32 * 2 + 1,
                height: 10,
                obstacle_count: 0,
                max_obstacle_size: 1,
                movement: 1,
                attack_range: 1,
            },
        )
        .unwrap(),
    ];
    for person in &mut combat.tactics.battles[0].people {
        person.position.x *= 2;
    }
    combat.reactions = (2..=count)
        .map(|person| Reaction {
            person,
            response: Response::Fight,
            retreat_at_or_below: None,
            pursue: true,
        })
        .collect();
    state
}

#[test]
fn concentrated_pursuit_publishes_unique_positions_across_workers() {
    let initial = concentrated_pursuit(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let battle = &state.combat.as_ref().unwrap().tactics.battles[0];
                assert_eq!(battle.people.last().unwrap().position.x, (4096 - 1) * 2 - 1);
                Economy::compile(state.clone()).unwrap();
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_pursuit_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = concentrated_pursuit(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=3 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn concentrated_combat_damage_matches_across_workers() {
    let initial = concentrated_combat(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state.people[0].health, 4096 * 4 - 4095);
                assert_eq!(state.combat.as_ref().unwrap().conflict.events.len(), 4095);
                state
            })
    };
    assert_eq!(run(1), run(4));
}

fn concentrated_protected_combat(count: u64) -> State {
    use game_parallel_economy_combat::Protection;
    let mut state = concentrated_combat(count);
    state.combat.as_mut().unwrap().protection = vec![Protection {
        good: 3,
        reduction: 1,
    }];
    let holder = state
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(
        holder.place,
        holder.capacity,
        vec![Lot {
            good: 3,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    state
}

#[test]
fn concentrated_attacks_respect_actual_protection_and_preserve_attribution() {
    let initial = concentrated_protected_combat(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state.people[0].health, 4096 * 4);
                let events = &state.combat.as_ref().unwrap().conflict.events;
                assert_eq!(events.len(), 4095);
                assert!(events.iter().all(|e| e.damage == 0));
                state
            })
    };
    assert_eq!(run(1), run(4));
}

fn mixed_weapon_combat(count: u64) -> State {
    use game_parallel_economy_combat::Weapon;
    let mut initial = concentrated_protected_combat(count);
    initial.combat.as_mut().unwrap().conflict.damage = 2;
    initial.combat.as_mut().unwrap().weapons = vec![Weapon { good: 2, bonus: 2 }];
    for holder in &mut initial.physical.custody.containers {
        if matches!(holder.place, Place::Person(id) if id > count / 2) {
            *holder = Container::new(
                holder.place,
                holder.capacity,
                vec![Lot {
                    good: 2,
                    quantity: 1,
                    expires_at: None,
                }],
            )
            .unwrap();
        }
    }
    initial
}

#[test]
fn concentrated_mixed_weapons_sum_individual_damage_deterministically() {
    let count = 4096;
    let initial = mixed_weapon_combat(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state.people[0].health, count as u32 * 4 - count as u32 / 2);
                let events = &state.combat.as_ref().unwrap().conflict.events;
                assert_eq!(events.len(), count as usize - 1);
                assert_eq!(
                    events.iter().filter(|e| e.damage == 1).count(),
                    count as usize / 2
                );
                assert_eq!(
                    events.iter().filter(|e| e.damage == 0).count(),
                    count as usize / 2 - 1
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_mixed_weapon_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = mixed_weapon_combat(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=3 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
#[ignore]
fn concentrated_protected_combat_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = concentrated_protected_combat(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=3 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
#[ignore]
fn concentrated_combat_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let mut initial = concentrated_combat(count);
    let disease = std::env::var("LEGACY_DISEASE").is_ok_and(|value| value == "1");
    if disease {
        let mut dynamics = infection_from(1);
        let strain = &mut dynamics.disease.as_mut().unwrap().strains[0];
        strain.transmission_per_million = 0;
        strain.combat_transmission_per_million = 1_000_000;
        initial.dynamics = Some(dynamics);
    }
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                let elapsed = started.elapsed().as_secs_f64();
                if disease {
                    assert_eq!(
                        world
                            .snapshot()
                            .dynamics
                            .unwrap()
                            .disease
                            .unwrap()
                            .cases
                            .len(),
                        count as usize
                    );
                }
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} disease={disease} ticks=3 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn recipient_can_automatically_acknowledge_a_promise_without_compelling_its_maker() {
    let mut state = fixture();
    state.agreements = Some(Agreements {
        offering: vec![],
        work_offering: vec![],
        next_id: 1,
        policies: vec![AgreementPolicy {
            person: 2,
            honor: false,
            accept: true,
            minimum_trust: None,
            minimum_honor_trust: None,
            fear_honor_threshold: None,
            obligation_honor_threshold: None,
            accept_pressure_threshold: None,
        }],
        bulk_debtors: vec![],
        contracts: vec![],
        events: vec![],
    });
    state.autowork = None;
    state.people[0].seek_meals = false;
    let mut world = Economy::compile(state).unwrap();
    world
        .propose_promises(&[PromiseProposal {
            debtor: 1,
            creditor: 2,
            good: 1,
            quantity: 1,
            due: 4,
        }])
        .unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    assert_eq!(
        state.agreements.as_ref().unwrap().contracts[0].status,
        Status::Active
    );
    assert_eq!(state.agreements.as_ref().unwrap().contracts[0].delivered, 0);
    let mut world = Economy::compile(state).unwrap();
    world.advance().unwrap();
    assert_eq!(
        world.snapshot().agreements.as_ref().unwrap().contracts[0].delivered,
        0
    );
    world.set_promise_choice(1, true).unwrap();
    world.advance().unwrap();
    assert_eq!(
        world.snapshot().agreements.as_ref().unwrap().contracts[0].status,
        Status::Fulfilled
    );
}

#[test]
fn aspirations_form_goals_and_renew_only_after_actual_work() {
    use game_parallel_dreams::Dream;
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: None,
    });
    initial.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: false,
        people: vec![],
        events: vec![],
    });
    initial.dreams = Some(Dreams {
        inherit_births: false,
        learning: vec![],
        people: vec![Dream {
            person: 2,
            recipe: 1,
            priority: 4,
            batch: 1,
            review_interval: 2,
            next_review: 0,
            enabled: true,
            recognize_help: false,
        }],
        events: vec![],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.advance().unwrap().started.is_empty());
                assert_eq!(world.snapshot().goals.as_ref().unwrap().people[0].target, 1);
                assert_eq!(world.advance().unwrap().started, vec![1]);
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut world = Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                assert_eq!(world.advance().unwrap().finished, vec![1]);
                let result = world.snapshot();
                let goal = &result.goals.as_ref().unwrap().people[0];
                assert_eq!((goal.progress, goal.target), (1, 2));
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn unavailable_goal_inputs_schedule_a_bounded_retry_without_pressuring_an_unwilling_person() {
    use game_parallel_goals::Goal;
    use game_parallel_purpose::{Person as PurposePerson, Rules as PurposeRules};
    let mut initial = fixture();
    initial.agreements = None;
    initial.purpose = Some(Purpose {
        rules: PurposeRules {
            max: 20,
            per_tick: 1,
            blocked_increase: 2,
            completion_relief: 3,
            damage_threshold: 20,
            damage: 1,
        },
        people: vec![
            PurposePerson {
                person: 2,
                deprivation: 0,
                eligible_at: 0,
            },
            PurposePerson {
                person: 3,
                deprivation: 0,
                eligible_at: 0,
            },
        ],
    });
    initial.people[2].willing_to_work = true;
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: None,
    });
    initial.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: false,
        people: vec![
            Goal {
                person: 2,
                recipe: 1,
                priority: 1,
                target: 1,
                progress: 0,
                next_review: 0,
                enabled: true,
                recognize_help: false,
            },
            Goal {
                person: 3,
                recipe: 1,
                priority: 1,
                target: 1,
                progress: 0,
                next_review: 0,
                enabled: true,
                recognize_help: false,
            },
        ],
        events: vec![],
    });
    initial.people[1].willing_to_work = false;
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 2 })
        .unwrap();
    *stock = Container::new(stock.place, stock.capacity, vec![]).unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.advance().unwrap().started.is_empty());
                let state = world.snapshot();
                assert_eq!(
                    state
                        .purpose
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .map(|p| p.deprivation)
                        .collect::<Vec<_>>(),
                    vec![1, 3]
                );
                let goals = state.goals.as_ref().unwrap();
                assert_eq!(goals.people[0].next_review, 0);
                assert_eq!(goals.people[1].next_review, 4);
                assert!(
                    goals
                        .events
                        .iter()
                        .any(|event| event.person == 3 && event.blocked)
                );
                let mut world = Economy::compile(state).unwrap();
                world.advance().unwrap();
                assert!(world.snapshot().goals.as_ref().unwrap().events.is_empty());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
    let mut overflow = initial;
    overflow.goals.as_mut().unwrap().retry_interval = u64::MAX;
    let mut world = Economy::compile(overflow).unwrap();
    let before = world.snapshot();
    assert!(world.advance().is_err());
    assert_eq!(world.snapshot(), before);
}

#[test]
fn crops_consume_seed_grow_without_labor_and_harvest_only_from_personal_choice() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.people[1].willing_to_work = false;
    initial.opinions.as_mut().unwrap().rules.fear_decay_per_tick = 5;
    initial.opinions.as_mut().unwrap().records = vec![Opinion {
        observer: 2,
        target: 1,
        trust: 0,
        fear: 5,
        obligation: 0,
        affinity: 0,
    }];
    initial.agriculture = Some(Agriculture {
        inherit_births: false,
        crops: vec![Crop {
            id: 1,
            seed: 2,
            seed_quantity: 2,
            output: 1,
            output_quantity: 3,
            growth_ticks: 2,
        }],
        fields: vec![Field {
            id: 1,
            region: 1,
            crop: 1,
            phase: CropPhase::Fallow,
            remaining: 0,
        }],
        policies: vec![FarmPolicy {
            person: 2,
            willing: false,
            pressured_by: Some(1),
            pressure_threshold: Some(3),
        }],
        max_hunger: 20,
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state.agriculture.as_ref().unwrap().fields[0].phase,
                    CropPhase::Growing
                );
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Stock { region: 1, good: 2 })
                        .unwrap()
                        .used,
                    0
                );
                let mut world = Economy::compile(state).unwrap();
                world.advance().unwrap();
                world.advance().unwrap();
                assert_eq!(
                    world.snapshot().agriculture.as_ref().unwrap().fields[0].phase,
                    CropPhase::Ripe
                );
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state.agriculture.as_ref().unwrap().fields[0].phase,
                    CropPhase::Ripe
                );
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
                        .unwrap()
                        .used,
                    0
                );
                let mut world = Economy::compile(state).unwrap();
                world.set_farming_choice(2, true).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state.agriculture.as_ref().unwrap().fields[0].phase,
                    CropPhase::Fallow
                );
                let stock = state
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|row| row.place == Place::Stock { region: 1, good: 1 })
                    .unwrap();
                assert_eq!((stock.used, stock.lots[0].expires_at), (3, Some(15)));
                Economy::compile(state).unwrap().snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn later_digestion_cannot_revive_a_person_who_died() {
    let mut state = fixture();
    state.pending_food = vec![
        FoodEffect {
            person: 3,
            at: 1,
            health: -100,
            nutrients: [0; 6],
        },
        FoodEffect {
            person: 3,
            at: 1,
            health: 100,
            nutrients: [0; 6],
        },
    ];
    let mut economy = Economy::compile(state).unwrap();
    economy.advance().unwrap();
    assert_eq!(economy.snapshot().people[2].health, 0);
}

fn concentrated(count: u64) -> State {
    let mut state = fixture();
    state.people = (1..=count)
        .map(|id| Person {
            id,
            region: Some(1),
            health: 100,
            hunger: if id <= count / 2 { 5 } else { 0 },
            willing_to_work: id > count / 2,
            seek_meals: id <= count / 2,
            controlled: false,
        })
        .collect();
    let stock = |good, quantity| {
        Container::new(
            Place::Stock { region: 1, good },
            count * 2,
            if quantity == 0 {
                vec![]
            } else {
                vec![Lot {
                    good,
                    quantity,
                    expires_at: None,
                }]
            },
        )
        .unwrap()
    };
    let mut containers = vec![stock(1, 0), stock(2, count / 4), stock(3, 0)];
    containers.extend((1..=count).map(|id| {
        Container::new(
            Place::Person(id),
            10,
            if id <= count / 2 {
                vec![Lot {
                    good: 1,
                    quantity: 1,
                    expires_at: Some(10),
                }]
            } else {
                vec![]
            },
        )
        .unwrap()
    }));
    state.physical.custody = Custody {
        tick: 0,
        revision: 0,
        containers,
    };
    state.agreements = None;
    state.opinions = None;
    state.autowork = Some(Autowork {
        next_id: 1,
        policies: (count / 2 + 1..=count)
            .map(|person| WorkPolicy {
                person,
                recipe: 1,
                enabled: true,
                disregard_claims: false,
            })
            .collect(),
        requests: None,
    });
    state
}

fn field_workload(count: u64) -> State {
    let mut initial = concentrated(count);
    initial.autowork = None;
    initial.nutrition = None;
    for person in &mut initial.people {
        person.seek_meals = false;
        person.hunger = 0;
    }
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 2 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        count * 2,
        vec![Lot {
            good: 2,
            quantity: count / 2,
            expires_at: None,
        }],
    )
    .unwrap();
    initial.agriculture = Some(Agriculture {
        inherit_births: false,
        crops: vec![Crop {
            id: 1,
            seed: 2,
            seed_quantity: 1,
            output: 3,
            output_quantity: 1,
            growth_ticks: 1,
        }],
        fields: (1..=count)
            .map(|id| Field {
                id,
                region: 1,
                crop: 1,
                phase: CropPhase::Fallow,
                remaining: 0,
            })
            .collect(),
        policies: (1..=count)
            .map(|person| FarmPolicy {
                person,
                willing: true,
                pressured_by: None,
                pressure_threshold: None,
            })
            .collect(),
        max_hunger: 20,
    });
    initial
}

#[test]
#[ignore]
fn concentrated_animal_care_benchmark() {
    use game_parallel_genetics::{Genome, Sex};
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let social = std::env::var_os("LEGACY_ANIMAL_RESPONSE").is_some();
    let human_social = std::env::var_os("LEGACY_HUMAN_ANIMAL_FEELINGS").is_some();
    let mut initial = concentrated(count);
    initial.autowork = None;
    initial.nutrition = None;
    for person in &mut initial.people {
        person.seek_meals = false;
        person.hunger = 0;
    }
    initial.fauna = Some(Fauna {
        last_tick: 0,
        next_id: 2,
        species: vec![AnimalSpecies {
            id: 1,
            name: "grazer".into(),
            food_good: 1,
            intake: 2,
            hunger_per_tick: 1,
            relief_per_unit: 1,
            starvation_threshold: u32::MAX,
            starvation_damage: 1,
            max_health: 10,
            adult_age: 1,
            fertile_until: 10,
            lifespan: 30,
            gestation: 2,
            conception_one_in: 10,
            breeding_max_hunger: 5,
            newborn_health: 10,
            mutation_per_million: 0,
            mutation_step: 0,
            climate: None,
            hunting: None,
        }],
        animals: vec![Animal {
            id: 1,
            species: 1,
            region: 1,
            age: 1,
            health: 10,
            hunger: u32::try_from(count).unwrap(),
            genome: Genome {
                sex: Sex::Male,
                alleles: [[100; 2]; 4],
            },
            pregnancy: None,
            parents: None,
            social_response: social.then_some(game_parallel_fauna::SocialResponse {
                feed_affinity: 1,
                minimum: -10,
                maximum: 10,
                capacity: 64,
            }),
            opinions: vec![],
        }],
    });
    initial.animal_care = Some(AnimalCare {
        policies: (1..=count)
            .map(|person| AnimalCarePolicy {
                person,
                animal: 1,
                good: 1,
                quantity: 1,
                willing: true,
                health_at_most: 10,
                affinity_at_least: None,
                affinity_at_most: None,
                pressured_by: None,
                pressure_threshold: None,
            })
            .collect(),
    });
    if human_social {
        initial.animal_feelings = Some(AnimalFeelings {
            minds: (1..=count)
                .map(|person| AnimalFeelingMind {
                    person,
                    minimum: -10,
                    maximum: 10,
                    capacity: 1,
                    on_feed: 1,
                    on_hunt: 0,
                    opinions: vec![],
                })
                .collect(),
        });
    }
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let start = std::time::Instant::now();
                world.advance().unwrap();
                let elapsed = start.elapsed().as_secs_f64();
                let state = world.snapshot();
                let fauna = state.fauna.unwrap();
                assert_eq!(
                    fauna.animals[0].hunger,
                    u32::try_from(count - count / 2 + 1).unwrap()
                );
                assert_eq!(fauna.animals[0].opinions.len(), if social { (count / 2).min(64) as usize } else { 0 });
                if human_social {
                    let minds = &state.animal_feelings.as_ref().unwrap().minds;
                    assert_eq!(minds[0].opinions[0].affinity, 1);
                    assert!(minds[(count / 2) as usize].opinions.is_empty());
                }
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} social={social} human_social={human_social} concentrated_animal_care_seconds={:.6}",
                samples[1]
            );
            #[cfg(feature = "profile")]
            for (name, measurement) in game_diagnostics::snapshot() {
                println!(
                    "{name}: {:.3} ms/call",
                    measurement.nanoseconds as f64 / measurement.calls as f64 / 1_000_000.0
                );
            }
        });
}

#[test]
fn controlled_cultivation_uses_real_seed_and_publishes_actual_harvest() {
    let mut initial = field_workload(4);
    initial.people[0].controlled = true;
    for policy in &mut initial.agriculture.as_mut().unwrap().policies {
        policy.willing = false;
    }
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.farm_field(1, 1).unwrap());
                assert_eq!(
                    world.snapshot().agriculture.as_ref().unwrap().fields[0].phase,
                    CropPhase::Growing
                );
                world.advance().unwrap();
                assert_eq!(
                    world.snapshot().agriculture.as_ref().unwrap().fields[0].phase,
                    CropPhase::Ripe
                );
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored = Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                assert!(world.farm_field(1, 1).unwrap());
                assert!(restored.farm_field(1, 1).unwrap());
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(
                    state.agriculture.as_ref().unwrap().fields[0].phase,
                    CropPhase::Fallow
                );
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Stock { region: 1, good: 3 })
                        .unwrap()
                        .used,
                    1
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 2 })
        .unwrap();
    *stock = Container::new(stock.place, stock.capacity, vec![]).unwrap();
    let mut world = Economy::compile(initial).unwrap();
    assert!(!world.farm_field(1, 1).unwrap());
    assert_eq!(world.tick(), 1);
    assert_eq!(
        world.snapshot().agriculture.unwrap().fields[0].phase,
        CropPhase::Fallow
    );
}

#[test]
fn people_and_wildlife_share_a_finite_physical_resource_atomically() {
    use game_parallel_genetics::{Genome, Sex};
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    initial.nutrition = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.gathering = Some(Gathering {
        inherit_births: false,
        max_hunger: 20,
        nodes: vec![ResourceNode {
            id: 1,
            region: 1,
            good: 1,
            available: 3,
            capacity: 3,
            regrowth: 0,
            yield_per_action: 2,
        }],
        policies: vec![GatherPolicy {
            person: 2,
            node: 1,
            willing: true,
            pressured_by: None,
            pressure_threshold: None,
        }],
    });
    initial.fauna = Some(Fauna {
        last_tick: 0,
        next_id: 3,
        species: vec![AnimalSpecies {
            id: 1,
            name: "grazer".into(),
            food_good: 1,
            intake: 2,
            hunger_per_tick: 1,
            relief_per_unit: 1,
            starvation_threshold: 10,
            starvation_damage: 1,
            max_health: 10,
            adult_age: 1,
            fertile_until: 10,
            lifespan: 30,
            gestation: 2,
            conception_one_in: 10,
            breeding_max_hunger: 5,
            newborn_health: 10,
            mutation_per_million: 0,
            mutation_step: 0,
            climate: None,
            hunting: None,
        }],
        animals: (1..=2)
            .map(|id| Animal {
                id,
                species: 1,
                region: 1,
                age: 1,
                health: 10,
                hunger: 1,
                genome: Genome {
                    sex: Sex::Male,
                    alleles: [[100; 2]; 4],
                },
                pregnancy: None,
                parents: None,
                social_response: None,
                opinions: vec![],
            })
            .collect(),
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let stale = world.prepare().unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert!(world.commit(stale).is_err());
                assert_eq!(world.snapshot(), state);
                assert_eq!(state.gathering.as_ref().unwrap().nodes[0].available, 0);
                let stock = state
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|c| c.place == Place::Stock { region: 1, good: 1 })
                    .unwrap();
                assert_eq!(stock.used, 2);
                assert_eq!(
                    state
                        .fauna
                        .as_ref()
                        .unwrap()
                        .animals
                        .iter()
                        .map(|a| 2 - a.hunger)
                        .sum::<u32>(),
                    1
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn gathering_depletes_real_sites_only_when_storage_accepts_the_output() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    initial.nutrition = None;
    initial.people[0].controlled = true;
    for p in &mut initial.people {
        p.seek_meals = false;
    }
    initial.gathering = Some(Gathering {
        inherit_births: false,
        max_hunger: 20,
        nodes: vec![ResourceNode {
            id: 1,
            region: 1,
            good: 2,
            available: 4,
            capacity: 4,
            regrowth: 0,
            yield_per_action: 1,
        }],
        policies: (2..=4)
            .map(|person| GatherPolicy {
                person,
                node: 1,
                willing: true,
                pressured_by: None,
                pressure_threshold: None,
            })
            .collect(),
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.gather(1, 1).unwrap());
                let saved = world.snapshot();
                assert_eq!(saved.gathering.as_ref().unwrap().nodes[0].available, 0);
                assert_eq!(
                    saved
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Stock { region: 1, good: 2 })
                        .unwrap()
                        .used,
                    6
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                assert!(!world.gather(1, 1).unwrap());
                assert!(!restored.gather(1, 1).unwrap());
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
    let mut choice = Economy::compile(initial.clone()).unwrap();
    let stale = choice.prepare().unwrap();
    choice.set_gathering_choice(2, 1, false).unwrap();
    assert!(choice.commit(stale).is_err());
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 2 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        10,
        vec![Lot {
            good: 2,
            quantity: 9,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut world = Economy::compile(initial).unwrap();
    world.gather(1, 1).unwrap();
    let state = world.snapshot();
    assert_eq!(state.gathering.as_ref().unwrap().nodes[0].available, 3);
    assert_eq!(
        state
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Stock { region: 1, good: 2 })
            .unwrap()
            .used,
        10
    );
}

#[test]
fn gathered_outputs_become_personal_claims_only_after_real_delivery() {
    use game_parallel_economy_claims::Claims;
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    initial.people[0].controlled = true;
    initial.people[0].seek_meals = false;
    initial.dynamics = Some(Dynamics {
        personality: None,
        social: None,
        disease: None,
        treatment: vec![],
        claims: Some(Claims::default()),
    });
    initial.gathering = Some(Gathering {
        inherit_births: false,
        max_hunger: 20,
        nodes: vec![ResourceNode {
            id: 1,
            region: 1,
            good: 2,
            available: 2,
            capacity: 2,
            regrowth: 0,
            yield_per_action: 1,
        }],
        policies: vec![],
    });
    let mut world = Economy::compile(initial.clone()).unwrap();
    world.set_output_claim_choice(1, true).unwrap();
    assert!(world.gather(1, 1).unwrap());
    assert!(
        world
            .snapshot()
            .dynamics
            .unwrap()
            .claims
            .unwrap()
            .claims
            .iter()
            .any(|c| c.person == 1 && c.region == 1 && c.good == 2)
    );
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 2 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        10,
        vec![Lot {
            good: 2,
            quantity: 10,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut world = Economy::compile(initial).unwrap();
    world.set_output_claim_choice(1, true).unwrap();
    assert!(!world.gather(1, 1).unwrap());
    assert!(
        world
            .snapshot()
            .dynamics
            .unwrap()
            .claims
            .unwrap()
            .claims
            .is_empty()
    );
    assert_eq!(world.snapshot().gathering.unwrap().nodes[0].available, 2);
}

#[test]
fn fear_can_motivate_gathering_and_renewable_sites_regrow() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    for p in &mut initial.people {
        p.seek_meals = false;
    }
    initial.opinions.as_mut().unwrap().records = vec![Opinion {
        observer: 2,
        target: 1,
        fear: 5,
        obligation: 0,
        trust: 0,
        affinity: 0,
    }];
    initial.gathering = Some(Gathering {
        inherit_births: false,
        max_hunger: 20,
        nodes: vec![ResourceNode {
            id: 1,
            region: 1,
            good: 2,
            available: 3,
            capacity: 3,
            regrowth: 1,
            yield_per_action: 2,
        }],
        policies: vec![GatherPolicy {
            person: 2,
            node: 1,
            willing: false,
            pressured_by: Some(1),
            pressure_threshold: Some(3),
        }],
    });
    let mut world = Economy::compile(initial.clone()).unwrap();
    world.advance().unwrap();
    assert_eq!(world.snapshot().gathering.unwrap().nodes[0].available, 2);
    initial.opinions.as_mut().unwrap().records[0].fear = 0;
    let mut world = Economy::compile(initial).unwrap();
    world.advance().unwrap();
    assert_eq!(world.snapshot().gathering.unwrap().nodes[0].available, 3);
}

#[test]
fn farming_and_gathering_share_one_atomic_stock_boundary() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    for p in &mut initial.people {
        p.seek_meals = false;
    }
    initial.agriculture = Some(Agriculture {
        inherit_births: false,
        max_hunger: 20,
        crops: vec![Crop {
            id: 1,
            seed: 2,
            seed_quantity: 1,
            output: 1,
            output_quantity: 1,
            growth_ticks: 1,
        }],
        fields: vec![Field {
            id: 1,
            region: 1,
            crop: 1,
            phase: CropPhase::Fallow,
            remaining: 0,
        }],
        policies: vec![FarmPolicy {
            person: 2,
            willing: true,
            pressured_by: None,
            pressure_threshold: None,
        }],
    });
    initial.gathering = Some(Gathering {
        inherit_births: false,
        max_hunger: 20,
        nodes: vec![ResourceNode {
            id: 1,
            region: 1,
            good: 2,
            available: 1,
            capacity: 1,
            regrowth: 0,
            yield_per_action: 1,
        }],
        policies: vec![GatherPolicy {
            person: 3,
            node: 1,
            willing: true,
            pressured_by: None,
            pressure_threshold: None,
        }],
    });
    let mut world = Economy::compile(initial).unwrap();
    let events = world.advance().unwrap();
    assert_eq!(
        events
            .physical_actions
            .iter()
            .filter(|action| action.accepted)
            .count(),
        2
    );
    let state = world.snapshot();
    assert_eq!(
        state.agriculture.unwrap().fields[0].phase,
        CropPhase::Growing
    );
    assert_eq!(state.gathering.unwrap().nodes[0].available, 0);
    assert_eq!(
        state
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Stock { region: 1, good: 2 })
            .unwrap()
            .used,
        2
    );
}

fn gathering_workload(count: u64) -> State {
    let mut initial = field_workload(count);
    initial.agriculture = None;
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 2 })
        .unwrap();
    *stock = Container::new(stock.place, count * 2, vec![]).unwrap();
    initial.gathering = Some(Gathering {
        inherit_births: false,
        max_hunger: 20,
        nodes: vec![ResourceNode {
            id: 1,
            region: 1,
            good: 2,
            available: count / 2,
            capacity: count,
            regrowth: 0,
            yield_per_action: 1,
        }],
        policies: (1..=count)
            .map(|person| GatherPolicy {
                person,
                node: 1,
                willing: true,
                pressured_by: None,
                pressure_threshold: None,
            })
            .collect(),
    });
    initial
}

#[test]
fn concentrated_gathering_depletes_once_and_matches_across_workers() {
    let count = 4096;
    let initial = gathering_workload(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let saved = world.snapshot();
                assert_eq!(saved.gathering.as_ref().unwrap().nodes[0].available, 0);
                assert_eq!(
                    saved
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Stock { region: 1, good: 2 })
                        .unwrap()
                        .used,
                    count / 2
                );
                world.advance().unwrap();
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_gathering_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = gathering_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

fn gathering_claims_workload(count: u64) -> State {
    use game_parallel_economy_claims::{Claims, Preference};
    let mut state = gathering_workload(count);
    state.dynamics = Some(Dynamics {
        personality: None,
        social: None,
        disease: None,
        treatment: vec![],
        claims: Some(Claims {
            inherit_births: false,
            site_claims: vec![],
            site_events: vec![],
            preferences: (1..=count)
                .map(|person| Preference {
                    person,
                    disregard_others: false,
                    assert_outputs: true,
                    assert_built_sites: false,
                    desperate_hunger: None,
                })
                .collect(),
            ..Default::default()
        }),
    });
    state
}

#[test]
fn concentrated_gathering_claims_match_across_workers() {
    let count = 4096;
    let initial = gathering_claims_workload(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                assert_eq!(
                    world
                        .snapshot()
                        .dynamics
                        .unwrap()
                        .claims
                        .unwrap()
                        .claims
                        .len(),
                    count as usize / 2
                );
                world.advance().unwrap();
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_gathering_claims_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = gathering_claims_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                for _ in 0..2 {
                    world.advance().unwrap();
                }
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=2 elapsed={:.6}",
                samples[1]
            );
        });
}

fn fatigue_rules() -> game_parallel_fatigue::Rules {
    game_parallel_fatigue::Rules {
        capacity: 20,
        baseline_gain: 0,
        work_gain: 2,
        travel_gain: 0,
        idle_recovery: 0,
        rest_recovery: 3,
        rest_at: 5,
        resume_at: 2,
        exhaustion_at: 20,
        exhaustion_damage: 1,
    }
}

#[test]
fn travel_exhaustion_can_kill_a_traveler_and_leave_their_possessions() {
    let mut initial = expedition_fixture();
    initial.missions = None;
    initial.dynamics = None;
    initial.people[1].health = 5;
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(2))
        .unwrap();
    *holder = Container::new(
        holder.place,
        holder.capacity,
        vec![Lot {
            good: 2,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    initial.travel.as_mut().unwrap().routes[0].ticks = 4;
    let mut rules = fatigue_rules();
    rules.travel_gain = 3;
    rules.exhaustion_at = 6;
    rules.exhaustion_damage = 4;
    initial.fatigue = Some(Fatigue {
        rules,
        people: vec![],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world
                    .depart(&[Departure {
                        person: 2,
                        destination: 2,
                    }])
                    .unwrap();
                world.advance().unwrap();
                let saved = world.snapshot();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                for _ in 0..2 {
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(
                    (state.people[1].health, state.people[1].region),
                    (0, Some(1))
                );
                assert!(state.travel.as_ref().unwrap().journeys.is_empty());
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap()
                        .used,
                    1
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

fn traveling_fatigue_workload(count: u64) -> State {
    let mut state = field_workload(count);
    state.agriculture = None;
    state
        .physical
        .custody
        .containers
        .push(Container::new(Place::Stock { region: 2, good: 2 }, count * 2, vec![]).unwrap());
    state
        .physical
        .custody
        .containers
        .sort_unstable_by_key(|c| c.place);
    for p in &mut state.people {
        p.region = None;
    }
    state.travel = Some(Travel {
        routes: vec![Route {
            source: 1,
            destination: 2,
            ticks: 10,
        }],
        journeys: (1..=count)
            .map(|person| Journey {
                person,
                source: 1,
                destination: 2,
                remaining: 10,
            })
            .collect(),
    });
    let mut rules = fatigue_rules();
    rules.travel_gain = 1;
    state.fatigue = Some(Fatigue {
        rules,
        people: vec![],
    });
    state
}

fn traveling_social_workload(count: u64) -> State {
    use game_parallel_social::{Participant, Rules as SocialRules, Social};
    let mut state = traveling_fatigue_workload(count);
    state.dynamics = Some(Dynamics {
        personality: None,
        disease: None,
        treatment: vec![],
        claims: None,
        social: Some(Social {
            rules: SocialRules {
                interval: 1,
                min_health: 1,
                max_hunger: 100,
                familiarity_gain: 1,
                familiarity_max: 10,
                relationship_retention: None,
            },
            participants: (1..=count)
                .map(|person| Participant {
                    person,
                    willing: true,
                })
                .collect(),
            pressures: vec![],
            relationships: vec![],
            events: vec![],
            needs: None,
        }),
    });
    state
}

#[test]
fn traveling_social_encounters_scale_deterministically() {
    let count = 4096;
    let initial = traveling_social_workload(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .social
                        .as_ref()
                        .unwrap()
                        .events
                        .len(),
                    count as usize / 2
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn traveling_social_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = traveling_social_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                for _ in 0..2 {
                    world.advance().unwrap();
                }
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=2 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn traveling_fatigue_matches_across_workers() {
    let initial = traveling_fatigue_workload(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                for _ in 0..2 {
                    world.advance().unwrap();
                }
                let state = world.snapshot();
                assert!(
                    state
                        .fatigue
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .all(|p| p.fatigue == 2)
                );
                assert_eq!(state.fatigue.as_ref().unwrap().people.len(), 4096);
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn traveling_fatigue_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = traveling_fatigue_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                for _ in 0..2 {
                    world.advance().unwrap();
                }
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=2 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn autonomous_rest_pauses_work_and_controlled_rest_is_a_choice() {
    use game_parallel_fatigue::Person as Tired;
    let mut initial = fixture();
    initial.agreements = None;
    initial.people[0].controlled = true;
    for person in &mut initial.people {
        person.seek_meals = false;
        person.hunger = 0;
    }
    initial.physical.recipes[0].ticks = 4;
    initial.fatigue = Some(Fatigue {
        rules: fatigue_rules(),
        people: vec![
            Tired {
                id: 1,
                fatigue: 8,
                resting: false,
            },
            Tired {
                id: 2,
                fatigue: 4,
                resting: false,
            },
        ],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert_eq!(world.advance().unwrap().started, vec![1]);
                let state = world.snapshot();
                let tired = &state.fatigue.as_ref().unwrap().people;
                assert!(!tired.iter().find(|p| p.id == 1).unwrap().resting);
                assert!(tired.iter().find(|p| p.id == 2).unwrap().resting);
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                world.rest(1).unwrap();
                restored.rest(1).unwrap();
                assert_eq!(world.snapshot().physical.work[0].job.remaining, 4);
                assert_eq!(
                    world
                        .snapshot()
                        .fatigue
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .find(|p| p.id == 1)
                        .unwrap()
                        .fatigue,
                    5
                );
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot().physical.work[0].job.remaining, 4);
                assert!(
                    !world
                        .snapshot()
                        .fatigue
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .find(|p| p.id == 1)
                        .unwrap()
                        .resting
                );
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot().physical.work[0].job.remaining, 3);
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn exhaustion_death_updates_lineage_and_does_not_create_a_successor() {
    let mut initial = campaign_fixture();
    initial.pending_food.clear();
    initial.people[2].health = 1;
    initial.fatigue = Some(Fatigue {
        rules: fatigue_rules(),
        people: vec![game_parallel_fatigue::Person {
            id: 3,
            fatigue: 20,
            resting: false,
        }],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state.people[2].health, 0);
                assert!(
                    !state
                        .family
                        .as_ref()
                        .unwrap()
                        .members
                        .iter()
                        .find(|m| m.id == 3)
                        .unwrap()
                        .alive
                );
                assert_eq!(state.campaign.as_ref().unwrap().lineage.current, None);
                assert_eq!(state.people.len(), 4);
                world.advance().unwrap();
                assert_eq!(
                    world.snapshot().campaign.as_ref().unwrap().lineage.current,
                    Some(5)
                );
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn concentrated_fatigue_matches_across_workers() {
    let mut initial = concentrated(4096);
    initial.fatigue = Some(Fatigue {
        rules: fatigue_rules(),
        people: vec![],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                let state = world.snapshot();
                assert!(!state.fatigue.as_ref().unwrap().people.is_empty());
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_fatigue_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let mut initial = concentrated(count);
    initial.fatigue = Some(Fatigue {
        rules: fatigue_rules(),
        people: vec![],
    });
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=3 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
#[ignore]
fn concentrated_exposure_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let mut initial = concentrated(count);
    initial.environment = Some(Environment {
        regions: vec![Hazard {
            region: 1,
            damage: 1,
            shelter: (count / 2) as u32,
            shelter_reduction: 1,
            shelter_good: None,
        }],
        seasons: vec![],
    });
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=3 elapsed={:.6}",
                samples[1]
            );
        });
}
#[test]
fn concentrated_fields_compete_for_seed_and_publish_only_actual_harvests() {
    let count = 4096;
    let initial = field_workload(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let first = world.advance().unwrap();
                assert_eq!(
                    first
                        .physical_actions
                        .iter()
                        .filter(|row| row.accepted)
                        .count(),
                    count as usize / 2
                );
                world.advance().unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Stock { region: 1, good: 3 })
                        .unwrap()
                        .used,
                    count / 2
                );
                assert!(
                    state
                        .agriculture
                        .as_ref()
                        .unwrap()
                        .fields
                        .iter()
                        .all(|field| field.phase == CropPhase::Fallow)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}
#[test]
#[ignore]
fn concentrated_field_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = field_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                assert_eq!(
                    world
                        .advance()
                        .unwrap()
                        .physical_actions
                        .iter()
                        .filter(|row| row.accepted)
                        .count(),
                    count as usize / 2
                );
                world.advance().unwrap();
                assert_eq!(
                    world
                        .advance()
                        .unwrap()
                        .physical_actions
                        .iter()
                        .filter(|row| row.accepted)
                        .count(),
                    count as usize / 2
                );
                let elapsed = started.elapsed().as_secs_f64();
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} fields={count} ticks=3 elapsed={:.6}",
                samples[1]
            );
        });
}
#[test]
fn combined_personal_food_and_work_match_across_workers() {
    let initial = concentrated(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut economy = Economy::compile(initial.clone()).unwrap();
                assert_eq!(economy.advance().unwrap().started.len(), 1024);
                assert_eq!(economy.advance().unwrap().finished.len(), 1024);
                assert_eq!(economy.advance().unwrap().delivered.len(), 1024);
                economy.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}
#[test]
#[ignore]
fn concentrated_economy_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = concentrated(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut economy = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                assert_eq!(economy.advance().unwrap().started.len(), count as usize / 4);
                assert_eq!(
                    economy.advance().unwrap().finished.len(),
                    count as usize / 4
                );
                assert_eq!(
                    economy.advance().unwrap().delivered.len(),
                    count as usize / 4
                );
                let elapsed = started.elapsed().as_secs_f64();
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=3 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
#[ignore]
fn concentrated_family_care_benchmark() {
    use game_parallel_dependent_care::{Policy as CarePolicy, Rules as CareRules};
    use game_parallel_families::{Families, FamilyRules, FamilyTotals, Genome, Member, Sex};
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let adults = count * 4 / 5;
    let mut initial = concentrated(count);
    for person in &mut initial.people {
        person.seek_meals = person.id > adults;
        person.hunger = if person.id > adults { 5 } else { 0 };
    }
    let members = (1..=count)
        .map(|id| {
            let child = id > adults;
            let offset = id.saturating_sub(adults + 1);
            Member {
                id,
                born: None,
                age: if child { 2 } else { 20 },
                lifespan_loss: 0,
                genome: Genome {
                    sex: if id % 2 == 0 { Sex::Male } else { Sex::Female },
                    alleles: [[500; 2]; 4],
                },
                parents: if child {
                    vec![2 * offset + 1, 2 * offset + 2].into()
                } else {
                    Default::default()
                },
                partner: None,
                pregnancy: None,
                next_conception: 0,
                alive: true,
                reproduction_allowed: false,
                reconsider_partnership: false,
            }
        })
        .collect();
    initial.family = Some(Families {
        rules: FamilyRules {
            adult_age: 10,
            min_work_age: 10,
            fertile_until: 50,
            gestation: 3,
            birth_spacing: 5,
            lifespan_min: 60,
            lifespan_max: 100,
            conception_one_in: 100,
            minimum_health: 1,
            maximum_hunger: 100,
            newborn_health: 100,
            mutation_per_million: 0,
            mutation_step: 0,
            enroll_births: false,
        },
        members,
        next_id: count + 1,
        last_tick: 0,
        totals: FamilyTotals::default(),
        events: vec![],
    });
    initial.care = Some(DependentCare {
        rules: CareRules {
            dependent_until: 5,
            max_caregiver_hunger: 100,
            neglect_damage: 0,
            feed_from_cargo: true,
            fetch_local_food: false,
            pressure_threshold: None,
        },
        inherit_births: false,
        policies: (1..=(count - adults) * 2)
            .map(|person| CarePolicy {
                person,
                willing: true,
            })
            .collect(),
        events: vec![],
    });
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                let first = world.advance().unwrap();
                assert_eq!(
                    first
                        .physical_actions
                        .iter()
                        .filter(|row| row.accepted)
                        .count(),
                    (count - adults) as usize
                );
                world.advance().unwrap();
                world.advance().unwrap();
                let elapsed = started.elapsed().as_secs_f64();
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} dependents={} ticks=3 elapsed={:.6}",
                count - adults,
                samples[1]
            );
        });
}

#[test]
fn fear_can_start_requested_work_and_the_worker_can_leave_when_fear_fades() {
    use game_parallel_autowork::requests::{Preference, Request, Requests};
    let mut initial = fixture();
    initial.agreements = None;
    initial.dynamics = Some(Dynamics {
        personality: Some(game_parallel_economy_dynamics::personality::Personality {
            social_urgency_per_isolation: 0,
            inherit_births: false,
            people: vec![game_parallel_economy_dynamics::personality::Person {
                person: 2,
                sociability: 0,
                diligence: 0,
            }],
        }),
        social: None,
        disease: None,
        treatment: vec![],
        claims: None,
    });
    initial.people[1].willing_to_work = false;
    initial.physical.recipes[0].ticks = 2;
    initial.opinions.as_mut().unwrap().rules.fear_decay_per_tick = 5;
    initial.opinions.as_mut().unwrap().records = vec![Opinion {
        observer: 2,
        target: 1,
        trust: 0,
        fear: 10,
        obligation: 0,
        affinity: 0,
    }];
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: Some(Requests {
            inherit_births: false,
            next_id: 2,
            memory_limit: 0,
            work_links: vec![],
            memories: vec![],
            preferences: vec![Preference {
                person: 2,
                effort: 5,
                help_value: 0,
                affinity_weight: 0,
                trust_weight: 0,
                offer_value: None,
                disregard_claims: false,
            }],
            askers: vec![],
            pending: vec![Request {
                id: 1,
                requester: 1,
                worker: 2,
                region: 1,
                recipe: 1,
                created_at: 0,
                expires: 3,
            }],
            events: vec![],
        }),
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut economy = Economy::compile(initial.clone()).unwrap();
                assert_eq!(economy.advance().unwrap().started, vec![1]);
                let after_start = economy.snapshot();
                let requests = after_start
                    .autowork
                    .as_ref()
                    .unwrap()
                    .requests
                    .as_ref()
                    .unwrap();
                assert_eq!(requests.work_links.len(), 1);
                assert!(requests.memories.is_empty());
                let mut forged = after_start.clone();
                forged
                    .autowork
                    .as_mut()
                    .unwrap()
                    .requests
                    .as_mut()
                    .unwrap()
                    .work_links[0]
                    .job += 1;
                assert!(Economy::compile(forged).is_err());
                let saved = serde_json::to_string(&after_start).unwrap();
                let mut economy = Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                assert!(economy.advance().unwrap().abandoned.is_empty());
                assert_eq!(economy.snapshot().physical.work[0].job.remaining, 1);
                assert_eq!(
                    economy.snapshot().opinions.as_ref().unwrap().records[0].fear,
                    0
                );
                let events = economy.advance().unwrap();
                assert_eq!(events.abandoned, vec![1]);
                assert!(
                    economy
                        .snapshot()
                        .autowork
                        .as_ref()
                        .unwrap()
                        .requests
                        .as_ref()
                        .unwrap()
                        .work_links
                        .is_empty()
                );
                economy.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
    let mut fatal = initial.clone();
    fatal.pending_food.push(FoodEffect {
        person: 2,
        at: 1,
        health: -100,
        nutrients: [0; 6],
    });
    let mut economy = Economy::compile(fatal).unwrap();
    let events = economy.advance().unwrap();
    assert_eq!(events.started, vec![1]);
    assert_eq!(events.interrupted, vec![1]);
    let saved = economy.snapshot();
    assert_eq!(saved.people[1].health, 0);
    assert_eq!(
        saved.physical.work[0].phase,
        game_parallel_lot_runtime::Phase::Returning
    );
    assert!(
        saved
            .autowork
            .as_ref()
            .unwrap()
            .requests
            .as_ref()
            .unwrap()
            .work_links
            .is_empty()
    );
    let mut restored = Economy::compile(saved).unwrap();
    assert_eq!(restored.advance().unwrap().returned, vec![1]);
    let mut no_fear = initial;
    no_fear.opinions.as_mut().unwrap().records[0].fear = 0;
    let mut economy = Economy::compile(no_fear).unwrap();
    assert!(economy.advance().unwrap().started.is_empty());
    assert!(matches!(
        economy
            .snapshot()
            .autowork
            .unwrap()
            .requests
            .unwrap()
            .events[0]
            .outcome,
        game_parallel_autowork::requests::Outcome::Refused
    ));
}

#[test]
fn affinity_can_motivate_requested_work_and_a_changed_view_can_end_it() {
    use game_parallel_autowork::requests::{Preference, Request, Requests};
    let mut initial = fixture();
    initial.agreements = None;
    initial.people[1].willing_to_work = false;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.physical.recipes[0].ticks = 3;
    let opinions = initial.opinions.as_mut().unwrap();
    opinions.rules.affinity = Some(game_parallel_opinions::AffinityRules {
        limit: 20,
        encounter_change: 0,
        pressured_encounter_change: 0,
        help_change: 0,
        harm_change_per_damage: 0,
    });
    opinions.records = vec![Opinion {
        observer: 2,
        target: 1,
        trust: 0,
        fear: 0,
        obligation: 0,
        affinity: 2,
    }];
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: Some(Requests {
            inherit_births: false,
            next_id: 2,
            memory_limit: 2,
            work_links: vec![],
            memories: vec![],
            preferences: vec![Preference {
                person: 2,
                effort: 5,
                help_value: 0,
                affinity_weight: 3,
                trust_weight: 0,
                offer_value: None,
                disregard_claims: false,
            }],
            askers: vec![],
            pending: vec![Request {
                id: 1,
                requester: 1,
                worker: 2,
                region: 1,
                recipe: 1,
                created_at: 0,
                expires: 3,
            }],
            events: vec![],
        }),
    });
    let run = |workers, weight, affinity| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = initial.clone();
                state
                    .autowork
                    .as_mut()
                    .unwrap()
                    .requests
                    .as_mut()
                    .unwrap()
                    .preferences[0]
                    .affinity_weight = weight;
                state.opinions.as_mut().unwrap().records[0].affinity = affinity;
                let mut world = Economy::compile(state).unwrap();
                assert_eq!(world.advance().unwrap().started, vec![1]);
                let mut state = world.snapshot();
                state.opinions.as_mut().unwrap().records[0].affinity = -affinity;
                let mut world = Economy::compile(state).unwrap();
                assert_eq!(world.advance().unwrap().abandoned, vec![1]);
                let state = world.snapshot();
                assert_eq!(state.opinions.as_ref().unwrap().index().obligation(2, 1), 0);
                state
            })
    };
    assert_eq!(run(1, 3, 2), run(4, 3, 2));
    assert_eq!(run(1, -3, -2), run(4, -3, -2));
}

#[test]
fn an_individual_can_value_a_revocable_offer_when_choosing_work() {
    use game_parallel_autowork::requests::{Preference, Requests};
    let mut initial = fixture();
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.people[0].hunger = 0;
    initial.people[1].willing_to_work = false;
    initial.physical.recipes[0].ticks = 3;
    initial.opinions.as_mut().unwrap().records.clear();
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: Some(Requests {
            next_id: 1,
            inherit_births: false,
            memory_limit: 1,
            work_links: vec![],
            memories: vec![],
            preferences: vec![Preference {
                person: 2,
                effort: 5,
                help_value: 0,
                affinity_weight: 0,
                trust_weight: 0,
                offer_value: Some(WorkOfferValue {
                    good: 1,
                    value_per_unit: 3,
                    max_quantity: 2,
                }),
                disregard_claims: false,
            }],
            askers: vec![],
            pending: vec![],
            events: vec![],
        }),
    });
    let agreements = initial.agreements.as_mut().unwrap();
    agreements.next_id = 1;
    agreements.contracts.clear();
    agreements.events.clear();
    agreements.policies.clear();
    let proposal = |creditor, good, quantity| PromiseProposal {
        debtor: 1,
        creditor,
        good,
        quantity,
        due: 10,
    };
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.propose_promises(&[proposal(2, 1, 2)]).unwrap();
                world.ask_work(1, 2, 1, 5).unwrap();
                assert_eq!(world.advance().unwrap().started, vec![1]);
                let state = world.snapshot();
                assert_eq!(
                    state.agreements.as_ref().unwrap().contracts[0].status,
                    Status::Proposed
                );
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Person(1))
                        .unwrap()
                        .used,
                    1
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                for sim in [&mut world, &mut restored] {
                    sim.decide_promises(&[PromiseDecision {
                        contract: 1,
                        actor: 1,
                        accept: false,
                    }])
                    .unwrap();
                    assert_eq!(sim.advance().unwrap().abandoned, vec![1]);
                }
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
    for (creditor, good, quantity) in [(3, 1, 2), (2, 2, 2), (2, 1, 1)] {
        let mut world = Economy::compile(initial.clone()).unwrap();
        world
            .propose_promises(&[proposal(creditor, good, quantity)])
            .unwrap();
        world.ask_work(1, 2, 1, 5).unwrap();
        assert!(world.advance().unwrap().started.is_empty());
    }
    let mut negative = initial.clone();
    let preference = &mut negative
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .preferences[0];
    preference.help_value = 6;
    preference.offer_value.as_mut().unwrap().value_per_unit = -3;
    let mut world = Economy::compile(negative).unwrap();
    world.propose_promises(&[proposal(2, 1, 1)]).unwrap();
    world.ask_work(1, 2, 1, 5).unwrap();
    assert!(world.advance().unwrap().started.is_empty());
    let mut world = Economy::compile(initial).unwrap();
    let before = world.snapshot();
    assert!(
        world
            .set_work_offer_value(
                2,
                Some(WorkOfferValue {
                    good: 1,
                    value_per_unit: 1,
                    max_quantity: 0
                })
            )
            .is_err()
    );
    assert_eq!(world.snapshot(), before);
    world.set_work_offer_value(2, None).unwrap();
    world.propose_promises(&[proposal(2, 1, 2)]).unwrap();
    world.ask_work(1, 2, 1, 5).unwrap();
    assert!(world.advance().unwrap().started.is_empty());
}

#[test]
fn experienced_reliability_can_change_a_persons_choice_to_help() {
    use game_parallel_autowork::requests::{Preference, Requests};
    let mut initial = fixture();
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.people[0].hunger = 0;
    initial.people[1].willing_to_work = false;
    initial.physical.recipes[0].ticks = 3;
    initial.opinions.as_mut().unwrap().records.clear();
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: Some(Requests {
            next_id: 1,
            inherit_births: false,
            memory_limit: 2,
            work_links: vec![],
            memories: vec![],
            preferences: vec![Preference {
                person: 2,
                effort: 1,
                help_value: 0,
                affinity_weight: 0,
                trust_weight: 1,
                offer_value: None,
                disregard_claims: false,
            }],
            askers: vec![],
            pending: vec![],
            events: vec![],
        }),
    });
    let agreements = initial.agreements.as_mut().unwrap();
    agreements.next_id = 1;
    agreements.contracts.clear();
    agreements.events.clear();
    agreements.policies = vec![AgreementPolicy {
        person: 1,
        honor: false,
        accept: false,
        minimum_trust: None,
        minimum_honor_trust: None,
        fear_honor_threshold: None,
        obligation_honor_threshold: None,
        accept_pressure_threshold: None,
    }];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.ask_work(1, 2, 1, 5).unwrap();
                assert!(world.advance().unwrap().started.is_empty());
                let id = world
                    .propose_promises(&[PromiseProposal {
                        debtor: 1,
                        creditor: 2,
                        good: 1,
                        quantity: 1,
                        due: 10,
                    }])
                    .unwrap()[0];
                world
                    .decide_promises(&[PromiseDecision {
                        contract: id,
                        actor: 2,
                        accept: true,
                    }])
                    .unwrap();
                world.set_promise_choice(1, true).unwrap();
                world.advance().unwrap();
                assert_eq!(
                    world
                        .snapshot()
                        .opinions
                        .as_ref()
                        .unwrap()
                        .index()
                        .trust(2, 1),
                    1
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&world.snapshot()).unwrap())
                        .unwrap(),
                )
                .unwrap();
                for sim in [&mut world, &mut restored] {
                    sim.ask_work(1, 2, 1, 5).unwrap();
                    assert_eq!(sim.advance().unwrap().started, vec![1]);
                }
                assert_eq!(world.snapshot(), restored.snapshot());
                let mut changed = world.snapshot();
                changed
                    .opinions
                    .as_mut()
                    .unwrap()
                    .records
                    .iter_mut()
                    .find(|r| r.observer == 2 && r.target == 1)
                    .unwrap()
                    .trust = 0;
                let mut changed = Economy::compile(changed).unwrap();
                assert_eq!(changed.advance().unwrap().abandoned, vec![1]);
                changed.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
    let mut contrary = initial;
    contrary.opinions.as_mut().unwrap().rules.initial = -2;
    contrary
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .preferences[0]
        .trust_weight = -1;
    let mut contrary = Economy::compile(contrary).unwrap();
    contrary.ask_work(1, 2, 1, 5).unwrap();
    assert_eq!(contrary.advance().unwrap().started, vec![1]);
}

#[test]
fn diligence_influences_starting_work_without_cancelling_an_existing_commitment() {
    use game_parallel_economy_dynamics::personality::{Person as Disposition, Personality};
    let mut initial = fixture();
    initial.agreements = None;
    initial.people[0].seek_meals = false;
    initial.physical.recipes[0].ticks = 2;
    initial.dynamics = Some(Dynamics {
        personality: Some(Personality {
            social_urgency_per_isolation: 0,
            inherit_births: false,
            people: vec![Disposition {
                person: 2,
                sociability: 0,
                diligence: 0,
            }],
        }),
        social: None,
        disease: None,
        treatment: vec![],
        claims: None,
    });
    let mut reluctant = Economy::compile(initial.clone()).unwrap();
    assert!(reluctant.advance().unwrap().started.is_empty());
    initial
        .dynamics
        .as_mut()
        .unwrap()
        .personality
        .as_mut()
        .unwrap()
        .people[0]
        .diligence = 1000;
    let mut willing = Economy::compile(initial).unwrap();
    assert_eq!(willing.advance().unwrap().started, vec![1]);
    let mut saved = willing.snapshot();
    saved
        .dynamics
        .as_mut()
        .unwrap()
        .personality
        .as_mut()
        .unwrap()
        .people[0]
        .diligence = 0;
    let mut world = Economy::compile(saved).unwrap();
    assert!(world.advance().unwrap().abandoned.is_empty());
    assert_eq!(world.snapshot().physical.work[0].job.remaining, 1);
}

#[test]
fn actual_completed_work_relieves_purpose_and_deprivation_can_harm_health() {
    use game_parallel_purpose::{Person as PurposePerson, Rules as PurposeRules};
    let mut initial = fixture();
    initial.agreements = None;
    initial.nutrition = None;
    initial.rules.hunger_per_tick = 0;
    initial.people[0].seek_meals = false;
    initial.purpose = Some(Purpose {
        rules: PurposeRules {
            max: 20,
            per_tick: 1,
            blocked_increase: 2,
            completion_relief: 3,
            damage_threshold: 5,
            damage: 2,
        },
        people: vec![
            PurposePerson {
                person: 2,
                deprivation: 4,
                eligible_at: 0,
            },
            PurposePerson {
                person: 3,
                deprivation: 4,
                eligible_at: 0,
            },
        ],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert_eq!(world.advance().unwrap().started, vec![1]);
                let saved = world.snapshot();
                assert_eq!(saved.people[1].health, 98);
                assert_eq!(saved.people[2].health, 98);
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                let next = world.snapshot();
                assert_eq!(next.people[1].health, 98);
                assert_eq!(next.people[2].health, 96);
                assert_eq!(next.purpose.as_ref().unwrap().people[0].deprivation, 3);
                next
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn recovery_uses_real_rest_and_does_not_revive_or_heal_starving_people() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    initial.nutrition = None;
    initial.rules.hunger_per_tick = 0;
    for p in &mut initial.people {
        p.seek_meals = false;
    }
    initial.people[0].controlled = true;
    initial.people[0].health = 50;
    initial.people[1].health = 95;
    initial.people[2].health = 0;
    initial.people[3].health = 50;
    initial.people[3].hunger = 20;
    initial.fatigue = Some(Fatigue {
        rules: fatigue_rules(),
        people: vec![],
    });
    initial.recovery = Some(RecoveryRules {
        interval: 1,
        health_per_interval: 2,
        rest_bonus: 3,
        max_hunger: 5,
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.rest(1).unwrap();
                let saved = world.snapshot();
                assert_eq!(
                    saved.people.iter().map(|p| p.health).collect::<Vec<_>>(),
                    vec![55, 97, 0, 50]
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
    let mut fatal = initial;
    fatal.pending_food.push(FoodEffect {
        person: 1,
        at: 1,
        health: -100,
        nutrients: [0; 6],
    });
    let mut world = Economy::compile(fatal).unwrap();
    world.rest(1).unwrap();
    assert_eq!(world.snapshot().people[0].health, 0);
}

#[test]
fn requested_work_is_learned_only_by_a_present_observer() {
    use game_parallel_autowork::requests::{Preference, Request, Requests};
    use game_parallel_dreams::Learning;
    let mut initial = fixture();
    initial.agreements = None;
    initial.people[1].willing_to_work = false;
    initial.opinions.as_mut().unwrap().records = vec![Opinion {
        observer: 2,
        target: 1,
        trust: 0,
        fear: 5,
        obligation: 0,
        affinity: 0,
    }];
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: Some(Requests {
            inherit_births: false,
            next_id: 2,
            memory_limit: 2,
            work_links: vec![],
            memories: vec![],
            preferences: vec![Preference {
                person: 2,
                effort: 5,
                help_value: 0,
                affinity_weight: 0,
                trust_weight: 0,
                offer_value: None,
                disregard_claims: false,
            }],
            askers: vec![],
            pending: vec![Request {
                id: 1,
                requester: 1,
                worker: 2,
                region: 1,
                recipe: 1,
                created_at: 0,
                expires: 3,
            }],
            events: vec![],
        }),
    });
    initial.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: false,
        people: vec![],
        events: vec![],
    });
    initial.dreams = Some(Dreams {
        inherit_births: false,
        people: vec![],
        learning: vec![Learning {
            person: 1,
            max_aspirations: 2,
            priority: 3,
            batch: 1,
            review_interval: 2,
        }],
        events: vec![],
    });
    let mut economy = Economy::compile(initial).unwrap();
    assert_eq!(economy.advance().unwrap().started, vec![1]);
    let present = economy.snapshot();
    let mut absent = present.clone();
    absent.people[0].region = Some(2);
    let mut absent = Economy::compile(absent).unwrap();
    assert_eq!(economy.advance().unwrap().finished, vec![1]);
    assert_eq!(absent.advance().unwrap().finished, vec![1]);
    let present = economy.snapshot();
    let memory = &present
        .autowork
        .as_ref()
        .unwrap()
        .requests
        .as_ref()
        .unwrap()
        .memories;
    assert_eq!(memory.len(), 1);
    assert_eq!(
        (memory[0].observer, memory[0].worker, memory[0].successes),
        (1, 2, 1)
    );
    assert_eq!(present.dreams.as_ref().unwrap().people[0].recipe, 1);
    assert!(absent.snapshot().dreams.unwrap().people.is_empty());
    let saved = serde_json::to_string(&present).unwrap();
    let mut restored = Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
    economy.advance().unwrap();
    restored.advance().unwrap();
    assert_eq!(economy.snapshot(), restored.snapshot());
    assert!(
        economy
            .snapshot()
            .goals
            .unwrap()
            .people
            .iter()
            .any(|goal| goal.person == 1 && goal.recipe == 1)
    );
    assert!(
        absent
            .snapshot()
            .autowork
            .unwrap()
            .requests
            .unwrap()
            .memories
            .is_empty()
    );
}

#[test]
fn a_person_asks_for_help_with_a_goal_and_learns_from_observed_work() {
    use game_parallel_autowork::requests::{Asker, Preference, Requests};
    use game_parallel_goals::{Goal, Goals};
    let mut initial = fixture();
    initial.agreements = None;
    initial.people[0].seek_meals = false;
    initial.people[0].hunger = 0;
    initial.people[1].willing_to_work = false;
    initial.opinions.as_mut().unwrap().records = vec![Opinion {
        observer: 2,
        target: 1,
        trust: 0,
        fear: 5,
        obligation: 0,
        affinity: 0,
    }];
    initial.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: false,
        people: vec![Goal {
            person: 1,
            recipe: 1,
            priority: 1,
            target: 1,
            progress: 0,
            next_review: 0,
            enabled: true,
            recognize_help: false,
        }],
        events: vec![],
    });
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: Some(Requests {
            inherit_births: false,
            next_id: 1,
            memory_limit: 2,
            work_links: vec![],
            memories: vec![],
            preferences: vec![Preference {
                person: 2,
                effort: 5,
                help_value: 0,
                affinity_weight: 0,
                trust_weight: 0,
                offer_value: None,
                disregard_claims: false,
            }],
            askers: vec![Asker {
                person: 1,
                interval: 10,
                lifetime: 5,
                retry_after_offer: false,
            }],
            pending: vec![],
            events: vec![],
        }),
    });
    let run = |workers, recognize| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut economy = Economy::compile(initial.clone()).unwrap();
                economy.set_goal_help_recognition(1, 1, recognize).unwrap();
                assert!(economy.advance().unwrap().started.is_empty());
                let state = economy.snapshot();
                let pending = &state
                    .autowork
                    .as_ref()
                    .unwrap()
                    .requests
                    .as_ref()
                    .unwrap()
                    .pending;
                assert_eq!(
                    (pending[0].requester, pending[0].worker, pending[0].recipe),
                    (1, 2, 1)
                );
                let mut economy = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                assert_eq!(economy.advance().unwrap().started, vec![1]);
                assert_eq!(economy.advance().unwrap().finished, vec![1]);
                let state = economy.snapshot();
                let memory = &state
                    .autowork
                    .as_ref()
                    .unwrap()
                    .requests
                    .as_ref()
                    .unwrap()
                    .memories;
                assert_eq!(
                    (memory[0].observer, memory[0].worker, memory[0].successes),
                    (1, 2, 1)
                );
                assert_eq!(
                    state.goals.as_ref().unwrap().people[0].progress,
                    u64::from(recognize)
                );
                state
            })
    };
    assert_eq!(run(1, false), run(4, false));
    assert_eq!(run(1, true), run(4, true));
    let mut economy = Economy::compile(initial).unwrap();
    let before = economy.snapshot();
    assert!(economy.set_goal_help_recognition(1, 999, true).is_err());
    assert_eq!(economy.snapshot(), before);
}
#[test]
fn a_persons_goal_progresses_only_after_their_actual_work_finishes() {
    use game_parallel_goals::{Goal, Goals};
    let mut state = fixture();
    state.agreements = None;
    state.opinions = None;
    state.autowork.as_mut().unwrap().policies.clear();
    state.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: false,
        people: vec![Goal {
            person: 2,
            recipe: 1,
            priority: 1,
            target: 1,
            progress: 0,
            next_review: 0,
            enabled: true,
            recognize_help: false,
        }],
        events: vec![],
    });
    let mut economy = Economy::compile(state).unwrap();
    assert_eq!(economy.advance().unwrap().started, vec![1]);
    assert_eq!(
        economy.snapshot().goals.as_ref().unwrap().people[0].progress,
        0
    );
    assert_eq!(economy.advance().unwrap().finished, vec![1]);
    assert_eq!(
        economy.snapshot().goals.as_ref().unwrap().people[0].progress,
        1
    );
}

#[test]
fn personal_reports_preserve_observation_time_and_respect_directed_trust() {
    use game_parallel_economy_knowledge::{Knowledge, Mind, Record};
    let mut initial = expedition_fixture();
    initial.people[0].controlled = true;
    initial
        .dynamics
        .as_mut()
        .unwrap()
        .social
        .as_mut()
        .unwrap()
        .participants[1]
        .willing = true;
    initial.opinions.as_mut().unwrap().records.clear();
    let place = Place::Stock { region: 1, good: 1 };
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == place)
        .unwrap();
    *stock = Container::new(
        place,
        10,
        vec![Lot {
            good: 1,
            quantity: 3,
            expires_at: Some(1),
        }],
    )
    .unwrap();
    initial.knowledge = Some(Knowledge {
        max_records: 4,
        minds: vec![
            Mind {
                person: 1,
                misreport: None,
                records: vec![],
            },
            Mind {
                person: 2,
                misreport: None,
                records: vec![Record {
                    region: 2,
                    food: 7,
                    observed_at: 0,
                    witness: 2,
                    learned_from: None,
                }],
            },
        ],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.meet(1, 2).unwrap());
                let records = world.known_food(1).unwrap();
                assert_eq!(
                    records[0],
                    Record {
                        region: 1,
                        food: 3,
                        observed_at: 0,
                        witness: 1,
                        learned_from: None
                    }
                );
                assert_eq!(
                    records[1],
                    Record {
                        region: 2,
                        food: 7,
                        observed_at: 0,
                        witness: 2,
                        learned_from: Some(2)
                    }
                );
                assert_eq!(
                    world
                        .snapshot()
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == place)
                        .unwrap()
                        .used,
                    0
                );
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let restored = Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                assert_eq!(
                    world.known_food(1).unwrap(),
                    restored.known_food(1).unwrap()
                );
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
    let opinions = initial.opinions.as_mut().unwrap();
    opinions.rules.minimum_report_trust = Some(0);
    opinions.records.push(Opinion {
        observer: 1,
        target: 2,
        trust: -1,
        fear: 0,
        obligation: 0,
        affinity: 0,
    });
    let mut world = Economy::compile(initial).unwrap();
    assert!(world.meet(1, 2).unwrap());
    assert_eq!(world.known_food(1).unwrap().len(), 1);
    assert_eq!(world.known_food(1).unwrap()[0].food, 3);
}

#[test]
fn false_food_reports_are_personal_statements_not_physical_stock() {
    use game_parallel_economy_knowledge::{Knowledge, Mind, Misreport, Record};
    let mut initial = expedition_fixture();
    initial.people[0].controlled = true;
    initial
        .dynamics
        .as_mut()
        .unwrap()
        .social
        .as_mut()
        .unwrap()
        .participants[1]
        .willing = true;
    initial.opinions.as_mut().unwrap().records.clear();
    let remembered = Record {
        region: 2,
        food: 2,
        observed_at: 0,
        witness: 2,
        learned_from: None,
    };
    initial.knowledge = Some(Knowledge {
        max_records: 4,
        minds: vec![
            Mind {
                person: 1,
                misreport: None,
                records: vec![],
            },
            Mind {
                person: 2,
                misreport: None,
                records: vec![remembered.clone()],
            },
        ],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let before = world.snapshot();
                assert!(
                    world
                        .set_food_misreport(
                            2,
                            Some(Misreport {
                                target: Some(999),
                                food_delta: 50
                            })
                        )
                        .is_err()
                );
                assert_eq!(world.snapshot(), before);
                let pending = world.prepare().unwrap();
                world
                    .set_food_misreport(
                        2,
                        Some(Misreport {
                            target: Some(1),
                            food_delta: 50,
                        }),
                    )
                    .unwrap();
                assert!(world.commit(pending).is_err());
                let physical_before = world.snapshot().physical.custody.clone();
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut world = Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                assert!(world.meet(1, 2).unwrap());
                let statement = world
                    .known_food(1)
                    .unwrap()
                    .into_iter()
                    .find(|r| r.region == 2)
                    .unwrap();
                assert_eq!(statement.food, 52);
                assert_eq!(statement.learned_from, Some(2));
                assert_eq!(
                    world
                        .known_food(2)
                        .unwrap()
                        .into_iter()
                        .find(|r| r.region == 2),
                    Some(remembered.clone())
                );
                assert_eq!(
                    world.snapshot().physical.custody.containers,
                    physical_before.containers
                );
                assert_eq!(world.food_misreport(2).unwrap().unwrap().target, Some(1));
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
    let opinions = initial.opinions.as_mut().unwrap();
    opinions.rules.minimum_report_trust = Some(0);
    opinions.records.push(Opinion {
        observer: 1,
        target: 2,
        trust: -1,
        fear: 0,
        obligation: 0,
        affinity: 0,
    });
    for (policy, accepts) in [
        (None, false),
        (Some(ReportReception::Always), true),
        (Some(ReportReception::Never), false),
        (Some(ReportReception::TrustAtLeast(-1)), true),
        (Some(ReportReception::TrustAtLeast(0)), false),
    ] {
        let mut world = Economy::compile(initial.clone()).unwrap();
        world.set_food_report_reception(1, policy).unwrap();
        world
            .set_food_misreport(
                2,
                Some(Misreport {
                    target: None,
                    food_delta: 50,
                }),
            )
            .unwrap();
        let saved = serde_json::to_string(&world.snapshot()).unwrap();
        let mut world = Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
        assert_eq!(world.food_report_reception(1).unwrap(), policy);
        assert!(world.meet(1, 2).unwrap());
        assert_eq!(
            world.known_food(1).unwrap().iter().any(|r| r.region == 2),
            accepts
        );
    }
}

#[test]
fn a_hungry_person_can_travel_on_a_false_report_then_observe_empty_stock() {
    use game_parallel_economy_knowledge::{Knowledge, Mind, Misreport, Record};
    let mut initial = expedition_fixture();
    initial.people[0].controlled = true;
    initial.people[1].hunger = 5;
    initial.people[1].seek_meals = true;
    initial.nutrition = None;
    initial
        .dynamics
        .as_mut()
        .unwrap()
        .social
        .as_mut()
        .unwrap()
        .participants[1]
        .willing = true;
    initial.knowledge = Some(Knowledge {
        max_records: 4,
        minds: vec![
            Mind {
                person: 1,
                misreport: Some(Misreport {
                    target: Some(2),
                    food_delta: 10,
                }),
                records: vec![Record {
                    region: 2,
                    food: 0,
                    observed_at: 0,
                    witness: 1,
                    learned_from: None,
                }],
            },
            Mind {
                person: 2,
                misreport: None,
                records: vec![],
            },
        ],
    });
    initial.food_search = Some(FoodSearch {
        inherit_births: false,
        min_hunger: 5,
        min_report_food: 1,
        max_report_age: 20,
        retry_ticks: 3,
        people: vec![FoodSearchPreference {
            person: 2,
            enabled: true,
            next_review: 0,
            target: None,
        }],
    });
    initial.opinions.as_mut().unwrap().rules.affinity =
        Some(game_parallel_opinions::AffinityRules {
            limit: 10,
            encounter_change: 0,
            pressured_encounter_change: 0,
            help_change: 0,
            harm_change_per_damage: 0,
        });
    initial
        .opinions
        .as_mut()
        .unwrap()
        .rules
        .minimum_report_trust = Some(0);
    let run = |workers, rate| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = initial.clone();
                state
                    .opinions
                    .as_mut()
                    .unwrap()
                    .sensitivities_mut()
                    .push(OpinionSensitivity {
                        person: 2,
                        affinity_report_discrepancy: Some(rate),
                        trust_report_discrepancy: Some(rate),
                        ..Default::default()
                    });
                let mut world = Economy::compile(state).unwrap();
                assert!(world.meet(1, 2).unwrap());
                assert_eq!(
                    world
                        .known_food(2)
                        .unwrap()
                        .iter()
                        .find(|r| r.region == 2)
                        .unwrap()
                        .food,
                    10
                );
                world.advance().unwrap();
                assert_eq!(world.snapshot().people[1].region, None);
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored = Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                for _ in 0..3 {
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                assert_eq!(world.snapshot(), restored.snapshot());
                assert_eq!(world.snapshot().people[1].region, Some(2));
                assert_eq!(
                    world
                        .known_food(2)
                        .unwrap()
                        .iter()
                        .find(|r| r.region == 2)
                        .unwrap()
                        .food,
                    0
                );
                assert_eq!(
                    world
                        .opinions()
                        .iter()
                        .find(|o| o.observer == 2 && o.target == 1)
                        .map_or(0, |o| o.affinity),
                    rate
                );
                let own = world
                    .opinions()
                    .iter()
                    .find(|o| o.observer == 2 && o.target == 1);
                assert_eq!(own.map_or(0, |o| o.trust), rate);
                let snapshot = world.snapshot();
                assert_eq!(
                    snapshot
                        .opinions
                        .as_ref()
                        .unwrap()
                        .report_filter()
                        .unwrap()
                        .accepts(2, 1),
                    rate >= 0
                );
                world.advance().unwrap();
                assert_eq!(
                    world
                        .opinions()
                        .iter()
                        .find(|o| o.observer == 2 && o.target == 1)
                        .map_or(0, |o| o.affinity),
                    rate
                );
                world.snapshot()
            })
    };
    for rate in [-2, 0, 3] {
        assert_eq!(run(1, rate), run(4, rate));
    }
}

#[test]
fn stale_draft_does_not_publish_a_report_discrepancy_reaction() {
    use game_parallel_economy_knowledge::{Knowledge, Mind, Record};
    let mut state = expedition_fixture();
    state.knowledge = Some(Knowledge {
        max_records: 4,
        minds: vec![Mind {
            person: 2,
            misreport: None,
            records: vec![Record {
                region: 1,
                food: 99,
                observed_at: 0,
                witness: 1,
                learned_from: Some(1),
            }],
        }],
    });
    state
        .opinions
        .as_mut()
        .unwrap()
        .sensitivities_mut()
        .push(OpinionSensitivity {
            person: 2,
            trust_report_discrepancy: Some(-2),
            ..Default::default()
        });
    let mut world = Economy::compile(state).unwrap();
    let stale = world.prepare().unwrap();
    world.set_social_choice(2, true).unwrap();
    assert!(world.commit(stale).is_err());
    assert_eq!(world.known_food(2).unwrap()[0].food, 99);
    assert!(
        world
            .opinions()
            .iter()
            .all(|o| (o.observer, o.target) != (2, 1))
    );
    world.advance().unwrap();
    assert_eq!(
        world
            .opinions()
            .iter()
            .find(|o| (o.observer, o.target) == (2, 1))
            .unwrap()
            .trust,
        -2
    );
    let saved = serde_json::to_string(&world.snapshot()).unwrap();
    let mut restored = Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
    restored.advance().unwrap();
    assert_eq!(
        restored
            .opinions()
            .iter()
            .find(|o| (o.observer, o.target) == (2, 1))
            .unwrap()
            .trust,
        -2
    );
}

#[test]
fn invoked_memory_remains_in_the_past_mind_and_requires_control() {
    use game_parallel_economy_campaign::{Invocation, InvocationRules};
    use game_parallel_economy_knowledge::{Knowledge, Mind, Record};
    let mut initial = campaign_fixture();
    initial.pending_food.clear();
    let record = Record {
        region: 2,
        food: 6,
        observed_at: 0,
        witness: 4,
        learned_from: None,
    };
    initial.knowledge = Some(Knowledge {
        max_records: 4,
        minds: vec![
            Mind {
                person: 3,
                misreport: None,
                records: vec![],
            },
            Mind {
                person: 4,
                misreport: None,
                records: vec![record.clone()],
            },
        ],
    });
    initial.campaign.as_mut().unwrap().lineage.invocation = Some(Invocation {
        rules: InvocationRules {
            rate_per_life: 1,
            recovery_per_tick: 1,
            strain_threshold: 100,
            loss_threshold: 200,
            strain_consent_ticks: 2,
            control_loss_ticks: 2,
            strain_damage_per_tick: 0,
            permanent_exposure_threshold: 0,
            lifespan_loss_per_tick: 0,
        },
        owner: Some(3),
        active: vec![],
        burden: 0,
        exposure: 0,
        strain_accepted_until: None,
        lifespan_accepted_until: None,
        loss_approved: false,
        loss_triggered: false,
        control_until: None,
        pending: None,
        revision: 0,
    });
    let mut world = Economy::compile(initial).unwrap();
    assert!(world.invoked_knowledge(4).is_err());
    world.invoke_lives(&[4]).unwrap();
    assert_eq!(world.invoked_knowledge(4).unwrap(), vec![record]);
    assert!(world.known_food(3).unwrap().is_empty());
    let saved = serde_json::to_string(&world.snapshot()).unwrap();
    let mut restored = Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
    assert_eq!(
        restored.invoked_knowledge(4).unwrap(),
        world.invoked_knowledge(4).unwrap()
    );
    restored.dismiss_invocation().unwrap();
    assert!(restored.invoked_knowledge(4).is_err());
}

#[test]
fn requested_hauling_follows_connected_paths_with_physical_cargo_and_reload() {
    let mut initial = expedition_fixture();
    initial.dynamics = None;
    initial.missions = None;
    initial.people[0].controlled = true;
    initial.people[0].region = Some(3);
    initial.people[1].region = Some(3);
    initial.people[1].willing_to_work = false;
    initial
        .physical
        .custody
        .containers
        .push(Container::new(Place::Stock { region: 3, good: 2 }, 10, vec![]).unwrap());
    initial
        .physical
        .custody
        .containers
        .sort_unstable_by_key(|c| c.place);
    initial.travel.as_mut().unwrap().routes = vec![
        Route {
            source: 1,
            destination: 3,
            ticks: 1,
        },
        Route {
            source: 3,
            destination: 2,
            ticks: 1,
        },
        Route {
            source: 2,
            destination: 3,
            ticks: 1,
        },
        Route {
            source: 3,
            destination: 1,
            ticks: 1,
        },
        Route {
            source: 1,
            destination: 2,
            ticks: 5,
        },
    ];
    initial.hauling = Some(Hauling {
        asking: vec![],
        requests: vec![],
        policies: vec![HaulingPolicy {
            person: 2,
            source: 1,
            destination: 2,
            good: 2,
            quantity: 2,
            willing: true,
            requires_request: true,
            request_pressure_threshold: None,
            pressured_by: None,
            pressure_threshold: None,
        }],
    });
    let run =
        |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut world = Economy::compile(initial.clone()).unwrap();
                    world.ask_haul(1, 2, 1, 2, 2, 30).unwrap();
                    let mut saw_source = false;
                    let mut saw_intermediate_cargo = false;
                    let mut saved = None;
                    for _ in 0..14 {
                        world.advance().unwrap();
                        let state = world.snapshot();
                        let held = state
                            .physical
                            .custody
                            .containers
                            .iter()
                            .find(|c| c.place == Place::Person(2))
                            .unwrap()
                            .used;
                        if state.people[1].region == Some(1) && held == 0 {
                            saw_source = true;
                        }
                        if state.travel.as_ref().unwrap().journeys.iter().any(|j| {
                            j.person == 2 && j.source == 1 && j.destination == 3 && held == 2
                        }) {
                            saw_intermediate_cargo = true;
                            saved = Some(state.clone());
                        }
                        if state
                            .physical
                            .custody
                            .containers
                            .iter()
                            .find(|c| c.place == Place::Stock { region: 2, good: 2 })
                            .unwrap()
                            .used
                            == 2
                        {
                            break;
                        }
                    }
                    assert!(saw_source && saw_intermediate_cargo);
                    let final_state = world.snapshot();
                    assert_eq!(
                        final_state
                            .physical
                            .custody
                            .containers
                            .iter()
                            .find(|c| c.place == Place::Stock { region: 2, good: 2 })
                            .unwrap()
                            .used,
                        2
                    );
                    let saved = saved.unwrap();
                    let mut restored = Economy::compile(
                        serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                    )
                    .unwrap();
                    while restored.snapshot().physical.custody.tick
                        < final_state.physical.custody.tick
                    {
                        restored.advance().unwrap();
                    }
                    assert_eq!(restored.snapshot(), final_state);
                    final_state
                })
        };
    assert_eq!(run(1), run(4));
    initial
        .travel
        .as_mut()
        .unwrap()
        .routes
        .retain(|r| !(r.source == 2 && r.destination == 3));
    assert!(Economy::compile(initial).is_err());
}

#[test]
fn food_responses_use_each_persons_signed_affinity_motives_and_preferences() {
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    initial.nutrition = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.people[2].hunger = 5;
    for container in &mut initial.physical.custody.containers {
        if container.place == Place::Person(1) {
            *container = Container::new(container.place, 10, vec![]).unwrap();
        }
        if container.place == Place::Person(2) {
            *container = Container::new(
                container.place,
                10,
                vec![Lot {
                    good: 1,
                    quantity: 1,
                    expires_at: None,
                }],
            )
            .unwrap();
        }
    }
    initial.opinions.as_mut().unwrap().records = vec![
        Opinion {
            observer: 2,
            target: 1,
            trust: 0,
            fear: 0,
            obligation: 0,
            affinity: -3,
        },
        Opinion {
            observer: 2,
            target: 3,
            trust: 0,
            fear: 0,
            obligation: 0,
            affinity: 3,
        },
    ];
    initial.giving = Some(Giving {
        practices: None,
        inherit_births: false,
        policies: vec![],
        learning: vec![],
        asking: vec![],
        acquaintance_asking: vec![],
        responses: vec![FoodResponse {
            person: 2,
            good: 1,
            quantity: 1,
            recipient_hunger_at_least: 5,
            willing: false,
            minimum_trust: None,
            affinity_threshold: None,
            affinity_at_most: Some(-1),
            affinity_weight: None,
            pressure_threshold: None,
        }],
        requests: vec![
            FoodRequest {
                requester: 1,
                target: 2,
                expires_at: 5,
            },
            FoodRequest {
                requester: 3,
                target: 2,
                expires_at: 5,
            },
        ],
    });
    let run = |workers, initial: &State| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&world.snapshot()).unwrap())
                        .unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                state
            })
    };
    let disliked = run(1, &initial);
    assert_eq!(disliked, run(4, &initial));
    assert_eq!(
        disliked
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(1))
            .unwrap()
            .used,
        1
    );
    let response = &mut initial.giving.as_mut().unwrap().responses[0];
    response.willing = true;
    response.affinity_at_most = None;
    response.affinity_weight = Some(1);
    let liked = run(4, &initial);
    assert_eq!(
        liked
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(3))
            .unwrap()
            .used,
        1
    );
    initial.giving.as_mut().unwrap().responses[0].affinity_weight = Some(-1);
    let reversed = run(4, &initial);
    assert_eq!(
        reversed
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(1))
            .unwrap()
            .used,
        1
    );
    let response = &mut initial.giving.as_mut().unwrap().responses[0];
    response.willing = false;
    response.affinity_threshold = Some(-4);
    response.affinity_weight = Some(0);
    assert!(Economy::compile(initial).is_ok());
}

#[test]
fn personal_sharing_value_grows_from_real_food_received_and_can_be_abandoned() {
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    initial.nutrition = None;
    initial.people[0].controlled = true;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.people[2].hunger = 5;
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(
        holder.place,
        holder.capacity,
        vec![
            Lot {
                good: 1,
                quantity: 2,
                expires_at: None,
            },
            Lot {
                good: 3,
                quantity: 1,
                expires_at: None,
            },
        ],
    )
    .unwrap();
    initial.giving = Some(Giving {
        practices: None,
        inherit_births: false,
        acquaintance_asking: vec![],
        policies: vec![],
        responses: vec![FoodResponse {
            person: 2,
            good: 1,
            quantity: 1,
            recipient_hunger_at_least: 5,
            willing: false,
            minimum_trust: None,
            pressure_threshold: None,
            affinity_threshold: None,
            affinity_at_most: None,
            affinity_weight: None,
        }],
        learning: vec![SharingLearning {
            person: 2,
            enabled: true,
            threshold: 2,
            capacity: 3,
            experiences: 0,
            last_helper: None,
            reciprocate_hunger: None,
            ask_helper: None,
        }],
        asking: vec![],
        requests: vec![FoodRequest {
            requester: 3,
            target: 2,
            expires_at: 20,
        }],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.give(1, 2, 3, 1).unwrap());
                assert_eq!(
                    world.snapshot().giving.as_ref().unwrap().learning[0].experiences,
                    0
                );
                assert!(world.give(1, 2, 1, 1).unwrap());
                assert_eq!(
                    world.snapshot().giving.as_ref().unwrap().learning[0].experiences,
                    1
                );
                assert!(world.give(1, 2, 1, 1).unwrap());
                let saved = world.snapshot();
                assert_eq!(saved.giving.as_ref().unwrap().learning[0].experiences, 2);
                assert_eq!(world.food_requests().len(), 1);
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                let mut refusing = Economy::compile(saved).unwrap();
                let draft = refusing.prepare().unwrap();
                refusing.set_giving_choice(2, false).unwrap();
                assert!(refusing.commit(draft).is_err());
                refusing.advance().unwrap();
                assert_eq!(refusing.food_requests().len(), 1);
                refusing.set_sharing_learning_choice(2, true).unwrap();
                refusing.advance().unwrap();
                assert!(refusing.food_requests().is_empty());
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert!(world.food_requests().is_empty());
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(3))
                        .unwrap()
                        .used,
                    1
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

fn sharing_learning_workload(count: u64) -> State {
    let mut state = giving_workload(count);
    let giving = state.giving.as_mut().unwrap();
    giving.responses.push(FoodResponse {
        person: 1,
        good: 1,
        quantity: 1,
        recipient_hunger_at_least: 1,
        willing: false,
        minimum_trust: None,
        pressure_threshold: None,
        affinity_threshold: None,
        affinity_at_most: None,
        affinity_weight: None,
    });
    giving.learning.push(SharingLearning {
        person: 1,
        enabled: true,
        threshold: 2,
        capacity: u32::try_from(count).unwrap(),
        experiences: 0,
        last_helper: None,
        reciprocate_hunger: None,
        ask_helper: None,
    });
    state
}

#[test]
fn crowded_sharing_learning_counts_only_actual_receipts_across_workers() {
    let count = 4096;
    let initial = sharing_learning_workload(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state.giving.as_ref().unwrap().learning[0].experiences,
                    count as u32 / 2
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_sharing_learning_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = sharing_learning_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn people_can_invent_a_reciprocal_offer_from_their_own_experience() {
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    initial.nutrition = None;
    initial.people[0].controlled = true;
    for p in &mut initial.people {
        p.seek_meals = false;
    }
    initial.giving = Some(Giving {
        practices: None,
        inherit_births: false,
        acquaintance_asking: vec![],
        policies: vec![],
        responses: vec![FoodResponse {
            person: 2,
            good: 1,
            quantity: 1,
            recipient_hunger_at_least: 5,
            willing: false,
            minimum_trust: None,
            pressure_threshold: None,
            affinity_threshold: None,
            affinity_at_most: None,
            affinity_weight: None,
        }],
        learning: vec![SharingLearning {
            person: 2,
            enabled: true,
            threshold: 1,
            capacity: 5,
            experiences: 0,
            last_helper: None,
            reciprocate_hunger: Some(5),
            ask_helper: None,
        }],
        asking: vec![],
        requests: vec![],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.give(1, 2, 1, 1).unwrap());
                let saved = world.snapshot();
                assert_eq!(
                    saved.giving.as_ref().unwrap().learning[0].last_helper,
                    Some(1)
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                let mut refusing = Economy::compile(saved).unwrap();
                refusing.set_giving_choice(2, false).unwrap();
                refusing.advance().unwrap();
                assert_eq!(
                    refusing
                        .snapshot()
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(1))
                        .unwrap()
                        .used,
                    0
                );
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(1))
                        .unwrap()
                        .used,
                    1
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
    let mut remote = initial;
    remote.people[0].region = Some(2);
    remote
        .physical
        .custody
        .containers
        .push(Container::new(Place::Stock { region: 2, good: 1 }, 10, vec![]).unwrap());
    remote
        .physical
        .custody
        .containers
        .sort_unstable_by_key(|c| c.place);
    let held = remote
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(2))
        .unwrap();
    *held = Container::new(
        held.place,
        held.capacity,
        vec![Lot {
            good: 1,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let learning = &mut remote.giving.as_mut().unwrap().learning[0];
    learning.experiences = 1;
    learning.last_helper = Some(1);
    remote.travel = Some(Travel {
        routes: vec![Route {
            source: 1,
            destination: 2,
            ticks: 1,
        }],
        journeys: vec![],
    });
    let mut world = Economy::compile(remote).unwrap();
    world.advance().unwrap();
    assert_eq!(
        world
            .snapshot()
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(2))
            .unwrap()
            .used,
        1
    );
}

fn reciprocal_sharing_workload(count: u64) -> State {
    let mut state = giving_workload(count);
    let giving = state.giving.as_mut().unwrap();
    giving.policies.clear();
    giving.responses = (2..=count)
        .map(|person| FoodResponse {
            person,
            good: 1,
            quantity: 1,
            recipient_hunger_at_least: 5,
            willing: false,
            minimum_trust: None,
            pressure_threshold: None,
            affinity_threshold: None,
            affinity_at_most: None,
            affinity_weight: None,
        })
        .collect();
    giving.learning = (2..=count)
        .map(|person| SharingLearning {
            person,
            enabled: true,
            threshold: 1,
            capacity: 2,
            experiences: 1,
            last_helper: Some(1),
            reciprocate_hunger: Some(5),
            ask_helper: None,
        })
        .collect();
    state
}

#[test]
fn many_reciprocal_choices_share_physical_capacity_across_workers() {
    let count = 4096;
    let initial = reciprocal_sharing_workload(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let events = world.advance().unwrap();
                assert_eq!(
                    events
                        .physical_actions
                        .iter()
                        .filter(|e| e.accepted)
                        .count(),
                    count as usize / 2
                );
                let state = world.snapshot();
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(1))
                        .unwrap()
                        .used,
                    count / 2
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_reciprocal_sharing_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = reciprocal_sharing_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn hungry_person_constructs_a_request_to_someone_who_actually_helped_them() {
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    initial.nutrition = None;
    initial.people[0].controlled = true;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.people[1].seek_meals = true;
    initial.people[1].hunger = 5;
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(
        holder.place,
        holder.capacity,
        vec![Lot {
            good: 1,
            quantity: 2,
            expires_at: None,
        }],
    )
    .unwrap();
    initial.giving = Some(Giving {
        practices: None,
        inherit_births: false,
        acquaintance_asking: vec![],
        policies: vec![],
        responses: vec![FoodResponse {
            person: 2,
            good: 1,
            quantity: 1,
            recipient_hunger_at_least: 1,
            willing: false,
            minimum_trust: None,
            pressure_threshold: None,
            affinity_threshold: None,
            affinity_at_most: None,
            affinity_weight: None,
        }],
        learning: vec![SharingLearning {
            person: 2,
            enabled: true,
            threshold: 1,
            capacity: 5,
            experiences: 0,
            last_helper: None,
            reciprocate_hunger: None,
            ask_helper: Some(HelperAsking {
                min_hunger: 1,
                retry_ticks: 5,
                request_lifetime: 4,
                next_review: 0,
            }),
        }],
        asking: vec![],
        requests: vec![],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.give(1, 2, 1, 1).unwrap());
                assert!(world.food_requests().is_empty());
                world.advance().unwrap();
                assert!(world.food_requests().is_empty());
                world.advance().unwrap();
                assert_eq!(world.food_requests().len(), 1);
                assert_eq!(
                    (
                        world.food_requests()[0].requester,
                        world.food_requests()[0].target
                    ),
                    (2, 1)
                );
                let saved = world.snapshot();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.food_requests().len(), 1);
                assert!(world.give(1, 2, 1, 1).unwrap());
                assert!(restored.give(1, 2, 1, 1).unwrap());
                assert!(world.food_requests().is_empty());
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

fn learned_food_asking_workload(count: u64) -> State {
    let mut state = food_asking_workload(count);
    let giving = state.giving.as_mut().unwrap();
    giving.asking.clear();
    giving.responses = (2..=count)
        .map(|person| FoodResponse {
            person,
            good: 1,
            quantity: 1,
            recipient_hunger_at_least: 5,
            willing: false,
            minimum_trust: None,
            pressure_threshold: None,
            affinity_threshold: None,
            affinity_at_most: None,
            affinity_weight: None,
        })
        .collect();
    giving.learning = (2..=count)
        .map(|person| SharingLearning {
            person,
            enabled: true,
            threshold: 1,
            capacity: 2,
            experiences: 1,
            last_helper: Some(1),
            reciprocate_hunger: None,
            ask_helper: Some(HelperAsking {
                min_hunger: 5,
                retry_ticks: 8,
                request_lifetime: 4,
                next_review: 0,
            }),
        })
        .collect();
    state
}

#[test]
fn many_people_form_independent_requests_from_personal_experience() {
    let count = 4096;
    let initial = learned_food_asking_workload(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                assert_eq!(world.food_requests().len(), count as usize - 1);
                assert!(world.food_requests().iter().all(|r| r.target == 1));
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_learned_food_asking_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = learned_food_asking_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn directed_affinity_motivates_and_prioritizes_personal_help() {
    let mut initial = food_response_workload(4);
    let response = &mut initial.giving.as_mut().unwrap().responses[0];
    response.willing = false;
    response.affinity_threshold = Some(3);
    let mut opinions = fixture().opinions.unwrap();
    opinions.records = vec![
        Opinion {
            observer: 1,
            target: 2,
            trust: 0,
            fear: 0,
            obligation: 0,
            affinity: 1,
        },
        Opinion {
            observer: 1,
            target: 3,
            trust: 0,
            fear: 0,
            obligation: 0,
            affinity: 5,
        },
        Opinion {
            observer: 1,
            target: 4,
            trust: 0,
            fear: 0,
            obligation: 0,
            affinity: -4,
        },
        Opinion {
            observer: 2,
            target: 1,
            trust: 0,
            fear: 0,
            obligation: 0,
            affinity: 9,
        },
    ];
    opinions.rules.affinity = Some(game_parallel_opinions::AffinityRules {
        limit: 10,
        encounter_change: 1,
        help_change: 2,
        harm_change_per_damage: -1,

        pressured_encounter_change: 0,
    });
    initial.opinions = Some(opinions);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let saved = world.snapshot();
                let recipient = saved
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|c| c.place == Place::Person(3))
                    .unwrap();
                assert_eq!(recipient.used, 1);
                assert!(world.food_requests().iter().all(|r| r.requester != 3));
                assert_eq!(saved.opinions.as_ref().unwrap().index().affinity(3, 1), 2);
                assert_eq!(saved.opinions.as_ref().unwrap().index().affinity(1, 3), 5);
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
    let mut refusing = Economy::compile(initial).unwrap();
    refusing.set_giving_choice(1, false).unwrap();
    refusing.advance().unwrap();
    assert_eq!(refusing.food_requests().len(), 3);
}

#[test]
fn an_actual_nonfood_gift_can_affect_affinity_without_creating_food_obligation() {
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    initial.people[0].controlled = true;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(
        holder.place,
        holder.capacity,
        vec![Lot {
            good: 3,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let opinions = initial.opinions.as_mut().unwrap();
    opinions.records.clear();
    opinions.rules.gratitude_gain = 1;
    opinions.sensitivities = vec![OpinionSensitivity {
        person: 2,
        report_reception: None,
        affinity_report_discrepancy: None,
        trust_report_discrepancy: None,
        trust_promise_fulfilled: None,
        trust_promise_unfulfilled: None,
        fear_per_damage: None,
        gratitude_gain: Some(0),
        obligation_relief: None,
        theft_loss: None,
        confrontation: None,
        affinity_help: None,
        affinity_harm_per_damage: None,
        affinity_encounter: None,
        affinity_pressured_encounter: None,
        affinity_site_use: None,
        affinity_site_destruction: None,
        affinity_site_salvage: None,
        affinity_work_success: None,
        affinity_work_refusal: None,
        relationship_cases: vec![],
        obligation_development: None,
        promise_obligation_gain: None,
    }]
    .into();
    opinions.rules.affinity = Some(game_parallel_opinions::AffinityRules {
        limit: 10,
        encounter_change: 0,
        help_change: 2,
        harm_change_per_damage: 0,

        pressured_encounter_change: 0,
    });
    let mut world = Economy::compile(initial).unwrap();
    assert!(world.give(1, 2, 3, 1).unwrap());
    let saved = world.snapshot();
    let opinions = saved.opinions.as_ref().unwrap();
    assert_eq!(opinions.index().affinity(2, 1), 2);
    assert_eq!(opinions.index().obligation(2, 1), 0);
    assert_eq!(opinions.index().affinity(1, 2), 0);
}

#[test]
fn friends_seek_each_other_only_while_both_choose_to_meet() {
    use game_parallel_social::Participant;
    let mut initial = expedition_fixture();
    initial.missions = None;
    initial.opinions.as_mut().unwrap().rules.affinity =
        Some(game_parallel_opinions::AffinityRules {
            limit: 10,
            encounter_change: 1,
            help_change: 1,
            harm_change_per_damage: -1,

            pressured_encounter_change: 0,
        });
    initial.opinions.as_mut().unwrap().records = vec![
        Opinion {
            observer: 1,
            target: 2,
            trust: 0,
            fear: 0,
            obligation: 0,
            affinity: 5,
        },
        Opinion {
            observer: 2,
            target: 1,
            trust: 0,
            fear: 0,
            obligation: 0,
            affinity: 3,
        },
    ];
    let social = initial.dynamics.as_mut().unwrap().social.as_mut().unwrap();
    social.participants = (1..=4)
        .map(|person| Participant {
            person,
            willing: true,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let saved = world.snapshot();
                let events = &saved
                    .dynamics
                    .as_ref()
                    .unwrap()
                    .social
                    .as_ref()
                    .unwrap()
                    .events;
                assert!(
                    events
                        .iter()
                        .any(|e| (e.a, e.b, e.pressured) == (1, 2, None))
                );
                let opinions = saved.opinions.as_ref().unwrap().index();
                assert_eq!((opinions.affinity(1, 2), opinions.affinity(2, 1)), (6, 4));
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
    let mut refusing = Economy::compile(initial).unwrap();
    refusing.set_social_choice(2, false).unwrap();
    refusing.advance().unwrap();
    assert!(
        refusing
            .snapshot()
            .dynamics
            .as_ref()
            .unwrap()
            .social
            .as_ref()
            .unwrap()
            .events
            .iter()
            .all(|e| e.a != 2 && e.b != 2)
    );
}

#[test]
fn a_person_may_feel_no_obligation_and_still_cooperate_out_of_fear() {
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    initial.nutrition = None;
    initial.people[0].controlled = true;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.people[1].hunger = 5;
    initial.people[2].hunger = 5;
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(
        holder.place,
        holder.capacity,
        vec![Lot {
            good: 1,
            quantity: 2,
            expires_at: None,
        }],
    )
    .unwrap();
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(4))
        .unwrap();
    *holder = Container::new(holder.place, holder.capacity, vec![]).unwrap();
    let opinions = initial.opinions.as_mut().unwrap();
    opinions.rules.gratitude_gain = 3;
    opinions.sensitivities = vec![game_parallel_opinions::Sensitivity {
        person: 2,
        report_reception: None,
        affinity_report_discrepancy: None,
        trust_report_discrepancy: None,
        trust_promise_fulfilled: None,
        trust_promise_unfulfilled: None,
        fear_per_damage: None,
        gratitude_gain: Some(0),
        obligation_relief: None,
        theft_loss: None,
        confrontation: None,
        affinity_help: None,
        affinity_harm_per_damage: None,
        affinity_encounter: None,
        affinity_pressured_encounter: None,
        affinity_site_use: None,
        affinity_site_destruction: None,
        affinity_site_salvage: None,
        affinity_work_success: None,
        affinity_work_refusal: None,
        relationship_cases: vec![],
        obligation_development: None,
        promise_obligation_gain: None,
    }]
    .into();
    opinions.records = vec![Opinion {
        observer: 2,
        target: 1,
        trust: 0,
        fear: 5,
        obligation: 0,
        affinity: 0,
    }];
    initial.giving = Some(Giving {
        practices: None,
        inherit_births: false,
        acquaintance_asking: vec![],
        learning: vec![],
        responses: vec![],
        asking: vec![],
        requests: vec![],
        policies: vec![GivingPolicy {
            person: 2,
            recipient: 4,
            good: 1,
            quantity: 1,
            recipient_hunger_at_least: 0,
            willing: false,
            requires_request: false,
            pressured_by: Some(1),
            pressure_threshold: Some(3),
        }],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.give(1, 2, 1, 1).unwrap());
                assert!(world.give(1, 3, 1, 1).unwrap());
                let state = world.snapshot();
                let opinions = state.opinions.as_ref().unwrap().index();
                assert_eq!(opinions.obligation(2, 1), 0);
                assert_eq!(opinions.obligation(3, 1), 3);
                assert_eq!(opinions.fear(2, 1), 5);
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(4))
                        .unwrap()
                        .used,
                    1
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

fn affinity_social_workload(count: u64) -> State {
    let mut state = traveling_social_workload(count);
    let mut opinions = fixture().opinions.unwrap();
    opinions.rules.affinity = Some(game_parallel_opinions::AffinityRules {
        limit: 10,
        encounter_change: 1,
        pressured_encounter_change: 0,
        help_change: 0,
        harm_change_per_damage: 0,
    });
    opinions.records = (1..=count)
        .step_by(2)
        .filter(|id| *id < count)
        .map(|id| Opinion {
            observer: id,
            target: id + 1,
            trust: 0,
            fear: 0,
            obligation: 0,
            affinity: 5,
        })
        .collect();
    state.opinions = Some(opinions);
    state
}

#[test]
fn affinity_based_meetings_remain_disjoint_and_deterministic_at_scale() {
    let count = 4096;
    let initial = affinity_social_workload(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let events = &state
                    .dynamics
                    .as_ref()
                    .unwrap()
                    .social
                    .as_ref()
                    .unwrap()
                    .events;
                assert_eq!(events.len(), count as usize / 2);
                assert!(events.iter().all(|e| e.a % 2 == 1 && e.b == e.a + 1));
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn affinity_social_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = affinity_social_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn personal_interpretations_can_change_atomically_without_creating_obligations() {
    let initial = fixture();
    let mut world = Economy::compile(initial).unwrap();
    let draft = world.prepare().unwrap();
    let mut sensitivity = OpinionSensitivity {
        person: 2,
        report_reception: None,
        affinity_report_discrepancy: None,
        trust_report_discrepancy: None,
        trust_promise_fulfilled: None,
        trust_promise_unfulfilled: None,
        fear_per_damage: None,
        gratitude_gain: Some(0),
        obligation_relief: None,
        theft_loss: None,
        confrontation: None,
        affinity_help: Some(-1),
        affinity_harm_per_damage: Some(2),
        affinity_encounter: None,
        affinity_pressured_encounter: None,
        affinity_site_use: None,
        affinity_site_destruction: None,
        affinity_site_salvage: None,
        affinity_work_success: None,
        affinity_work_refusal: None,
        relationship_cases: vec![],
        obligation_development: None,
        promise_obligation_gain: None,
    };
    let before = world.snapshot();
    sensitivity.relationship_cases.push(RelationshipCase {
        target: 999,
        report_discrepancy: None,
        report_discrepancy_trust: None,
        promise_fulfilled_trust: None,
        promise_unfulfilled_trust: None,
        help: None,
        harm_per_damage: Some(-1),
        encounter: None,
        pressured_encounter: None,
        site_use: None,
        site_destruction: None,
        site_salvage: None,
        work_success: None,
        work_refusal: None,
        obligation_gain: None,
        obligation_relief: None,
        promise_obligation_gain: None,
    });
    assert!(world.set_opinion_sensitivity(sensitivity.clone()).is_err());
    assert_eq!(world.snapshot(), before);
    sensitivity.relationship_cases[0].target = 1;
    world.set_opinion_sensitivity(sensitivity.clone()).unwrap();
    assert!(world.commit(draft).is_err());
    let saved = world.snapshot();
    assert_eq!(
        saved.opinions.as_ref().unwrap().sensitivities,
        std::sync::Arc::new(vec![sensitivity])
    );
    let restored =
        Economy::compile(serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap())
            .unwrap();
    assert_eq!(restored.snapshot(), saved);
}

#[test]
fn actual_harm_can_increase_one_persons_affinity_and_reduce_anothers() {
    let mut initial = fixture();
    initial.autowork = None;
    initial.agreements = None;
    initial.people[0].controlled = true;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.combat = Some(combat_rules());
    let opinions = initial.opinions.as_mut().unwrap();
    opinions.records.clear();
    opinions.rules.affinity = Some(game_parallel_opinions::AffinityRules {
        limit: 30,
        encounter_change: 0,
        pressured_encounter_change: 0,
        help_change: 0,
        harm_change_per_damage: -1,
    });
    opinions.sensitivities = vec![OpinionSensitivity {
        person: 2,
        report_reception: None,
        affinity_report_discrepancy: None,
        trust_report_discrepancy: None,
        trust_promise_fulfilled: None,
        trust_promise_unfulfilled: None,
        fear_per_damage: None,
        gratitude_gain: Some(0),
        obligation_relief: None,
        theft_loss: None,
        confrontation: None,
        affinity_help: None,
        affinity_harm_per_damage: Some(1),
        affinity_encounter: None,
        affinity_pressured_encounter: None,
        affinity_site_use: None,
        affinity_site_destruction: None,
        affinity_site_salvage: None,
        affinity_work_success: None,
        affinity_work_refusal: None,
        relationship_cases: vec![],
        obligation_development: None,
        promise_obligation_gain: None,
    }]
    .into();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.attack(1, 2).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!((state.people[0].health, state.people[1].health), (90, 90));
                let opinions = state.opinions.as_ref().unwrap().index();
                assert_eq!(
                    (opinions.affinity(1, 2), opinions.affinity(2, 1)),
                    (-10, 10)
                );
                assert_eq!(opinions.obligation(2, 1), 0);
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn children_can_develop_different_obligation_dispositions_from_the_same_care() {
    use game_parallel_dependent_care::{Policy as CarePolicy, Rules as CareRules};
    let mut initial = campaign_fixture();
    initial.campaign = None;
    initial.pending_food.clear();
    initial.nutrition = None;
    initial.people[3].health = 100;
    for person in &mut initial.people {
        person.controlled = false;
        person.seek_meals = false;
    }
    let family = initial.family.as_mut().unwrap();
    for member in &mut family.members {
        member.pregnancy = None;
        member.reproduction_allowed = false;
    }
    family.members[2].age = 1;
    family.members[2].parents = vec![1, 2].into();
    family.members[3].alive = true;
    family.members[3].age = 1;
    family.members[3].parents = vec![1, 2].into();
    initial.care = Some(DependentCare {
        inherit_births: false,
        rules: CareRules {
            dependent_until: 5,
            max_caregiver_hunger: 100,
            neglect_damage: 0,
            feed_from_cargo: false,
            fetch_local_food: false,
            pressure_threshold: None,
        },
        policies: vec![
            CarePolicy {
                person: 1,
                willing: true,
            },
            CarePolicy {
                person: 2,
                willing: true,
            },
        ],
        events: vec![],
    });
    let sensitivity = |person, cared_change, unattended_change| OpinionSensitivity {
        person,
        report_reception: None,
        affinity_report_discrepancy: None,
        trust_report_discrepancy: None,
        trust_promise_fulfilled: None,
        trust_promise_unfulfilled: None,
        fear_per_damage: None,
        gratitude_gain: Some(2),
        obligation_relief: None,
        theft_loss: None,
        confrontation: None,
        affinity_help: None,
        affinity_harm_per_damage: None,
        affinity_encounter: None,
        affinity_pressured_encounter: None,
        affinity_site_use: None,
        affinity_site_destruction: None,
        affinity_site_salvage: None,
        affinity_work_success: None,
        affinity_work_refusal: None,
        relationship_cases: vec![],
        obligation_development: Some(ObligationDevelopment {
            until_age: 3,
            minimum: 0,
            maximum: 5,
            cared_change,
            unattended_change,
            cared_promise_change: None,
            unattended_promise_change: None,
        }),
        promise_obligation_gain: None,
    };
    initial.opinions.as_mut().unwrap().sensitivities =
        vec![sensitivity(3, -1, 2), sensitivity(4, 1, -2)].into();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let saved = world.snapshot();
                assert!(
                    saved
                        .care
                        .as_ref()
                        .unwrap()
                        .events
                        .iter()
                        .all(|e| e.caregiver.is_some())
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                for _ in 0..2 {
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                let sensitivities = &state.opinions.as_ref().unwrap().sensitivities;
                assert_eq!(
                    (
                        sensitivities[0].gratitude_gain,
                        sensitivities[1].gratitude_gain
                    ),
                    (Some(0), Some(4))
                );
                assert_eq!(state.opinions.as_ref().unwrap().index().obligation(3, 1), 0);
                state
            })
    };
    assert_eq!(run(1), run(4));
    for policy in &mut initial.care.as_mut().unwrap().policies {
        policy.willing = false;
    }
    let mut unattended = Economy::compile(initial).unwrap();
    unattended.advance().unwrap();
    let state = unattended.snapshot();
    assert!(
        state
            .care
            .as_ref()
            .unwrap()
            .events
            .iter()
            .all(|e| e.caregiver.is_none())
    );
    let sensitivities = &state.opinions.as_ref().unwrap().sensitivities;
    assert_eq!(
        (
            sensitivities[0].gratitude_gain,
            sensitivities[1].gratitude_gain
        ),
        (Some(4), Some(0))
    );
}

fn concentrated_upbringing_workload(count: u64) -> State {
    use game_parallel_dependent_care::{Policy as CarePolicy, Rules as CareRules};
    use game_parallel_families::{Families, FamilyRules, FamilyTotals, Genome, Member, Sex};
    let mut state = field_workload(count);
    state.agriculture = None;
    state.autowork = None;
    for person in &mut state.people {
        person.seek_meals = false;
        person.hunger = 0;
    }
    state.family = Some(Families {
        rules: FamilyRules {
            adult_age: 10,
            min_work_age: 10,
            fertile_until: 50,
            gestation: 3,
            birth_spacing: 5,
            lifespan_min: 60,
            lifespan_max: 100,
            conception_one_in: 100,
            minimum_health: 1,
            maximum_hunger: 100,
            newborn_health: 100,
            mutation_per_million: 0,
            mutation_step: 0,
            enroll_births: false,
        },
        members: (1..=count)
            .map(|id| Member {
                id,
                born: None,
                age: if id <= 2 { 20 } else { 1 },
                lifespan_loss: 0,
                genome: Genome {
                    sex: if id == 1 { Sex::Female } else { Sex::Male },
                    alleles: [[500; 2]; 4],
                },
                parents: if id <= 2 {
                    Default::default()
                } else {
                    vec![1, 2].into()
                },
                partner: None,
                pregnancy: None,
                next_conception: 0,
                alive: true,
                reproduction_allowed: false,
                reconsider_partnership: false,
            })
            .collect(),
        next_id: count + 1,
        last_tick: 0,
        totals: FamilyTotals::default(),
        events: vec![],
    });
    state.care = Some(DependentCare {
        rules: CareRules {
            dependent_until: 5,
            max_caregiver_hunger: 100,
            neglect_damage: 0,
            feed_from_cargo: false,
            fetch_local_food: false,
            pressure_threshold: None,
        },
        inherit_births: false,
        policies: vec![
            CarePolicy {
                person: 1,
                willing: true,
            },
            CarePolicy {
                person: 2,
                willing: true,
            },
        ],
        events: vec![],
    });
    let mut opinions = fixture().opinions.unwrap();
    opinions.records.clear();
    opinions.sensitivities = (3..=count)
        .map(|person| OpinionSensitivity {
            person,
            report_reception: None,
            affinity_report_discrepancy: None,
            trust_report_discrepancy: None,
            trust_promise_fulfilled: None,
            trust_promise_unfulfilled: None,
            fear_per_damage: None,
            gratitude_gain: Some(2),
            obligation_relief: None,
            theft_loss: None,
            confrontation: None,
            affinity_help: None,
            affinity_harm_per_damage: None,
            affinity_encounter: None,
            affinity_pressured_encounter: None,
            affinity_site_use: None,
            affinity_site_destruction: None,
            affinity_site_salvage: None,
            affinity_work_success: None,
            affinity_work_refusal: None,
            relationship_cases: vec![],
            obligation_development: Some(ObligationDevelopment {
                until_age: 4,
                minimum: 0,
                maximum: 5,
                cared_change: 1,
                unattended_change: -1,
                cared_promise_change: None,
                unattended_promise_change: None,
            }),
            promise_obligation_gain: None,
        })
        .collect::<Vec<_>>()
        .into();
    state.opinions = Some(opinions);
    state
}

#[test]
fn crowded_upbringing_uses_actual_individual_care_across_workers() {
    let count = 4096;
    let initial = concentrated_upbringing_workload(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let care = &state.care.as_ref().unwrap().events;
                assert_eq!(care.len(), count as usize - 2);
                assert_eq!(care.iter().filter(|e| e.caregiver.is_some()).count(), 2);
                let sensitivities = &state.opinions.as_ref().unwrap().sensitivities;
                assert_eq!(
                    sensitivities
                        .iter()
                        .filter(|p| p.gratitude_gain == Some(3))
                        .count(),
                    2
                );
                assert_eq!(
                    sensitivities
                        .iter()
                        .filter(|p| p.gratitude_gain == Some(1))
                        .count(),
                    count as usize - 4
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_upbringing_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = concentrated_upbringing_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
            #[cfg(feature = "profile")]
            for (name, measurement) in game_diagnostics::snapshot() {
                println!(
                    "{name}: {:.3} ms/call",
                    measurement.nanoseconds as f64 / measurement.calls as f64 / 1_000_000.0
                );
            }
        });
}

#[test]
#[ignore]
fn concentrated_upbringing_without_learning_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let mut initial = concentrated_upbringing_workload(count);
    initial
        .opinions
        .as_mut()
        .unwrap()
        .sensitivities_mut()
        .clear();
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn felt_obligation_can_motivate_a_promise_without_binding_its_maker() {
    let mut initial = fixture();
    initial.autowork = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    let policy = &mut initial.agreements.as_mut().unwrap().policies[0];
    policy.honor = false;
    policy.fear_honor_threshold = None;
    policy.obligation_honor_threshold = Some(3);
    initial.opinions.as_mut().unwrap().records = vec![Opinion {
        observer: 4,
        target: 1,
        trust: 0,
        fear: 0,
        obligation: 3,
        affinity: 0,
    }];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let saved = world.snapshot();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(
                    state.agreements.as_ref().unwrap().contracts[0].status,
                    Status::Fulfilled
                );
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(1))
                        .unwrap()
                        .used,
                    2
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
    let mut refusing = Economy::compile(initial.clone()).unwrap();
    let draft = refusing.prepare().unwrap();
    refusing.set_promise_obligation_choice(4, None).unwrap();
    assert!(refusing.commit(draft).is_err());
    refusing.advance().unwrap();
    assert_ne!(
        refusing.snapshot().agreements.as_ref().unwrap().contracts[0].status,
        Status::Fulfilled
    );
    initial.opinions.as_mut().unwrap().records[0].obligation = 0;
    let mut without_obligation = Economy::compile(initial).unwrap();
    without_obligation.advance().unwrap();
    assert_ne!(
        without_obligation
            .snapshot()
            .agreements
            .as_ref()
            .unwrap()
            .contracts[0]
            .status,
        Status::Fulfilled
    );
}

#[test]
fn only_a_person_who_interprets_their_promise_as_an_obligation_feels_one() {
    let mut initial = fixture();
    initial.autowork = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    let agreements = initial.agreements.as_mut().unwrap();
    agreements.contracts.clear();
    agreements.events.clear();
    agreements.next_id = 1;
    agreements.policies[0].fear_honor_threshold = None;
    agreements.policies[0].obligation_honor_threshold = Some(3);
    let opinions = initial.opinions.as_mut().unwrap();
    opinions.records.clear();
    let sensitivity = |person, gain| OpinionSensitivity {
        person,
        report_reception: None,
        affinity_report_discrepancy: None,
        trust_report_discrepancy: None,
        trust_promise_fulfilled: None,
        trust_promise_unfulfilled: None,
        fear_per_damage: None,
        gratitude_gain: Some(0),
        promise_obligation_gain: Some(gain),
        obligation_relief: None,
        theft_loss: None,
        confrontation: None,
        affinity_help: None,
        affinity_harm_per_damage: None,
        affinity_encounter: None,
        affinity_pressured_encounter: None,
        affinity_site_use: None,
        affinity_site_destruction: None,
        affinity_site_salvage: None,
        affinity_work_success: None,
        affinity_work_refusal: None,
        relationship_cases: vec![],
        obligation_development: None,
    };
    opinions.sensitivities = vec![sensitivity(2, 0), sensitivity(4, 3)].into();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let before = world.snapshot();
                assert!(
                    world
                        .propose_promises(&[PromiseProposal {
                            debtor: 4,
                            creditor: 1,
                            good: 999,
                            quantity: 1,
                            due: 10
                        }])
                        .is_err()
                );
                assert_eq!(world.snapshot(), before);
                let ids = world
                    .propose_promises(&[
                        PromiseProposal {
                            debtor: 2,
                            creditor: 1,
                            good: 1,
                            quantity: 1,
                            due: 10,
                        },
                        PromiseProposal {
                            debtor: 4,
                            creditor: 1,
                            good: 1,
                            quantity: 1,
                            due: 10,
                        },
                    ])
                    .unwrap();
                let state = world.snapshot();
                let index = state.opinions.as_ref().unwrap().index();
                assert_eq!((index.obligation(2, 1), index.obligation(4, 1)), (0, 3));
                world
                    .decide_promises(&[PromiseDecision {
                        contract: ids[1],
                        actor: 1,
                        accept: true,
                    }])
                    .unwrap();
                let saved = world.snapshot();
                let mut refusing = Economy::compile(saved.clone()).unwrap();
                refusing.set_promise_obligation_choice(4, None).unwrap();
                refusing.advance().unwrap();
                assert_ne!(
                    refusing.snapshot().agreements.as_ref().unwrap().contracts[1].status,
                    Status::Fulfilled
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                assert_eq!(
                    world.snapshot().agreements.as_ref().unwrap().contracts[1].status,
                    Status::Fulfilled
                );
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn autonomous_promise_offers_can_be_refused_accepted_and_later_broken() {
    let mut initial = fixture();
    initial.autowork = None;
    initial.people[0].controlled = true;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(2))
        .unwrap();
    *holder = Container::new(
        holder.place,
        holder.capacity,
        vec![Lot {
            good: 1,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let agreements = initial.agreements.as_mut().unwrap();
    agreements.contracts.clear();
    agreements.events.clear();
    agreements.next_id = 1;
    agreements.policies = vec![AgreementPolicy {
        person: 2,
        honor: false,
        accept: false,
        minimum_trust: None,
        minimum_honor_trust: None,
        fear_honor_threshold: None,
        obligation_honor_threshold: Some(3),
        accept_pressure_threshold: None,
    }];
    agreements.offering = vec![PromiseOfferPolicy {
        person: 2,
        recipient: 1,
        good: 1,
        quantity: 1,
        due_after: 8,
        retry_ticks: 3,
        willing: true,
        recipient_hunger_at_least: 0,
        minimum_trust: None,
        pressure_threshold: None,
        next_review: 0,
        after_work_recipe: None,
        last_work_offer_at: None,
    }];
    let opinions = initial.opinions.as_mut().unwrap();
    opinions.records.clear();
    opinions.sensitivities = vec![OpinionSensitivity {
        person: 2,
        report_reception: None,
        affinity_report_discrepancy: None,
        trust_report_discrepancy: None,
        trust_promise_fulfilled: None,
        trust_promise_unfulfilled: None,
        fear_per_damage: None,
        gratitude_gain: Some(0),
        promise_obligation_gain: Some(3),
        obligation_relief: None,
        theft_loss: None,
        confrontation: None,
        affinity_help: None,
        affinity_harm_per_damage: None,
        affinity_encounter: None,
        affinity_pressured_encounter: None,
        affinity_site_use: None,
        affinity_site_destruction: None,
        affinity_site_salvage: None,
        affinity_work_success: None,
        affinity_work_refusal: None,
        relationship_cases: vec![],
        obligation_development: None,
    }]
    .into();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let proposed = world.snapshot();
                assert_eq!(proposed.agreements.as_ref().unwrap().contracts.len(), 1);
                assert_eq!(
                    proposed.agreements.as_ref().unwrap().contracts[0].status,
                    Status::Proposed
                );
                assert_eq!(
                    proposed.opinions.as_ref().unwrap().index().obligation(2, 1),
                    3
                );
                assert_eq!(
                    proposed
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap()
                        .used,
                    1
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&proposed).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                assert_eq!(
                    world
                        .snapshot()
                        .agreements
                        .as_ref()
                        .unwrap()
                        .contracts
                        .len(),
                    1
                );
                world
                    .decide_promises(&[PromiseDecision {
                        contract: 1,
                        actor: 1,
                        accept: true,
                    }])
                    .unwrap();
                restored
                    .decide_promises(&[PromiseDecision {
                        contract: 1,
                        actor: 1,
                        accept: true,
                    }])
                    .unwrap();
                world.set_promise_offer_choice(2, false).unwrap();
                restored.set_promise_offer_choice(2, false).unwrap();
                world.set_promise_obligation_choice(2, None).unwrap();
                restored.set_promise_obligation_choice(2, None).unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                assert_eq!(
                    world.snapshot().agreements.as_ref().unwrap().contracts[0].status,
                    Status::Active
                );
                world.set_promise_choice(2, true).unwrap();
                restored.set_promise_choice(2, true).unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(
                    state.agreements.as_ref().unwrap().contracts[0].status,
                    Status::Fulfilled
                );
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(1))
                        .unwrap()
                        .used,
                    2
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
    let mut declining = Economy::compile(initial.clone()).unwrap();
    declining.advance().unwrap();
    declining
        .decide_promises(&[PromiseDecision {
            contract: 1,
            actor: 1,
            accept: false,
        }])
        .unwrap();
    let state = declining.snapshot();
    assert_eq!(
        state.agreements.as_ref().unwrap().contracts[0].status,
        Status::Declined
    );
    assert_eq!(
        state
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(2))
            .unwrap()
            .used,
        1
    );
    let mut identity_overflow = initial.clone();
    identity_overflow.agreements.as_mut().unwrap().next_id = u64::MAX;
    let mut world = Economy::compile(identity_overflow).unwrap();
    let before = world.snapshot();
    assert!(world.advance().is_err());
    assert_eq!(world.snapshot(), before);
    let mut pressured = initial.clone();
    pressured.agreements.as_mut().unwrap().offering[0].willing = false;
    pressured.agreements.as_mut().unwrap().offering[0].pressure_threshold = Some(3);
    pressured.opinions.as_mut().unwrap().records = vec![Opinion {
        observer: 2,
        target: 1,
        trust: 0,
        fear: 4,
        obligation: 0,
        affinity: 0,
    }];
    let mut world = Economy::compile(pressured).unwrap();
    world.advance().unwrap();
    assert_eq!(
        world
            .snapshot()
            .agreements
            .as_ref()
            .unwrap()
            .contracts
            .len(),
        1
    );
    let mut overflow = initial;
    overflow.agreements.as_mut().unwrap().offering[0].due_after = u64::MAX;
    let mut world = Economy::compile(overflow).unwrap();
    let before = world.snapshot();
    assert!(world.advance().is_err());
    assert_eq!(world.snapshot(), before);
}

fn concentrated_personal_promise_workload(count: u64) -> State {
    let mut state = field_workload(count);
    state.agriculture = None;
    state.people[0].controlled = true;
    state.agreements = Some(Agreements {
        next_id: 1,
        policies: vec![],
        bulk_debtors: vec![],
        contracts: vec![],
        events: vec![],
        work_offering: vec![],
        offering: (2..=count)
            .map(|person| PromiseOfferPolicy {
                person,
                recipient: 1,
                good: 1,
                quantity: 1,
                due_after: 5,
                retry_ticks: 3,
                willing: true,
                recipient_hunger_at_least: 0,
                minimum_trust: None,
                pressure_threshold: None,
                next_review: 0,
                after_work_recipe: None,
                last_work_offer_at: None,
            })
            .collect(),
    });
    let mut opinions = fixture().opinions.unwrap();
    opinions.records.clear();
    opinions.sensitivities = (2..=count)
        .map(|person| OpinionSensitivity {
            person,
            report_reception: None,
            affinity_report_discrepancy: None,
            trust_report_discrepancy: None,
            trust_promise_fulfilled: None,
            trust_promise_unfulfilled: None,
            fear_per_damage: None,
            gratitude_gain: None,
            promise_obligation_gain: Some(1),
            obligation_relief: None,
            theft_loss: None,
            confrontation: None,
            affinity_help: None,
            affinity_harm_per_damage: None,
            affinity_encounter: None,
            affinity_pressured_encounter: None,
            affinity_site_use: None,
            affinity_site_destruction: None,
            affinity_site_salvage: None,
            affinity_work_success: None,
            affinity_work_refusal: None,
            relationship_cases: vec![],
            obligation_development: None,
        })
        .collect::<Vec<_>>()
        .into();
    state.opinions = Some(opinions);
    state
}

#[test]
fn concentrated_autonomous_promises_preserve_individual_feelings_and_choices() {
    let count = 4096;
    let initial = concentrated_personal_promise_workload(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let agreements = state.agreements.as_ref().unwrap();
                assert_eq!(agreements.contracts.len(), count as usize - 1);
                assert!(
                    agreements
                        .contracts
                        .iter()
                        .all(|c| c.creditor == 1 && c.status == Status::Proposed)
                );
                assert_eq!(
                    state.opinions.as_ref().unwrap().records.len(),
                    count as usize - 1
                );
                assert!(
                    state
                        .opinions
                        .as_ref()
                        .unwrap()
                        .records
                        .iter()
                        .all(|r| r.target == 1 && r.obligation == 1)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_autonomous_promises_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = concentrated_personal_promise_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn new_personal_offers_preserve_other_promise_events_in_the_same_boundary() {
    use game_parallel_agreements::EventKind;
    let mut initial = fixture();
    initial.autowork = None;
    initial.people[0].controlled = true;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.agreements.as_mut().unwrap().offering = vec![PromiseOfferPolicy {
        person: 2,
        recipient: 1,
        good: 1,
        quantity: 1,
        due_after: 8,
        retry_ticks: 3,
        willing: true,
        recipient_hunger_at_least: 0,
        minimum_trust: None,
        pressure_threshold: None,
        next_review: 0,
        after_work_recipe: None,
        last_work_offer_at: None,
    }];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let agreements = state.agreements.as_ref().unwrap();
                assert_eq!(agreements.contracts[0].status, Status::Fulfilled);
                assert_eq!(agreements.contracts[1].status, Status::Proposed);
                assert!(
                    agreements
                        .events
                        .iter()
                        .any(|e| e.contract == 1 && e.kind == EventKind::Fulfilled)
                );
                assert!(
                    agreements
                        .events
                        .iter()
                        .any(|e| e.contract == 2 && e.kind == EventKind::Proposed)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn fear_or_felt_obligation_can_motivate_outward_acceptance_without_binding_either_person() {
    let mut initial = fixture();
    initial.autowork = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    let agreements = initial.agreements.as_mut().unwrap();
    agreements.contracts.clear();
    agreements.events.clear();
    agreements.next_id = 1;
    agreements.policies = vec![
        AgreementPolicy {
            person: 1,
            accept: false,
            honor: false,
            minimum_trust: None,
            minimum_honor_trust: None,
            fear_honor_threshold: None,
            obligation_honor_threshold: None,
            accept_pressure_threshold: Some(3),
        },
        AgreementPolicy {
            person: 2,
            accept: false,
            honor: false,
            minimum_trust: None,
            minimum_honor_trust: None,
            fear_honor_threshold: None,
            obligation_honor_threshold: None,
            accept_pressure_threshold: None,
        },
    ];
    agreements.offering = vec![PromiseOfferPolicy {
        person: 2,
        recipient: 1,
        good: 1,
        quantity: 1,
        due_after: 8,
        retry_ticks: 3,
        willing: true,
        recipient_hunger_at_least: 0,
        minimum_trust: None,
        pressure_threshold: None,
        next_review: 0,
        after_work_recipe: None,
        last_work_offer_at: None,
    }];
    initial.opinions.as_mut().unwrap().records = vec![Opinion {
        observer: 1,
        target: 2,
        trust: 0,
        fear: 4,
        obligation: 0,
        affinity: 0,
    }];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                assert_eq!(
                    world.snapshot().agreements.as_ref().unwrap().contracts[0].status,
                    Status::Proposed
                );
                let saved = world.snapshot();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(
                    state.agreements.as_ref().unwrap().contracts[0].status,
                    Status::Active
                );
                assert!(!state.agreements.as_ref().unwrap().policies[0].accept);
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                assert_eq!(
                    world.snapshot().agreements.as_ref().unwrap().contracts[0].status,
                    Status::Active
                );
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
    initial.opinions.as_mut().unwrap().records[0].fear = 0;
    let mut unwilling = Economy::compile(initial.clone()).unwrap();
    unwilling.advance().unwrap();
    unwilling.advance().unwrap();
    assert_eq!(
        unwilling.snapshot().agreements.as_ref().unwrap().contracts[0].status,
        Status::Proposed
    );
    initial.opinions.as_mut().unwrap().records[0].obligation = 4;
    let mut obligated = Economy::compile(initial).unwrap();
    obligated.advance().unwrap();
    obligated.advance().unwrap();
    assert_eq!(
        obligated.snapshot().agreements.as_ref().unwrap().contracts[0].status,
        Status::Active
    );
}

#[test]
fn a_recipient_can_prefer_an_offer_from_someone_they_like() {
    let mut initial = fixture();
    initial.autowork = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    let agreements = initial.agreements.as_mut().unwrap();
    agreements.contracts.clear();
    agreements.events.clear();
    agreements.next_id = 1;
    agreements.policies = vec![AgreementPolicy {
        person: 1,
        accept: true,
        honor: false,
        minimum_trust: None,
        minimum_honor_trust: None,
        fear_honor_threshold: None,
        obligation_honor_threshold: None,
        accept_pressure_threshold: None,
    }];
    agreements.offering = [2, 3]
        .into_iter()
        .map(|person| PromiseOfferPolicy {
            person,
            recipient: 1,
            good: 1,
            quantity: 1,
            due_after: 8,
            retry_ticks: 5,
            willing: true,
            recipient_hunger_at_least: 0,
            minimum_trust: None,
            pressure_threshold: None,
            next_review: 0,
            after_work_recipe: None,
            last_work_offer_at: None,
        })
        .collect();
    let opinions = initial.opinions.as_mut().unwrap();
    opinions.rules.affinity = Some(game_parallel_opinions::AffinityRules {
        limit: 10,
        encounter_change: 0,
        pressured_encounter_change: 0,
        help_change: 0,
        harm_change_per_damage: 0,
    });
    opinions.records = vec![Opinion {
        observer: 1,
        target: 3,
        trust: 0,
        fear: 0,
        obligation: 0,
        affinity: 5,
    }];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let contracts = &state.agreements.as_ref().unwrap().contracts;
                assert_eq!(
                    (contracts[0].debtor, contracts[0].status),
                    (2, Status::Proposed)
                );
                assert_eq!(
                    (contracts[1].debtor, contracts[1].status),
                    (3, Status::Active)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

fn concentrated_promise_response_workload(count: u64) -> State {
    let mut state = concentrated_personal_promise_workload(count);
    state.people[0].controlled = false;
    state.agreements.as_mut().unwrap().policies = vec![AgreementPolicy {
        person: 1,
        honor: false,
        accept: true,
        minimum_trust: None,
        minimum_honor_trust: None,
        fear_honor_threshold: None,
        obligation_honor_threshold: None,
        accept_pressure_threshold: None,
    }];
    state
}

#[test]
fn one_recipient_considers_many_autonomous_offers_without_losing_individual_choices() {
    let count = 4096;
    let initial = concentrated_promise_response_workload(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let saved = world.snapshot();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                let contracts = &state.agreements.as_ref().unwrap().contracts;
                assert_eq!(
                    contracts
                        .iter()
                        .filter(|c| c.status == Status::Active)
                        .count(),
                    1
                );
                assert_eq!(
                    contracts
                        .iter()
                        .filter(|c| c.status == Status::Proposed)
                        .count(),
                    count as usize - 2
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_promise_response_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = concentrated_promise_response_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn actual_newborns_can_develop_personal_obligation_dispositions_without_inheriting_feelings() {
    use game_parallel_dependent_care::{Policy as CarePolicy, Rules as CareRules};
    let mut initial = campaign_fixture();
    initial.campaign = None;
    initial.pending_food.clear();
    initial.nutrition = None;
    for person in &mut initial.people {
        person.controlled = false;
        person.seek_meals = false;
    }
    for member in &mut initial.family.as_mut().unwrap().members {
        member.reproduction_allowed = false;
    }
    initial.care = Some(DependentCare {
        inherit_births: false,
        rules: CareRules {
            dependent_until: 5,
            max_caregiver_hunger: 100,
            neglect_damage: 0,
            feed_from_cargo: false,
            fetch_local_food: false,
            pressure_threshold: None,
        },
        policies: vec![
            CarePolicy {
                person: 1,
                willing: true,
            },
            CarePolicy {
                person: 2,
                willing: true,
            },
        ],
        events: vec![],
    });
    let opinions = initial.opinions.as_mut().unwrap();
    opinions.rules.gratitude_gain = 2;
    opinions.rules.newborn_obligation_development = Some(ObligationDevelopment {
        until_age: 5,
        minimum: 0,
        maximum: 5,
        cared_change: -1,
        unattended_change: 1,
        cared_promise_change: Some(2),
        unattended_promise_change: None,
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                world.advance().unwrap();
                let saved = world.snapshot();
                assert!(
                    saved
                        .family
                        .as_ref()
                        .unwrap()
                        .members
                        .iter()
                        .any(|m| m.id == 5 && m.born == Some(2))
                );
                let opinion = saved.opinions.as_ref().unwrap();
                let child = opinion
                    .sensitivities
                    .iter()
                    .find(|p| p.person == 5)
                    .unwrap();
                assert_eq!(
                    (child.gratitude_gain, child.promise_obligation_gain),
                    (None, None)
                );
                assert!(opinion.records.iter().all(|r| r.observer != 5));
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert!(
                    state
                        .care
                        .as_ref()
                        .unwrap()
                        .events
                        .iter()
                        .any(|e| e.child == 5 && e.caregiver.is_some())
                );
                let child = state
                    .opinions
                    .as_ref()
                    .unwrap()
                    .sensitivities
                    .iter()
                    .find(|p| p.person == 5)
                    .unwrap();
                assert_eq!(
                    (child.gratitude_gain, child.promise_obligation_gain),
                    (Some(1), Some(2))
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
    initial.family.as_mut().unwrap().rules.enroll_births = false;
    let mut world = Economy::compile(initial).unwrap();
    world.advance().unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    assert!(
        state
            .family
            .as_ref()
            .unwrap()
            .members
            .iter()
            .any(|m| m.id == 5)
    );
    assert!(
        state
            .opinions
            .as_ref()
            .unwrap()
            .sensitivities
            .iter()
            .all(|p| p.person != 5)
    );
}

#[test]
fn companions_can_make_accept_and_fulfill_personal_promises_while_traveling() {
    let mut initial = expedition_fixture();
    initial.dynamics = None;
    initial.travel.as_mut().unwrap().routes[0].ticks = 8;
    let mut agreements = fixture().agreements.unwrap();
    agreements.contracts.clear();
    agreements.events.clear();
    agreements.next_id = 1;
    agreements.policies = vec![
        AgreementPolicy {
            person: 1,
            honor: true,
            accept: false,
            minimum_trust: None,
            minimum_honor_trust: None,
            fear_honor_threshold: None,
            obligation_honor_threshold: None,
            accept_pressure_threshold: None,
        },
        AgreementPolicy {
            person: 2,
            honor: false,
            accept: true,
            minimum_trust: None,
            minimum_honor_trust: None,
            fear_honor_threshold: None,
            obligation_honor_threshold: None,
            accept_pressure_threshold: None,
        },
    ];
    agreements.offering = vec![PromiseOfferPolicy {
        person: 1,
        recipient: 2,
        good: 1,
        quantity: 1,
        due_after: 5,
        retry_ticks: 8,
        willing: true,
        recipient_hunger_at_least: 0,
        minimum_trust: None,
        pressure_threshold: None,
        next_review: 0,
        after_work_recipe: None,
        last_work_offer_at: None,
    }];
    initial.agreements = Some(agreements);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world
                    .depart(&[
                        Departure {
                            person: 1,
                            destination: 2,
                        },
                        Departure {
                            person: 2,
                            destination: 2,
                        },
                    ])
                    .unwrap();
                let before = world.snapshot();
                assert!(
                    world
                        .propose_promises(&[PromiseProposal {
                            debtor: 1,
                            creditor: 3,
                            good: 1,
                            quantity: 1,
                            due: 5
                        }])
                        .is_err()
                );
                assert_eq!(world.snapshot(), before);
                world.advance().unwrap();
                let proposed = world.snapshot();
                assert_eq!(
                    proposed.agreements.as_ref().unwrap().contracts[0].status,
                    Status::Proposed
                );
                assert_eq!(
                    proposed
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(1))
                        .unwrap()
                        .used,
                    1
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&proposed).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                assert_eq!(
                    world.snapshot().agreements.as_ref().unwrap().contracts[0].status,
                    Status::Active
                );
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(
                    state.agreements.as_ref().unwrap().contracts[0].status,
                    Status::Fulfilled
                );
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(1))
                        .unwrap()
                        .used,
                    0
                );
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap()
                        .used,
                    1
                );
                assert!(
                    state
                        .travel
                        .as_ref()
                        .unwrap()
                        .journeys
                        .iter()
                        .all(|j| j.remaining == 5)
                );
                assert_eq!(state.opinions.as_ref().unwrap().index().trust(2, 1), 1);
                state
            })
    };
    assert_eq!(run(1), run(4));

    // The controlled maker can independently propose and deliver; the person left
    // at the source cannot participate, and refusal never moves goods.
    initial.people[0].controlled = true;
    initial.agreements.as_mut().unwrap().offering.clear();
    let mut world = Economy::compile(initial.clone()).unwrap();
    world
        .depart(&[
            Departure {
                person: 1,
                destination: 2,
            },
            Departure {
                person: 2,
                destination: 2,
            },
        ])
        .unwrap();
    let id = world
        .propose_promises(&[PromiseProposal {
            debtor: 1,
            creditor: 2,
            good: 1,
            quantity: 1,
            due: 5,
        }])
        .unwrap()[0];
    world
        .decide_promises(&[PromiseDecision {
            contract: id,
            actor: 2,
            accept: true,
        }])
        .unwrap();
    assert!(world.fulfill_promise(1, id).unwrap());
    assert_eq!(
        world.snapshot().travel.as_ref().unwrap().journeys[0].remaining,
        7
    );

    // A proposal made before departure cannot be accepted or fulfilled remotely.
    let mut separated = Economy::compile(initial.clone()).unwrap();
    let id = separated
        .propose_promises(&[PromiseProposal {
            debtor: 1,
            creditor: 2,
            good: 1,
            quantity: 1,
            due: 5,
        }])
        .unwrap()[0];
    separated
        .depart(&[Departure {
            person: 1,
            destination: 2,
        }])
        .unwrap();
    let before = separated.snapshot();
    assert!(
        separated
            .decide_promises(&[PromiseDecision {
                contract: id,
                actor: 2,
                accept: true
            }])
            .is_err()
    );
    assert_eq!(separated.snapshot(), before);
    separated.advance().unwrap();
    assert_eq!(
        separated.snapshot().agreements.as_ref().unwrap().contracts[0].status,
        Status::Proposed
    );
    assert_eq!(
        separated
            .snapshot()
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(1))
            .unwrap()
            .used,
        1
    );

    // People on the same route but at different progress are separate too.
    initial.people[0].controlled = false;
    initial.agreements.as_mut().unwrap().offering = vec![PromiseOfferPolicy {
        person: 1,
        recipient: 2,
        good: 1,
        quantity: 1,
        due_after: 5,
        retry_ticks: 8,
        willing: true,
        recipient_hunger_at_least: 0,
        minimum_trust: None,
        pressure_threshold: None,
        next_review: 0,
        after_work_recipe: None,
        last_work_offer_at: None,
    }];
    let mut staggered = Economy::compile(initial).unwrap();
    staggered
        .depart(&[Departure {
            person: 1,
            destination: 2,
        }])
        .unwrap();
    staggered.advance().unwrap();
    staggered
        .depart(&[Departure {
            person: 2,
            destination: 2,
        }])
        .unwrap();
    staggered.advance().unwrap();
    assert!(
        staggered
            .snapshot()
            .agreements
            .as_ref()
            .unwrap()
            .contracts
            .is_empty()
    );
}

#[test]
fn people_interpret_witnessed_work_and_refusal_individually() {
    use game_parallel_autowork::requests::{Preference, Request, Requests};
    use game_parallel_opinions::AffinityRules;
    let initial = |help: bool, override_case: bool| {
        let mut state = fixture();
        state.agreements = None;
        for person in &mut state.people {
            person.seek_meals = false;
        }
        let opinions = state.opinions.as_mut().unwrap();
        opinions.records.clear();
        opinions.rules.affinity = Some(AffinityRules {
            limit: 10,
            encounter_change: 0,
            pressured_encounter_change: 0,
            help_change: 0,
            harm_change_per_damage: 0,
        });
        opinions.sensitivities = vec![OpinionSensitivity {
            person: 1,
            report_reception: None,
            affinity_report_discrepancy: None,
            trust_report_discrepancy: None,
            trust_promise_fulfilled: None,
            trust_promise_unfulfilled: None,
            fear_per_damage: None,
            gratitude_gain: None,
            promise_obligation_gain: None,
            obligation_relief: None,
            theft_loss: None,
            confrontation: None,
            affinity_help: None,
            affinity_harm_per_damage: None,
            affinity_encounter: None,
            affinity_pressured_encounter: None,
            affinity_site_use: None,
            affinity_site_destruction: None,
            affinity_site_salvage: None,
            affinity_work_success: Some(-3),
            affinity_work_refusal: Some(5),
            relationship_cases: if override_case {
                vec![RelationshipCase {
                    target: 2,
                    report_discrepancy: None,
                    report_discrepancy_trust: None,
                    promise_fulfilled_trust: None,
                    promise_unfulfilled_trust: None,
                    help: None,
                    harm_per_damage: None,
                    encounter: None,
                    pressured_encounter: None,
                    site_use: None,
                    site_destruction: None,
                    site_salvage: None,
                    work_success: Some(4),
                    work_refusal: Some(-2),
                    obligation_gain: None,
                    promise_obligation_gain: None,
                    obligation_relief: None,
                }]
            } else {
                vec![]
            },
            obligation_development: None,
        }]
        .into();
        state.autowork = Some(Autowork {
            next_id: 1,
            policies: vec![],
            requests: Some(Requests {
                next_id: 2,
                inherit_births: false,
                memory_limit: 0,
                work_links: vec![],
                memories: vec![],
                preferences: vec![Preference {
                    person: 2,
                    effort: 1,
                    help_value: u64::from(help),
                    affinity_weight: 0,
                    trust_weight: 0,
                    offer_value: None,
                    disregard_claims: false,
                }],
                askers: vec![],
                pending: vec![Request {
                    id: 1,
                    requester: 1,
                    worker: 2,
                    region: 1,
                    recipe: 1,
                    created_at: 0,
                    expires: 3,
                }],
                events: vec![],
            }),
        });
        state
    };
    for (help, override_case, expected) in [
        (true, false, -3),
        (true, true, 4),
        (false, false, 5),
        (false, true, -2),
    ] {
        let state = initial(help, override_case);
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut world = Economy::compile(state.clone()).unwrap();
                    world.advance().unwrap();
                    if help {
                        assert_eq!(
                            world
                                .snapshot()
                                .opinions
                                .as_ref()
                                .unwrap()
                                .index()
                                .affinity(1, 2),
                            0
                        );
                        let mut restored = Economy::compile(
                            serde_json::from_str(
                                &serde_json::to_string(&world.snapshot()).unwrap(),
                            )
                            .unwrap(),
                        )
                        .unwrap();
                        world.advance().unwrap();
                        restored.advance().unwrap();
                        assert_eq!(world.snapshot(), restored.snapshot());
                    }
                    let state = world.snapshot();
                    assert_eq!(
                        state.opinions.as_ref().unwrap().index().affinity(1, 2),
                        expected
                    );
                    assert_eq!(state.opinions.as_ref().unwrap().index().obligation(1, 2), 0);
                    assert!(
                        state
                            .autowork
                            .as_ref()
                            .unwrap()
                            .requests
                            .as_ref()
                            .unwrap()
                            .memories
                            .is_empty()
                    );
                    state
                })
        };
        assert_eq!(run(1), run(4));
    }
    let mut world = Economy::compile(initial(true, false)).unwrap();
    world.advance().unwrap();
    let mut departed = world.snapshot();
    departed.people[0].region = Some(2);
    let mut departed = Economy::compile(departed).unwrap();
    departed.advance().unwrap();
    assert_eq!(
        departed.snapshot().opinions.unwrap().index().affinity(1, 2),
        0
    );
}

#[test]
fn concentrated_work_refusals_use_individual_reactions_across_workers() {
    let initial = concentrated_work_refusals(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let records = &state.opinions.as_ref().unwrap().records;
                assert_eq!(records.len(), 2048);
                assert!(
                    records
                        .iter()
                        .all(|row| row.affinity == 2 && row.obligation == 0)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

fn concentrated_offered_work(count: u64) -> State {
    use game_parallel_agreements::{Agreements, Contract, Status};
    use game_parallel_autowork::requests::{OfferValue, Preference, Request, Requests};

    let mut state = field_workload(count);
    state.agriculture = None;
    state.opinions = None;
    state.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: Some(Requests {
            next_id: count,
            inherit_births: false,
            memory_limit: 0,
            work_links: vec![],
            memories: vec![],
            preferences: (2..=count)
                .map(|person| Preference {
                    person,
                    effort: 1,
                    help_value: 0,
                    affinity_weight: 0,
                    trust_weight: 0,
                    offer_value: Some(OfferValue {
                        good: 1,
                        value_per_unit: 1,
                        max_quantity: 1,
                    }),
                    disregard_claims: false,
                })
                .collect(),
            askers: vec![],
            pending: (2..=count)
                .map(|worker| Request {
                    id: worker - 1,
                    requester: 1,
                    worker,
                    region: 1,
                    recipe: 1,
                    created_at: 0,
                    expires: 5,
                })
                .collect(),
            events: vec![],
        }),
    });
    state.agreements = Some(Agreements {
        next_id: count,
        policies: vec![],
        offering: vec![],
        work_offering: vec![],
        bulk_debtors: vec![],
        events: vec![],
        contracts: (2..=count)
            .map(|creditor| Contract {
                id: creditor - 1,
                debtor: 1,
                creditor,
                good: 1,
                quantity: 1,
                delivered: 0,
                due: 5,
                status: Status::Proposed,
            })
            .collect(),
    });
    state
}

#[test]
fn concentrated_offered_work_conserves_inputs_and_matches_across_workers() {
    let count = 4096;
    let initial = concentrated_offered_work(count);
    let run =
        |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut world = Economy::compile(initial.clone()).unwrap();
                    world.advance().unwrap();
                    let state = world.snapshot();
                    let requests = state.autowork.as_ref().unwrap().requests.as_ref().unwrap();
                    assert_eq!(requests.work_links.len(), count as usize / 2);
                    assert_eq!(
                        state
                            .physical
                            .custody
                            .containers
                            .iter()
                            .find(|row| row.place == Place::Stock { region: 1, good: 2 })
                            .unwrap()
                            .used,
                        0
                    );
                    assert!(
                        state.agreements.as_ref().unwrap().contracts.iter().all(
                            |contract| contract.status
                                == game_parallel_agreements::Status::Proposed
                                && contract.delivered == 0
                        )
                    );
                    state
                })
        };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore = "performance measurement"]
fn concentrated_offered_work_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = concentrated_offered_work(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                let elapsed = started.elapsed().as_secs_f64();
                assert_eq!(
                    world
                        .snapshot()
                        .autowork
                        .unwrap()
                        .requests
                        .unwrap()
                        .work_links
                        .len(),
                    count as usize / 2
                );
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
            #[cfg(feature = "profile")]
            for (name, measurement) in game_diagnostics::snapshot() {
                println!(
                    "{name}: {:.3} ms/call",
                    measurement.nanoseconds as f64 / measurement.calls as f64 / 1_000_000.0
                );
            }
        });
}

fn concentrated_witnessed_goal_work(count: u64) -> State {
    use game_parallel_goals::{Goal, Goals};
    let mut state = concentrated_work_refusals(count);
    state.opinions = None;
    let requests = state.autowork.as_mut().unwrap().requests.as_mut().unwrap();
    requests.memory_limit = 1;
    for preference in &mut requests.preferences {
        preference.help_value = 1;
    }
    state.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: false,
        people: (1..=count / 2)
            .map(|person| Goal {
                person,
                recipe: 1,
                priority: 1,
                target: 1,
                progress: 0,
                next_review: 0,
                enabled: true,
                recognize_help: true,
            })
            .collect(),
        events: vec![],
    });
    state
}

#[test]
fn many_people_recognize_only_completed_requested_work_across_workers() {
    let count = 4096;
    let initial = concentrated_witnessed_goal_work(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert_eq!(world.advance().unwrap().started.len(), count as usize / 2);
                assert!(
                    world
                        .snapshot()
                        .goals
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .all(|goal| goal.progress == 0)
                );
                assert_eq!(world.advance().unwrap().finished.len(), count as usize / 2);
                let state = world.snapshot();
                assert!(
                    state
                        .goals
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .all(|goal| goal.progress == 1)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore = "performance measurement"]
fn concentrated_witnessed_goal_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = concentrated_witnessed_goal_work(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                world.advance().unwrap();
                let elapsed = started.elapsed().as_secs_f64();
                assert!(
                    world
                        .snapshot()
                        .goals
                        .unwrap()
                        .people
                        .iter()
                        .all(|goal| goal.progress == 1)
                );
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=2 elapsed={:.6}",
                samples[1]
            );
        });
}

fn concentrated_work_refusals(count: u64) -> State {
    use game_parallel_autowork::requests::{Preference, Request, Requests};
    use game_parallel_opinions::AffinityRules;
    let mut state = field_workload(count);
    state.agriculture = None;
    let half = count / 2;
    state.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: Some(Requests {
            next_id: half + 1,
            inherit_births: false,
            memory_limit: 0,
            work_links: vec![],
            memories: vec![],
            preferences: (half + 1..=count)
                .map(|person| Preference {
                    person,
                    effort: 1,
                    help_value: 0,
                    affinity_weight: 0,
                    trust_weight: 0,
                    offer_value: None,
                    disregard_claims: false,
                })
                .collect(),
            askers: vec![],
            pending: (1..=half)
                .map(|requester| Request {
                    id: requester,
                    requester,
                    worker: requester + half,
                    region: 1,
                    recipe: 1,
                    created_at: 0,
                    expires: 5,
                })
                .collect(),
            events: vec![],
        }),
    });
    let mut opinions = fixture().opinions.unwrap();
    opinions.records.clear();
    opinions.rules.affinity = Some(AffinityRules {
        limit: 10,
        encounter_change: 0,
        pressured_encounter_change: 0,
        help_change: 0,
        harm_change_per_damage: 0,
    });
    opinions.sensitivities = (1..=half)
        .map(|person| OpinionSensitivity {
            person,
            report_reception: None,
            affinity_report_discrepancy: None,
            trust_report_discrepancy: None,
            trust_promise_fulfilled: None,
            trust_promise_unfulfilled: None,
            fear_per_damage: None,
            gratitude_gain: None,
            promise_obligation_gain: None,
            obligation_relief: None,
            theft_loss: None,
            confrontation: None,
            affinity_help: None,
            affinity_harm_per_damage: None,
            affinity_encounter: None,
            affinity_pressured_encounter: None,
            affinity_site_use: None,
            affinity_site_destruction: None,
            affinity_site_salvage: None,
            affinity_work_success: None,
            affinity_work_refusal: Some(2),
            relationship_cases: vec![],
            obligation_development: None,
        })
        .collect::<Vec<_>>()
        .into();
    state.opinions = Some(opinions);
    state
}

#[test]
fn concentrated_trust_based_refusals_are_deterministic() {
    let initial = concentrated_trust_refusals(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let opinions = state.opinions.as_ref().unwrap();
                assert_eq!(
                    opinions
                        .records
                        .iter()
                        .filter(|row| row.affinity == 2)
                        .count(),
                    2048
                );
                assert_eq!(
                    opinions
                        .records
                        .iter()
                        .filter(|row| row.trust == -1)
                        .count(),
                    2048
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

fn concentrated_trust_refusals(count: u64) -> State {
    let mut state = concentrated_work_refusals(count);
    for preference in &mut state
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .preferences
    {
        preference.trust_weight = 1;
    }
    let half = count / 2;
    state.opinions.as_mut().unwrap().records = (1..=half)
        .map(|requester| Opinion {
            observer: requester + half,
            target: requester,
            trust: -1,
            fear: 0,
            obligation: 0,
            affinity: 0,
        })
        .collect();
    state
}

#[test]
#[ignore = "performance measurement"]
fn concentrated_work_refusals_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let trust = std::env::var_os("LEGACY_TRUST").is_some();
    let initial = if trust {
        concentrated_trust_refusals(count)
    } else {
        concentrated_work_refusals(count)
    };
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let start = std::time::Instant::now();
                world.advance().unwrap();
                let elapsed = start.elapsed().as_secs_f64();
                let records = world.snapshot().opinions.unwrap().records;
                assert_eq!(
                    records
                        .iter()
                        .filter(|row| row.affinity == 2 && row.obligation == 0)
                        .count(),
                    count as usize / 2
                );
                assert_eq!(
                    records.len(),
                    if trust {
                        count as usize
                    } else {
                        count as usize / 2
                    }
                );
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} work_reaction_seconds={:.6}",
                samples[1]
            );
        });
}

#[test]
fn a_person_can_offer_after_refusal_and_ask_again() {
    use game_parallel_autowork::requests::{Asker, Preference, Request, Requests};
    use game_parallel_goals::{Goal, Goals};
    let mut initial = fixture();
    for person in &mut initial.people {
        person.seek_meals = false;
        person.hunger = 0;
        person.willing_to_work = false;
    }
    initial.people[2].health = 0;
    initial.people[3].health = 0;
    initial.opinions = None;
    initial.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: false,
        people: vec![Goal {
            person: 1,
            recipe: 1,
            priority: 1,
            target: 1,
            progress: 0,
            next_review: 0,
            enabled: true,
            recognize_help: true,
        }],
        events: vec![],
    });
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: Some(Requests {
            next_id: 2,
            inherit_births: false,
            memory_limit: 2,
            work_links: vec![],
            memories: vec![],
            preferences: vec![Preference {
                person: 2,
                effort: 5,
                help_value: 0,
                affinity_weight: 0,
                trust_weight: 0,
                offer_value: Some(WorkOfferValue {
                    good: 1,
                    value_per_unit: 5,
                    max_quantity: 1,
                }),
                disregard_claims: false,
            }],
            askers: vec![Asker {
                person: 1,
                interval: 1,
                lifetime: 5,
                retry_after_offer: true,
            }],
            pending: vec![Request {
                id: 1,
                requester: 1,
                worker: 2,
                region: 1,
                recipe: 1,
                created_at: 0,
                expires: 5,
            }],
            events: vec![],
        }),
    });
    let agreements = initial.agreements.as_mut().unwrap();
    agreements.next_id = 1;
    agreements.contracts.clear();
    agreements.events.clear();
    agreements.policies.clear();
    agreements.work_offering = vec![WorkPromiseOfferPolicy {
        person: 1,
        good: 1,
        quantity: 1,
        due_after: 8,
        retry_ticks: 1,
        willing: true,
        after_refusal: true,
        minimum_trust: None,
        pressure_threshold: None,
        next_review: 0,
        last_work_offer_at: None,
    }];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.advance().unwrap().started.is_empty());
                let refused = world.snapshot();
                assert_eq!(
                    refused
                        .autowork
                        .as_ref()
                        .unwrap()
                        .requests
                        .as_ref()
                        .unwrap()
                        .memories[0]
                        .last_refusal_at,
                    Some(1)
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&refused).unwrap()).unwrap(),
                )
                .unwrap();
                let mut starts = Vec::new();
                for _ in 0..6 {
                    starts.extend(world.advance().unwrap().started);
                    restored.advance().unwrap();
                    assert_eq!(world.snapshot(), restored.snapshot());
                }
                let state = world.snapshot();
                assert!(!starts.is_empty());
                assert_eq!(state.goals.as_ref().unwrap().people[0].progress, 1);
                let promise = &state.agreements.as_ref().unwrap().contracts[0];
                assert_eq!(
                    (
                        promise.debtor,
                        promise.creditor,
                        promise.status,
                        promise.delivered
                    ),
                    (1, 2, Status::Proposed, 0)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
    let mut no_retry = initial;
    no_retry
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .askers[0]
        .retry_after_offer = false;
    let mut world = Economy::compile(no_retry).unwrap();
    for _ in 0..7 {
        assert!(world.advance().unwrap().started.is_empty());
    }
    assert_eq!(world.snapshot().agreements.unwrap().contracts.len(), 1);
    assert_eq!(world.snapshot().goals.unwrap().people[0].progress, 0);
}

#[test]
fn a_person_can_offer_the_actual_helper_a_revocable_promise() {
    use game_parallel_autowork::requests::{Preference, Request, Requests};
    let mut initial = fixture();
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.people[0].hunger = 0;
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: Some(Requests {
            inherit_births: false,
            next_id: 2,
            memory_limit: 2,
            work_links: vec![],
            memories: vec![],
            preferences: vec![Preference {
                person: 2,
                effort: 1,
                help_value: 1,
                affinity_weight: 0,
                trust_weight: 0,
                offer_value: None,
                disregard_claims: false,
            }],
            askers: vec![],
            pending: vec![Request {
                id: 1,
                requester: 1,
                worker: 2,
                region: 1,
                recipe: 1,
                created_at: 0,
                expires: 3,
            }],
            events: vec![],
        }),
    });
    let agreements = initial.agreements.as_mut().unwrap();
    agreements.next_id = 1;
    agreements.contracts.clear();
    agreements.events.clear();
    agreements.offering.clear();
    agreements.policies = vec![AgreementPolicy {
        person: 1,
        honor: false,
        accept: false,
        minimum_trust: None,
        minimum_honor_trust: None,
        fear_honor_threshold: None,
        obligation_honor_threshold: None,
        accept_pressure_threshold: None,
    }];
    agreements.work_offering = vec![WorkPromiseOfferPolicy {
        person: 1,
        good: 1,
        quantity: 1,
        due_after: 8,
        retry_ticks: 1,
        willing: true,
        after_refusal: false,
        minimum_trust: None,
        pressure_threshold: None,
        next_review: 0,
        last_work_offer_at: None,
    }];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert_eq!(world.advance().unwrap().started, vec![1]);
                assert!(
                    world
                        .snapshot()
                        .agreements
                        .as_ref()
                        .unwrap()
                        .contracts
                        .is_empty()
                );
                assert_eq!(world.advance().unwrap().finished, vec![1]);
                assert_eq!(
                    world
                        .snapshot()
                        .autowork
                        .unwrap()
                        .requests
                        .unwrap()
                        .memories[0]
                        .last_success,
                    Some(game_parallel_autowork::requests::SuccessReceipt { request: 1, job: 1 })
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&world.snapshot()).unwrap())
                        .unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let offered = world.snapshot();
                assert_eq!(offered, restored.snapshot());
                let contract = &offered.agreements.as_ref().unwrap().contracts[0];
                assert_eq!(
                    (contract.debtor, contract.creditor, contract.status),
                    (1, 2, Status::Proposed)
                );
                assert_eq!(
                    offered.opinions.as_ref().unwrap().index().obligation(1, 2),
                    0
                );
                for sim in [&mut world, &mut restored] {
                    sim.decide_promises(&[PromiseDecision {
                        contract: 1,
                        actor: 2,
                        accept: true,
                    }])
                    .unwrap();
                    sim.set_work_promise_offer_choice(1, false).unwrap();
                    sim.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(
                    state.agreements.as_ref().unwrap().contracts[0].status,
                    Status::Active
                );
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Person(1))
                        .unwrap()
                        .used,
                    1
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
    let mut pressured = initial.clone();
    let preference = &mut pressured.agreements.as_mut().unwrap().work_offering[0];
    preference.willing = false;
    preference.pressure_threshold = Some(5);
    pressured.opinions.as_mut().unwrap().records.push(Opinion {
        observer: 1,
        target: 2,
        trust: 0,
        fear: 5,
        obligation: 0,
        affinity: 0,
    });
    let mut pressured = Economy::compile(pressured).unwrap();
    for _ in 0..3 {
        pressured.advance().unwrap();
    }
    assert_eq!(
        pressured.snapshot().agreements.unwrap().contracts[0].creditor,
        2
    );
    let mut unwilling = Economy::compile(initial).unwrap();
    unwilling.set_work_promise_offer_choice(1, false).unwrap();
    for _ in 0..3 {
        unwilling.advance().unwrap();
    }
    assert!(
        unwilling
            .snapshot()
            .agreements
            .unwrap()
            .contracts
            .is_empty()
    );
}

#[test]
fn concentrated_offers_after_refusals_are_deterministic() {
    let mut initial = concentrated_selected_helper_offers(4096);
    make_refusal_memories(&mut initial);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let contracts = &state.agreements.as_ref().unwrap().contracts;
                assert_eq!(contracts.len(), 4095);
                assert!(
                    contracts
                        .iter()
                        .all(|c| c.creditor == 1 && c.status == Status::Proposed)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

fn make_refusal_memories(initial: &mut State) {
    for policy in &mut initial.agreements.as_mut().unwrap().work_offering {
        policy.after_refusal = true;
    }
    for memory in &mut initial
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .memories
    {
        memory.successes = 0;
        memory.failures = 1;
        memory.last_success_at = None;
        memory.last_refusal_at = Some(0);
    }
}

#[test]
fn concentrated_selected_helper_offers_are_deterministic() {
    let initial = concentrated_selected_helper_offers(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let contracts = &state.agreements.as_ref().unwrap().contracts;
                assert_eq!(contracts.len(), 4095);
                assert!(
                    contracts
                        .iter()
                        .all(|c| c.creditor == 1 && c.status == Status::Proposed)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

fn concentrated_selected_helper_offers(count: u64) -> State {
    use game_parallel_autowork::requests::{Memory, Requests};
    let mut state = concentrated_personal_promise_workload(count);
    let owner = state.agreements.as_mut().unwrap();
    owner.offering.clear();
    owner.work_offering = (2..=count)
        .map(|person| WorkPromiseOfferPolicy {
            person,
            good: 1,
            quantity: 1,
            due_after: 5,
            retry_ticks: 3,
            willing: true,
            after_refusal: false,
            minimum_trust: None,
            pressure_threshold: None,
            next_review: 0,
            last_work_offer_at: None,
        })
        .collect();
    state.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: Some(Requests {
            next_id: 1,
            inherit_births: false,
            memory_limit: 1,
            work_links: vec![],
            memories: (2..=count)
                .map(|observer| Memory {
                    observer,
                    worker: 1,
                    recipe: 1,
                    successes: 1,
                    failures: 0,
                    last_tick: 0,
                    last_success_at: Some(0),
                    last_success: None,
                    last_refusal_at: None,
                })
                .collect(),
            preferences: vec![],
            askers: vec![],
            pending: vec![],
            events: vec![],
        }),
    });
    state
}

#[test]
#[ignore = "performance measurement"]
fn concentrated_selected_helper_offers_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let mut initial = concentrated_selected_helper_offers(count);
    if std::env::var_os("LEGACY_REFUSAL").is_some() {
        make_refusal_memories(&mut initial);
    }
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let start = std::time::Instant::now();
                world.advance().unwrap();
                let elapsed = start.elapsed().as_secs_f64();
                assert_eq!(
                    world.snapshot().agreements.unwrap().contracts.len(),
                    count as usize - 1
                );
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} remembered_offer_seconds={:.6}",
                samples[1]
            );
        });
}

#[test]
fn witnessed_work_can_inspire_a_personal_offer_without_creating_a_duty() {
    use game_parallel_autowork::requests::{Preference, Request, Requests};
    let mut initial = fixture();
    for p in &mut initial.people {
        p.seek_meals = false;
    }
    initial.people[0].hunger = 0;
    initial.people[1].willing_to_work = false;
    initial.opinions.as_mut().unwrap().records = vec![Opinion {
        observer: 2,
        target: 1,
        trust: 0,
        fear: 5,
        obligation: 0,
        affinity: 0,
    }];
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: Some(Requests {
            inherit_births: false,
            next_id: 2,
            memory_limit: 2,
            work_links: vec![],
            memories: vec![],
            preferences: vec![Preference {
                person: 2,
                effort: 5,
                help_value: 0,
                affinity_weight: 0,
                trust_weight: 0,
                offer_value: None,
                disregard_claims: false,
            }],
            askers: vec![],
            pending: vec![Request {
                id: 1,
                requester: 1,
                worker: 2,
                region: 1,
                recipe: 1,
                created_at: 0,
                expires: 3,
            }],
            events: vec![],
        }),
    });
    let agreements = initial.agreements.as_mut().unwrap();
    agreements.next_id = 1;
    agreements.contracts.clear();
    agreements.events.clear();
    agreements.policies = vec![AgreementPolicy {
        person: 1,
        honor: false,
        accept: false,
        minimum_trust: None,
        minimum_honor_trust: None,
        fear_honor_threshold: None,
        obligation_honor_threshold: None,
        accept_pressure_threshold: None,
    }];
    agreements.offering = vec![PromiseOfferPolicy {
        person: 1,
        recipient: 2,
        good: 1,
        quantity: 1,
        due_after: 8,
        retry_ticks: 1,
        willing: true,
        recipient_hunger_at_least: 0,
        minimum_trust: None,
        pressure_threshold: None,
        next_review: 0,
        after_work_recipe: Some(1),
        last_work_offer_at: None,
    }];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert_eq!(world.advance().unwrap().started, vec![1]);
                assert!(
                    world
                        .snapshot()
                        .agreements
                        .as_ref()
                        .unwrap()
                        .contracts
                        .is_empty()
                );
                assert_eq!(world.advance().unwrap().finished, vec![1]);
                let observed = world.snapshot();
                assert!(observed.agreements.as_ref().unwrap().contracts.is_empty());
                assert_eq!(
                    observed
                        .autowork
                        .as_ref()
                        .unwrap()
                        .requests
                        .as_ref()
                        .unwrap()
                        .memories[0]
                        .last_success_at,
                    Some(2)
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&observed).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let offered = world.snapshot();
                assert_eq!(offered, restored.snapshot());
                assert_eq!(
                    offered.agreements.as_ref().unwrap().contracts[0].status,
                    Status::Proposed
                );
                assert_eq!(
                    offered.agreements.as_ref().unwrap().offering[0].last_work_offer_at,
                    Some(2)
                );
                assert_eq!(
                    offered.opinions.as_ref().unwrap().index().obligation(1, 2),
                    0
                );
                for economy in [&mut world, &mut restored] {
                    economy
                        .decide_promises(&[PromiseDecision {
                            contract: 1,
                            actor: 2,
                            accept: true,
                        }])
                        .unwrap();
                    economy.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(
                    state.agreements.as_ref().unwrap().contracts[0].status,
                    Status::Active
                );
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(1))
                        .unwrap()
                        .used,
                    1
                );
                state
            })
    };
    assert_eq!(run(1), run(4));

    // Declining the offer and a later refusal do not repeat an old completion.
    let mut world = Economy::compile(initial.clone()).unwrap();
    for _ in 0..3 {
        world.advance().unwrap();
    }
    world
        .decide_promises(&[PromiseDecision {
            contract: 1,
            actor: 2,
            accept: false,
        }])
        .unwrap();
    let mut state = world.snapshot();
    state.opinions.as_mut().unwrap().records.clear();
    let requests = state.autowork.as_mut().unwrap().requests.as_mut().unwrap();
    requests.next_id = 3;
    requests.pending.push(Request {
        id: 2,
        requester: 1,
        worker: 2,
        region: 1,
        recipe: 1,
        created_at: 3,
        expires: 8,
    });
    let mut world = Economy::compile(state).unwrap();
    for _ in 0..3 {
        world.advance().unwrap();
    }
    let state = world.snapshot();
    assert_eq!(state.agreements.as_ref().unwrap().contracts.len(), 1);
    let memory = &state
        .autowork
        .as_ref()
        .unwrap()
        .requests
        .as_ref()
        .unwrap()
        .memories[0];
    assert_eq!(
        (memory.successes, memory.failures, memory.last_success_at),
        (1, 1, Some(2))
    );

    // A refusal is a memory, but is not a successful completion and cannot trigger an offer.
    let mut refusal = initial.clone();
    refusal.opinions.as_mut().unwrap().records.clear();
    let mut world = Economy::compile(refusal).unwrap();
    for _ in 0..4 {
        world.advance().unwrap();
    }
    let state = world.snapshot();
    let memories = &state
        .autowork
        .as_ref()
        .unwrap()
        .requests
        .as_ref()
        .unwrap()
        .memories;
    assert_eq!(memories[0].last_success_at, None);
    assert!(state.agreements.as_ref().unwrap().contracts.is_empty());

    // Completing work while its requester is away does not give that person knowledge.
    let mut world = Economy::compile(initial).unwrap();
    world.advance().unwrap();
    let mut away = world.snapshot();
    away.people[0].region = Some(2);
    let mut away = Economy::compile(away).unwrap();
    away.advance().unwrap();
    let mut returned = away.snapshot();
    returned.people[0].region = Some(1);
    let mut returned = Economy::compile(returned).unwrap();
    returned.advance().unwrap();
    assert!(
        returned
            .snapshot()
            .agreements
            .as_ref()
            .unwrap()
            .contracts
            .is_empty()
    );
}

fn concentrated_remembered_work_offers(count: u64) -> State {
    use game_parallel_autowork::requests::{Memory, Requests};
    let mut state = concentrated_personal_promise_workload(count);
    for offer in &mut state.agreements.as_mut().unwrap().offering {
        offer.after_work_recipe = Some(1);
    }
    state.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: Some(Requests {
            inherit_births: false,
            next_id: 1,
            memory_limit: 1,
            work_links: vec![],
            memories: (2..=count)
                .map(|observer| Memory {
                    observer,
                    worker: 1,
                    recipe: 1,
                    successes: 1,
                    failures: 0,
                    last_tick: 0,
                    last_success_at: Some(0),
                    last_success: None,
                    last_refusal_at: None,
                })
                .collect(),
            preferences: vec![],
            askers: vec![],
            pending: vec![],
            events: vec![],
        }),
    });
    state
}

#[test]
fn many_people_can_independently_recall_work_and_make_offers() {
    let count = 4096;
    let initial = concentrated_remembered_work_offers(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state.agreements.as_ref().unwrap().contracts.len(),
                    count as usize - 1
                );
                assert!(
                    state
                        .agreements
                        .as_ref()
                        .unwrap()
                        .offering
                        .iter()
                        .all(|p| p.last_work_offer_at == Some(0))
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_remembered_work_offers_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = concentrated_remembered_work_offers(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn reusable_tools_gate_work_without_blocking_an_equipped_competitor() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.opinions = None;
    for person in &mut initial.people {
        person.seek_meals = false;
        person.willing_to_work = true;
    }
    initial.work_tools = vec![WorkTool {
        recipe: 1,
        good: 3,
        quantity: 1,
    }];
    initial.autowork.as_mut().unwrap().policies = (1..=2)
        .map(|person| WorkPolicy {
            person,
            recipe: 1,
            enabled: true,
            disregard_claims: false,
        })
        .collect();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 2 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![Lot {
            good: 2,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(2))
        .unwrap();
    *holder = Container::new(
        holder.place,
        holder.capacity,
        vec![Lot {
            good: 3,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let events = world.advance().unwrap();
                assert_eq!(events.started.len(), 1);
                let state = world.snapshot();
                assert_eq!(state.physical.work[0].job.worker, 2);
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap()
                        .used,
                    1
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap()
                        .used,
                    1
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
    let mut invalid = initial;
    invalid.work_tools[0].good = 999;
    assert!(Economy::compile(invalid).is_err());
}

#[test]
fn work_pauses_when_a_carried_tool_expires_and_resumes_after_actual_replacement() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork.as_mut().unwrap().policies.clear();
    initial.physical.recipes[0].ticks = 3;
    initial.people[0].controlled = true;
    initial.people[2].controlled = true;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.work_tools = vec![WorkTool {
        recipe: 1,
        good: 3,
        quantity: 1,
    }];
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(
        holder.place,
        holder.capacity,
        vec![Lot {
            good: 3,
            quantity: 1,
            expires_at: Some(3),
        }],
    )
    .unwrap();
    let spare = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(3))
        .unwrap();
    *spare = Container::new(
        spare.place,
        spare.capacity,
        vec![Lot {
            good: 3,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut unequipped = initial.clone();
    let holder = unequipped
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(holder.place, holder.capacity, vec![]).unwrap();
    let mut blocked = Economy::compile(unequipped).unwrap();
    assert!(blocked.start_work(1, 1, 1).unwrap().started.is_empty());
    assert!(blocked.snapshot().physical.work.is_empty());
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert_eq!(world.start_work(1, 1, 1).unwrap().started.len(), 1);
                world.continue_work(1).unwrap();
                assert_eq!(world.snapshot().physical.work[0].job.remaining, 2);
                world.continue_work(1).unwrap();
                let paused = world.snapshot();
                assert_eq!(paused.physical.work[0].job.remaining, 2);
                assert_eq!(
                    paused
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(1))
                        .unwrap()
                        .used,
                    0
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&paused).unwrap()).unwrap(),
                )
                .unwrap();
                assert!(world.give(3, 1, 3, 1).unwrap());
                assert!(restored.give(3, 1, 3, 1).unwrap());
                world.continue_work(1).unwrap();
                restored.continue_work(1).unwrap();
                assert_eq!(world.snapshot().physical.work[0].job.remaining, 1);
                assert_eq!(world.continue_work(1).unwrap().finished.len(), 1);
                restored.continue_work(1).unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(1))
                        .unwrap()
                        .used,
                    1
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

fn concentrated_tool_workload(count: u64) -> State {
    let mut state = concentrated(count);
    state.work_tools = vec![WorkTool {
        recipe: 1,
        good: 3,
        quantity: 1,
    }];
    for holder in &mut state.physical.custody.containers {
        if let Place::Person(id) = holder.place
            && id > count / 2
            && id % 2 == 0
        {
            *holder = Container::new(
                holder.place,
                holder.capacity,
                vec![Lot {
                    good: 3,
                    quantity: 1,
                    expires_at: None,
                }],
            )
            .unwrap();
        }
    }
    state
}

#[test]
fn concentrated_tool_dependent_work_admits_only_equipped_workers() {
    let count = 4096;
    let initial = concentrated_tool_workload(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let events = world.advance().unwrap();
                assert_eq!(events.started.len(), count as usize / 4);
                let state = world.snapshot();
                assert!(state.physical.work.iter().all(|w| w.job.worker % 2 == 0));
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_tool_work_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = concentrated_tool_workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn autonomous_workers_obtain_tools_from_real_stock_and_respect_personal_claims() {
    use game_parallel_economy_claims::{Claim, Claims};
    let mut initial = fixture();
    initial.agreements = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.work_tools = vec![WorkTool {
        recipe: 1,
        good: 3,
        quantity: 1,
    }];
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 3 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![Lot {
            good: 3,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.advance().unwrap().started.is_empty());
                let pickup = world.snapshot();
                assert_eq!(
                    pickup
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap()
                        .used,
                    1
                );
                assert_eq!(
                    pickup
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Stock { region: 1, good: 3 })
                        .unwrap()
                        .used,
                    0
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&pickup).unwrap()).unwrap(),
                )
                .unwrap();
                assert_eq!(world.advance().unwrap().started.len(), 1);
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap()
                        .used,
                    1
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
    initial.dynamics = Some(Dynamics {
        personality: None,
        social: None,
        disease: None,
        treatment: vec![],
        claims: Some(Claims {
            inherit_births: false,
            site_claims: vec![],
            site_events: vec![],
            claims: vec![Claim {
                person: 1,
                region: 1,
                good: 3,
            }],
            preferences: vec![],
            events: vec![],
        }),
    });
    let mut respecting = Economy::compile(initial.clone()).unwrap();
    respecting.advance().unwrap();
    assert_eq!(
        respecting
            .snapshot()
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(2))
            .unwrap()
            .used,
        0
    );
    initial.autowork.as_mut().unwrap().policies[0].disregard_claims = true;
    let mut disregarding = Economy::compile(initial).unwrap();
    disregarding.advance().unwrap();
    let state = disregarding.snapshot();
    assert_eq!(
        state
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(2))
            .unwrap()
            .used,
        1
    );
    assert!(
        state
            .dynamics
            .as_ref()
            .unwrap()
            .claims
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|e| e.person == 2 && e.good == 3)
    );
}

#[test]
fn tool_pickup_and_gathering_share_one_physical_boundary() {
    let mut initial = fixture();
    initial.agreements = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.work_tools = vec![WorkTool {
        recipe: 1,
        good: 3,
        quantity: 1,
    }];
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 3 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![Lot {
            good: 3,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    initial.gathering = Some(Gathering {
        inherit_births: false,
        max_hunger: 20,
        nodes: vec![ResourceNode {
            id: 1,
            region: 1,
            good: 2,
            available: 1,
            capacity: 1,
            regrowth: 0,
            yield_per_action: 1,
        }],
        policies: vec![GatherPolicy {
            person: 3,
            node: 1,
            willing: true,
            pressured_by: None,
            pressure_threshold: None,
        }],
    });
    let mut world = Economy::compile(initial).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    assert_eq!(state.gathering.unwrap().nodes[0].available, 0);
    assert_eq!(
        state
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(2))
            .unwrap()
            .used,
        1
    );
    assert_eq!(
        state
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Stock { region: 1, good: 2 })
            .unwrap()
            .used,
        3
    );
}

fn concentrated_tool_pickups(count: u64) -> State {
    let mut state = concentrated(count);
    state.work_tools = vec![WorkTool {
        recipe: 1,
        good: 3,
        quantity: 1,
    }];
    let stock = state
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 3 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![Lot {
            good: 3,
            quantity: count / 8,
            expires_at: None,
        }],
    )
    .unwrap();
    state
}

#[test]
fn many_workers_compete_for_real_tools_without_duplicating_them() {
    let count = 4096;
    let initial = concentrated_tool_pickups(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.advance().unwrap().started.is_empty());
                let state = world.snapshot();
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .filter(|c| matches!(c.place, Place::Person(id) if id > count / 2)
                            && c.lots.iter().any(|l| l.good == 3))
                        .count(),
                    count as usize / 8
                );
                assert_eq!(world.advance().unwrap().started.len(), count as usize / 8);
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_tool_pickup_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = concentrated_tool_pickups(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn an_autonomous_worker_replaces_an_expired_tool_only_if_they_continue() {
    let mut initial = fixture();
    initial.agreements = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.physical.recipes[0].ticks = 3;
    initial.work_tools = vec![WorkTool {
        recipe: 1,
        good: 3,
        quantity: 1,
    }];
    for (place, expires_at) in [
        (Place::Person(2), Some(3)),
        (Place::Stock { region: 1, good: 3 }, None),
    ] {
        let holder = initial
            .physical
            .custody
            .containers
            .iter_mut()
            .find(|c| c.place == place)
            .unwrap();
        *holder = Container::new(
            holder.place,
            holder.capacity,
            vec![Lot {
                good: 3,
                quantity: 1,
                expires_at,
            }],
        )
        .unwrap();
    }
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert_eq!(world.advance().unwrap().started.len(), 1);
                world.advance().unwrap();
                let previous = world.snapshot();
                assert_eq!(previous.physical.work[0].job.remaining, 2);
                let mut stopped = Economy::compile(previous.clone()).unwrap();
                stopped.set_work_choice(2, false).unwrap();
                stopped.advance().unwrap();
                assert_eq!(
                    stopped
                        .snapshot()
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Stock { region: 1, good: 3 })
                        .unwrap()
                        .used,
                    1
                );
                world.advance().unwrap();
                let replaced = world.snapshot();
                assert_eq!(replaced.physical.work[0].job.remaining, 2);
                assert_eq!(
                    replaced
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap()
                        .used,
                    1
                );
                assert_eq!(
                    replaced
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Stock { region: 1, good: 3 })
                        .unwrap()
                        .used,
                    0
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&replaced).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.advance().unwrap().finished.len(), 1);
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn fear_can_motivate_a_worker_to_replace_a_tool_without_general_willingness() {
    use game_parallel_autowork::requests::{Preference, Request, Requests};
    let mut initial = fixture();
    initial.agreements = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.people[1].willing_to_work = false;
    initial.physical.recipes[0].ticks = 3;
    initial.work_tools = vec![WorkTool {
        recipe: 1,
        good: 3,
        quantity: 1,
    }];
    initial.opinions.as_mut().unwrap().records = vec![Opinion {
        observer: 2,
        target: 1,
        trust: 0,
        fear: 5,
        obligation: 0,
        affinity: 0,
    }];
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: Some(Requests {
            inherit_births: false,
            next_id: 2,
            memory_limit: 1,
            work_links: vec![],
            memories: vec![],
            preferences: vec![Preference {
                person: 2,
                effort: 5,
                help_value: 0,
                affinity_weight: 0,
                trust_weight: 0,
                offer_value: None,
                disregard_claims: false,
            }],
            askers: vec![],
            pending: vec![Request {
                id: 1,
                requester: 1,
                worker: 2,
                region: 1,
                recipe: 1,
                created_at: 0,
                expires: 4,
            }],
            events: vec![],
        }),
    });
    for (place, expires_at) in [
        (Place::Person(2), Some(3)),
        (Place::Stock { region: 1, good: 3 }, None),
    ] {
        let container = initial
            .physical
            .custody
            .containers
            .iter_mut()
            .find(|c| c.place == place)
            .unwrap();
        *container = Container::new(
            container.place,
            container.capacity,
            vec![Lot {
                good: 3,
                quantity: 1,
                expires_at,
            }],
        )
        .unwrap();
    }
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert_eq!(world.advance().unwrap().started.len(), 1);
                world.advance().unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert!(!state.people[1].willing_to_work);
                assert_eq!(state.physical.work[0].job.remaining, 2);
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap()
                        .used,
                    1
                );
                state
            })
    };
    assert_eq!(run(1), run(4));

    // Requested work uses the worker's preference for that request, rather than
    // a different standing preference for their own independent work.
    use game_parallel_economy_claims::{Claim, Claims};
    let mut world = Economy::compile(initial).unwrap();
    world.advance().unwrap();
    world.advance().unwrap();
    let mut contested = world.snapshot();
    contested.dynamics = Some(Dynamics {
        personality: None,
        social: None,
        disease: None,
        treatment: vec![],
        claims: Some(Claims {
            inherit_births: false,
            site_claims: vec![],
            site_events: vec![],
            claims: vec![Claim {
                person: 1,
                region: 1,
                good: 3,
            }],
            preferences: vec![],
            events: vec![],
        }),
    });
    contested
        .autowork
        .as_mut()
        .unwrap()
        .policies
        .push(WorkPolicy {
            person: 2,
            recipe: 1,
            enabled: true,
            disregard_claims: true,
        });
    let mut respecting = Economy::compile(contested.clone()).unwrap();
    respecting.advance().unwrap();
    assert_eq!(
        respecting
            .snapshot()
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Stock { region: 1, good: 3 })
            .unwrap()
            .used,
        1
    );
    contested.autowork.as_mut().unwrap().policies[0].disregard_claims = false;
    contested
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .preferences[0]
        .disregard_claims = true;
    let mut disregarding = Economy::compile(contested).unwrap();
    disregarding.advance().unwrap();
    assert_eq!(
        disregarding
            .snapshot()
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(2))
            .unwrap()
            .used,
        1
    );
}

#[test]
fn fear_can_motivate_fetching_a_tool_for_requested_work_without_consent() {
    use game_parallel_autowork::requests::{Preference, Request, Requests};
    let mut initial = fixture();
    initial.agreements = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.people[1].willing_to_work = false;
    initial.work_tools = vec![WorkTool {
        recipe: 1,
        good: 3,
        quantity: 1,
    }];
    initial.opinions.as_mut().unwrap().records = vec![Opinion {
        observer: 2,
        target: 1,
        trust: 0,
        fear: 5,
        obligation: 0,
        affinity: 0,
    }];
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: Some(Requests {
            inherit_births: false,
            next_id: 2,
            memory_limit: 1,
            work_links: vec![],
            memories: vec![],
            preferences: vec![Preference {
                person: 2,
                effort: 5,
                help_value: 0,
                affinity_weight: 0,
                trust_weight: 0,
                offer_value: None,
                disregard_claims: false,
            }],
            askers: vec![],
            pending: vec![Request {
                id: 1,
                requester: 1,
                worker: 2,
                region: 1,
                recipe: 1,
                created_at: 0,
                expires: 4,
            }],
            events: vec![],
        }),
    });
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 3 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![Lot {
            good: 3,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.advance().unwrap().started.is_empty());
                let state = world.snapshot();
                assert!(!state.people[1].willing_to_work);
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap()
                        .used,
                    1
                );
                assert_eq!(
                    state
                        .autowork
                        .as_ref()
                        .unwrap()
                        .requests
                        .as_ref()
                        .unwrap()
                        .pending
                        .len(),
                    1
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                assert_eq!(world.advance().unwrap().started.len(), 1);
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
    initial.opinions.as_mut().unwrap().records.clear();
    let mut refusing = Economy::compile(initial).unwrap();
    refusing.advance().unwrap();
    let state = refusing.snapshot();
    assert_eq!(
        state
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(2))
            .unwrap()
            .used,
        0
    );
    assert_eq!(
        state
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Stock { region: 1, good: 3 })
            .unwrap()
            .used,
        1
    );
}

#[test]
fn a_person_can_fetch_a_tool_for_their_own_work_goal() {
    use game_parallel_goals::{Goal, Goals};
    let mut initial = fixture();
    initial.agreements = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.autowork.as_mut().unwrap().policies.clear();
    initial.work_tools = vec![WorkTool {
        recipe: 1,
        good: 3,
        quantity: 1,
    }];
    initial.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: false,
        people: vec![Goal {
            person: 2,
            recipe: 1,
            priority: 1,
            target: 1,
            progress: 0,
            next_review: 0,
            enabled: true,
            recognize_help: false,
        }],
        events: vec![],
    });
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 3 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![Lot {
            good: 3,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.advance().unwrap().started.is_empty());
                let state = world.snapshot();
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap()
                        .used,
                    1
                );
                assert_eq!(state.goals.as_ref().unwrap().people[0].progress, 0);
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                assert_eq!(world.advance().unwrap().started.len(), 1);
                restored.advance().unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                assert_eq!(
                    world.snapshot().goals.as_ref().unwrap().people[0].progress,
                    1
                );
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
    initial.people[1].willing_to_work = false;
    let mut unwilling = Economy::compile(initial).unwrap();
    unwilling.advance().unwrap();
    assert_eq!(
        unwilling
            .snapshot()
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(2))
            .unwrap()
            .used,
        0
    );
}

#[test]
fn goal_input_planning_gets_the_tool_for_the_selected_prerequisite() {
    use game_parallel_goals::{Goal, Goals};
    let mut initial = fixture();
    initial.agreements = None;
    for person in &mut initial.people {
        person.seek_meals = false;
    }
    initial.autowork.as_mut().unwrap().policies.clear();
    initial.physical.recipes.push(Recipe {
        id: 4,
        name: "prepare input".into(),
        ticks: 1,
        inputs: vec![Amount {
            good: 1,
            quantity: 1,
        }],
        outputs: vec![Amount {
            good: 2,
            quantity: 1,
        }],
    });
    initial.work_tools = vec![WorkTool {
        recipe: 4,
        good: 3,
        quantity: 1,
    }];
    initial.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: true,
        people: vec![Goal {
            person: 2,
            recipe: 1,
            priority: 1,
            target: 1,
            progress: 0,
            next_review: 0,
            enabled: true,
            recognize_help: false,
        }],
        events: vec![],
    });
    for (good, quantity) in [(1, 1), (2, 0), (3, 1)] {
        let stock = initial
            .physical
            .custody
            .containers
            .iter_mut()
            .find(|c| c.place == Place::Stock { region: 1, good })
            .unwrap();
        let lots = if quantity == 0 {
            vec![]
        } else {
            vec![Lot {
                good,
                quantity,
                expires_at: None,
            }]
        };
        *stock = Container::new(stock.place, stock.capacity, lots).unwrap();
    }
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.advance().unwrap().started.is_empty());
                let state = world.snapshot();
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap()
                        .used,
                    1
                );
                assert_eq!(world.advance().unwrap().started.len(), 1);
                assert_eq!(world.snapshot().physical.work[0].job.recipe, 4);
                world.advance().unwrap();
                assert!(world.advance().unwrap().started.is_empty());
                assert!(world.snapshot().goals.as_ref().unwrap().plan_inputs);
                assert_eq!(world.advance().unwrap().started.len(), 1);
                assert_eq!(world.snapshot().physical.work[0].job.recipe, 1);
                world.advance().unwrap();
                assert_eq!(
                    world.snapshot().goals.as_ref().unwrap().people[0].progress,
                    1
                );
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

fn concentrated_goal_tool_pickups(count: u64) -> State {
    use game_parallel_goals::{Goal, Goals};
    let mut state = concentrated_tool_pickups(count);
    state.autowork.as_mut().unwrap().policies.clear();
    state.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: false,
        people: (count / 2 + 1..=count)
            .map(|person| Goal {
                person,
                recipe: 1,
                priority: 1,
                target: 1,
                progress: 0,
                next_review: 0,
                enabled: true,
                recognize_help: false,
            })
            .collect(),
        events: vec![],
    });
    state
}

#[test]
fn many_personal_goals_compete_for_tools_without_duplicate_plans() {
    let count = 4096;
    let initial = concentrated_goal_tool_pickups(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .filter(|c| matches!(c.place, Place::Person(id) if id > count / 2)
                            && c.lots.iter().any(|l| l.good == 3))
                        .count(),
                    count as usize / 8
                );
                assert_eq!(world.advance().unwrap().started.len(), count as usize / 8);
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_goal_tool_pickup_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = concentrated_goal_tool_pickups(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn physical_work_sites_limit_jobs_and_paused_jobs_keep_their_slots() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.physical.recipes[0].ticks = 3;
    for person in &mut initial.people {
        person.seek_meals = false;
        person.willing_to_work = true;
        person.hunger = 0;
    }
    initial.autowork.as_mut().unwrap().policies = [2, 4]
        .into_iter()
        .map(|person| WorkPolicy {
            person,
            recipe: 1,
            enabled: true,
            disregard_claims: false,
        })
        .collect();
    initial.work_sites = Some(WorkSites {
        next_id: 0,
        sites: vec![WorkSite {
            id: 7,
            region: 1,
            kind: "workshop".into(),
            capacity: 1,
        }],
        requirements: vec![WorkSiteRequirement {
            recipe: 1,
            kind: "workshop".into(),
            slots: 1,
        }],
        bindings: vec![],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert_eq!(world.advance().unwrap().started.len(), 1);
                let mut state = world.snapshot();
                assert_eq!(state.work_sites.as_ref().unwrap().bindings.len(), 1);
                let worker = state.physical.work[0].job.worker;
                let remaining = state.physical.work[0].job.remaining;
                state
                    .people
                    .iter_mut()
                    .find(|p| p.id == worker)
                    .unwrap()
                    .hunger = 11;
                let mut paused = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                assert!(paused.advance().unwrap().started.is_empty());
                let mut paused_state = paused.snapshot();
                assert_eq!(paused_state.physical.work[0].job.remaining, remaining);
                assert_eq!(paused_state.work_sites.as_ref().unwrap().bindings.len(), 1);
                let mut abandoning = Economy::compile(paused_state.clone()).unwrap();
                abandoning.set_work_choice(worker, false).unwrap();
                abandoning.advance().unwrap();
                assert!(
                    abandoning
                        .snapshot()
                        .work_sites
                        .as_ref()
                        .unwrap()
                        .bindings
                        .is_empty()
                );
                paused_state
                    .people
                    .iter_mut()
                    .find(|p| p.id == worker)
                    .unwrap()
                    .hunger = 0;
                let mut resumed = Economy::compile(paused_state).unwrap();
                for _ in 0..3 {
                    resumed.advance().unwrap();
                }
                let completed = resumed.snapshot();
                assert!(completed.work_sites.as_ref().unwrap().bindings.is_empty());
                assert_eq!(resumed.advance().unwrap().started.len(), 1);
                resumed.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn denied_physical_starts_do_not_occupy_work_sites() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.people[0].controlled = true;
    initial.people[0].seek_meals = false;
    initial.people[1].willing_to_work = true;
    initial.work_sites = Some(WorkSites {
        next_id: 0,
        sites: vec![WorkSite {
            id: 7,
            region: 1,
            kind: "workshop".into(),
            capacity: 1,
        }],
        requirements: vec![WorkSiteRequirement {
            recipe: 1,
            kind: "workshop".into(),
            slots: 1,
        }],
        bindings: vec![],
    });
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 2 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![Lot {
            good: 2,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut saw_denial = false;
    for seed in 1..=50 {
        initial.seed = seed;
        let mut world = Economy::compile(initial.clone()).unwrap();
        if world.transfer_stock(1, 2, 1, true).unwrap() {
            let state = world.snapshot();
            if state.physical.work.is_empty() {
                assert!(state.work_sites.as_ref().unwrap().bindings.is_empty());
                saw_denial = true;
                break;
            }
        }
    }
    assert!(saw_denial);
}

#[test]
fn multiple_work_sites_enforce_capacity_and_validate_saved_bindings() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.opinions = None;
    for person in &mut initial.people {
        person.seek_meals = false;
        person.willing_to_work = true;
    }
    initial.autowork.as_mut().unwrap().policies = (1..=4)
        .map(|person| WorkPolicy {
            person,
            recipe: 1,
            enabled: true,
            disregard_claims: false,
        })
        .collect();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 2 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![Lot {
            good: 2,
            quantity: 4,
            expires_at: None,
        }],
    )
    .unwrap();
    initial.work_sites = Some(WorkSites {
        next_id: 0,
        sites: vec![
            WorkSite {
                id: 8,
                region: 1,
                kind: "workshop".into(),
                capacity: 1,
            },
            WorkSite {
                id: 7,
                region: 1,
                kind: "workshop".into(),
                capacity: 2,
            },
        ],
        requirements: vec![WorkSiteRequirement {
            recipe: 1,
            kind: "workshop".into(),
            slots: 1,
        }],
        bindings: vec![],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert_eq!(world.advance().unwrap().started.len(), 3);
                let state = world.snapshot();
                let sites = state.work_sites.as_ref().unwrap();
                assert_eq!(sites.bindings.iter().filter(|b| b.site == 7).count(), 2);
                assert_eq!(sites.bindings.iter().filter(|b| b.site == 8).count(), 1);
                Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                let mut missing = state.clone();
                missing.work_sites.as_mut().unwrap().bindings.pop();
                assert!(Economy::compile(missing).is_err());
                let mut overcrowded = state.clone();
                overcrowded
                    .work_sites
                    .as_mut()
                    .unwrap()
                    .bindings
                    .iter_mut()
                    .find(|b| b.site == 8)
                    .unwrap()
                    .site = 7;
                assert!(Economy::compile(overcrowded).is_err());
                let mut incompatible = state.clone();
                incompatible.work_sites.as_mut().unwrap().sites[0].kind = "other".into();
                assert!(Economy::compile(incompatible).is_err());
                let draft = world.prepare().unwrap();
                world.set_work_choice(1, false).unwrap();
                let before = world.snapshot();
                assert!(world.commit(draft).is_err());
                assert_eq!(world.snapshot(), before);
                state
            })
    };
    assert_eq!(run(1), run(4));
}

fn concentrated_work_sites(count: u64) -> State {
    let mut state = concentrated(count);
    state.work_sites = Some(WorkSites {
        next_id: 0,
        sites: vec![WorkSite {
            id: 1,
            region: 1,
            kind: "workshop".into(),
            capacity: count / 8,
        }],
        requirements: vec![WorkSiteRequirement {
            recipe: 1,
            kind: "workshop".into(),
            slots: 1,
        }],
        bindings: vec![],
    });
    state
}

fn concentrated_site_refill(count: u64) -> State {
    let mut state = concentrated_work_sites(count);
    for person in &mut state.people {
        person.hunger = 0;
        person.seek_meals = false;
        person.willing_to_work = true;
    }
    let stock = state
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![Lot {
            good: 1,
            quantity: count,
            expires_at: Some(20),
        }],
    )
    .unwrap();
    state.physical.recipes.extend([
        Recipe {
            id: 4,
            name: "alternate material".into(),
            ticks: 3,
            inputs: vec![Amount {
                good: 1,
                quantity: 1,
            }],
            outputs: vec![Amount {
                good: 3,
                quantity: 1,
            }],
        },
        Recipe {
            id: 5,
            name: "open work".into(),
            ticks: 3,
            inputs: vec![Amount {
                good: 2,
                quantity: 1,
            }],
            outputs: vec![Amount {
                good: 3,
                quantity: 1,
            }],
        },
    ]);
    state
        .work_sites
        .as_mut()
        .unwrap()
        .requirements
        .push(WorkSiteRequirement {
            recipe: 4,
            kind: "workshop".into(),
            slots: 1,
        });
    state.autowork.as_mut().unwrap().policies = (1..=count)
        .map(|person| WorkPolicy {
            person,
            recipe: if person <= count / 4 {
                5
            } else if person % 2 == 0 {
                1
            } else {
                4
            },
            enabled: true,
            disregard_claims: false,
        })
        .collect();
    state
}

#[test]
fn concentrated_site_refills_are_physical_and_deterministic() {
    let count = 4096;
    let initial = concentrated_site_refill(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let events = world.advance().unwrap();
                assert!(
                    events.started.len() > (count / 4 + 3 * count / 32) as usize,
                    "only {} workers started",
                    events.started.len()
                );
                assert!(events.started.len() <= (count / 4 + count / 8) as usize);
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore = "performance measurement"]
fn concentrated_site_refill_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = concentrated_site_refill(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let start = std::time::Instant::now();
                let events = world.advance().unwrap();
                assert!(events.started.len() > (count / 4 + 3 * count / 32) as usize);
                if round > 0 {
                    samples.push(start.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

fn concentrated_claimed_work_site(count: u64) -> State {
    use game_parallel_economy_claims::{Claims, SiteClaim};
    let mut state = concentrated_work_sites(count);
    for policy in &mut state.autowork.as_mut().unwrap().policies {
        policy.disregard_claims = true;
    }
    state.dynamics = Some(Dynamics {
        personality: None,
        social: None,
        disease: None,
        treatment: vec![],
        claims: Some(Claims {
            inherit_births: false,
            site_claims: vec![SiteClaim { person: 1, site: 1 }],
            ..Default::default()
        }),
    });
    state
}

#[test]
fn concentrated_claimed_site_uses_remain_physical_and_worker_invariant() {
    let initial = concentrated_claimed_work_site(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert_eq!(world.advance().unwrap().started.len(), 512);
                let state = world.snapshot();
                assert_eq!(state.work_sites.as_ref().unwrap().bindings.len(), 512);
                assert_eq!(
                    state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .claims
                        .as_ref()
                        .unwrap()
                        .site_events
                        .len(),
                    512
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore = "performance measurement"]
fn concentrated_claimed_site_benchmark() {
    let workers: usize = std::env::var("LEGACY_WORKERS")
        .unwrap_or_else(|_| "1".into())
        .parse()
        .unwrap();
    let initial = concentrated_claimed_work_site(100_000);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut world = Economy::compile(initial).unwrap();
            let started = std::time::Instant::now();
            let events = world.advance().unwrap();
            eprintln!(
                "workers={workers} people=100000 ticks=1 elapsed={:.6}",
                started.elapsed().as_secs_f64()
            );
            assert_eq!(events.started.len(), 12_500);
        });
}

#[test]
fn concentrated_work_site_capacity_is_deterministic() {
    let count = 4096;
    let initial = concentrated_work_sites(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert_eq!(world.advance().unwrap().started.len(), count as usize / 8);
                let state = world.snapshot();
                assert_eq!(
                    state.work_sites.as_ref().unwrap().bindings.len(),
                    count as usize / 8
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_work_site_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let initial = concentrated_work_sites(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn a_full_site_does_not_let_blocked_work_monopolize_inputs() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.physical.recipes[0].ticks = 4;
    initial.physical.recipes.push(Recipe {
        id: 4,
        name: "open air work".into(),
        ticks: 1,
        inputs: vec![Amount {
            good: 2,
            quantity: 1,
        }],
        outputs: vec![Amount {
            good: 3,
            quantity: 1,
        }],
    });
    for person in &mut initial.people {
        person.seek_meals = false;
        person.willing_to_work = true;
    }
    initial.autowork.as_mut().unwrap().policies = vec![WorkPolicy {
        person: 2,
        recipe: 1,
        enabled: true,
        disregard_claims: false,
    }];
    initial.work_sites = Some(WorkSites {
        next_id: 0,
        sites: vec![WorkSite {
            id: 7,
            region: 1,
            kind: "workshop".into(),
            capacity: 1,
        }],
        requirements: vec![WorkSiteRequirement {
            recipe: 1,
            kind: "workshop".into(),
            slots: 1,
        }],
        bindings: vec![],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert_eq!(world.advance().unwrap().started.len(), 1);
                let mut state = world.snapshot();
                state.autowork.as_mut().unwrap().policies = vec![
                    WorkPolicy {
                        person: 1,
                        recipe: 1,
                        enabled: true,
                        disregard_claims: false,
                    },
                    WorkPolicy {
                        person: 3,
                        recipe: 4,
                        enabled: true,
                        disregard_claims: false,
                    },
                ];
                let mut world = Economy::compile(state).unwrap();
                assert_eq!(world.advance().unwrap().started.len(), 1);
                let state = world.snapshot();
                assert!(
                    state
                        .physical
                        .work
                        .iter()
                        .any(|w| w.job.worker == 3 && w.job.recipe == 4)
                );
                assert_eq!(state.work_sites.as_ref().unwrap().bindings.len(), 1);
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn a_worker_missing_inputs_does_not_monopolize_site_capacity() {
    let mut initial = claimed_work_sites_fixture();
    initial
        .work_sites
        .as_mut()
        .unwrap()
        .sites
        .retain(|site| site.id == 7);
    initial.work_sites.as_mut().unwrap().sites[0].capacity = 1;
    initial
        .work_sites
        .as_mut()
        .unwrap()
        .requirements
        .push(WorkSiteRequirement {
            recipe: 4,
            kind: "workshop".into(),
            slots: 1,
        });
    initial.physical.recipes.push(Recipe {
        id: 4,
        name: "missing materials".into(),
        ticks: 1,
        inputs: vec![Amount {
            good: 1,
            quantity: 1,
        }],
        outputs: vec![Amount {
            good: 3,
            quantity: 1,
        }],
    });
    initial.autowork.as_mut().unwrap().policies = vec![
        WorkPolicy {
            person: 1,
            recipe: 4,
            enabled: true,
            disregard_claims: false,
        },
        WorkPolicy {
            person: 2,
            recipe: 1,
            enabled: true,
            disregard_claims: false,
        },
    ];
    for seed in 1..=16 {
        initial.seed = seed;
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut world = Economy::compile(initial.clone()).unwrap();
                    assert_eq!(world.advance().unwrap().started.len(), 1);
                    let state = world.snapshot();
                    assert_eq!(state.physical.work[0].job.worker, 2);
                    state
                })
        };
        assert_eq!(run(1), run(4));
    }
}

#[test]
fn site_capacity_is_reconsidered_after_virtual_material_contention() {
    let mut initial = claimed_work_sites_fixture();
    let sites = initial.work_sites.as_mut().unwrap();
    sites.sites.retain(|site| site.id == 7);
    sites.sites[0].capacity = 1;
    sites.requirements.push(WorkSiteRequirement {
        recipe: 4,
        kind: "workshop".into(),
        slots: 1,
    });
    initial.physical.recipes.extend([
        Recipe {
            id: 4,
            name: "other material".into(),
            ticks: 3,
            inputs: vec![Amount {
                good: 1,
                quantity: 1,
            }],
            outputs: vec![Amount {
                good: 3,
                quantity: 1,
            }],
        },
        Recipe {
            id: 5,
            name: "work without site".into(),
            ticks: 3,
            inputs: vec![Amount {
                good: 2,
                quantity: 1,
            }],
            outputs: vec![Amount {
                good: 3,
                quantity: 1,
            }],
        },
    ]);
    for good in [1, 2] {
        let stock = initial
            .physical
            .custody
            .containers
            .iter_mut()
            .find(|c| c.place == Place::Stock { region: 1, good })
            .unwrap();
        *stock = Container::new(
            stock.place,
            stock.capacity,
            vec![Lot {
                good,
                quantity: 1,
                expires_at: (good == 1).then_some(20),
            }],
        )
        .unwrap();
    }
    initial.autowork.as_mut().unwrap().policies = [(1, 5), (2, 1), (3, 4)]
        .into_iter()
        .map(|(person, recipe)| WorkPolicy {
            person,
            recipe,
            enabled: true,
            disregard_claims: false,
        })
        .collect();
    for seed in 1..=32 {
        initial.seed = seed;
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut world = Economy::compile(initial.clone()).unwrap();
                    assert_eq!(world.advance().unwrap().started.len(), 2, "seed {seed}");
                    let state = world.snapshot();
                    let mut workers: Vec<_> = state
                        .physical
                        .work
                        .iter()
                        .map(|work| work.job.worker)
                        .collect();
                    workers.sort_unstable();
                    assert_eq!(workers, [1, 3], "seed {seed}");
                    state
                })
        };
        assert_eq!(run(1), run(4));
    }
}

#[test]
fn a_worker_who_dies_on_start_does_not_keep_a_site_binding() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.people[0].seek_meals = false;
    initial.people[1].health = 1;
    initial.physical.recipes[0].ticks = 3;
    initial.pending_food.push(FoodEffect {
        person: 2,
        at: 1,
        health: -1,
        nutrients: [0; 6],
    });
    initial.work_sites = Some(WorkSites {
        next_id: 0,
        sites: vec![WorkSite {
            id: 7,
            region: 1,
            kind: "workshop".into(),
            capacity: 1,
        }],
        requirements: vec![WorkSiteRequirement {
            recipe: 1,
            kind: "workshop".into(),
            slots: 1,
        }],
        bindings: vec![],
    });
    let mut world = Economy::compile(initial).unwrap();
    let events = world.advance().unwrap();
    assert_eq!(events.started.len(), 1);
    assert_eq!(events.interrupted.len(), 1);
    let state = world.snapshot();
    assert_eq!(state.people[1].health, 0);
    assert!(state.work_sites.as_ref().unwrap().bindings.is_empty());
    Economy::compile(state).unwrap();
}

#[test]
fn physical_work_sites_match_local_region_and_recipe_kind() {
    let mut initial = expedition_fixture();
    initial.agreements = None;
    initial.dynamics = None;
    for person in &mut initial.people {
        person.seek_meals = false;
        person.willing_to_work = true;
    }
    initial.people[2].region = Some(2);
    initial.people[3].region = Some(2);
    initial.physical.recipes.push(Recipe {
        id: 4,
        name: "mill".into(),
        ticks: 1,
        inputs: vec![Amount {
            good: 2,
            quantity: 1,
        }],
        outputs: vec![Amount {
            good: 3,
            quantity: 1,
        }],
    });
    initial.autowork = Some(Autowork {
        next_id: 1,
        requests: None,
        policies: vec![
            WorkPolicy {
                person: 1,
                recipe: 1,
                enabled: true,
                disregard_claims: false,
            },
            WorkPolicy {
                person: 2,
                recipe: 4,
                enabled: true,
                disregard_claims: false,
            },
            WorkPolicy {
                person: 3,
                recipe: 1,
                enabled: true,
                disregard_claims: false,
            },
            WorkPolicy {
                person: 4,
                recipe: 4,
                enabled: true,
                disregard_claims: false,
            },
        ],
    });
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 2, good: 2 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![Lot {
            good: 2,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    initial.work_sites = Some(WorkSites {
        next_id: 0,
        sites: vec![
            WorkSite {
                id: 1,
                region: 1,
                kind: "forge".into(),
                capacity: 1,
            },
            WorkSite {
                id: 2,
                region: 2,
                kind: "mill".into(),
                capacity: 1,
            },
        ],
        requirements: vec![
            WorkSiteRequirement {
                recipe: 1,
                kind: "forge".into(),
                slots: 1,
            },
            WorkSiteRequirement {
                recipe: 4,
                kind: "mill".into(),
                slots: 1,
            },
        ],
        bindings: vec![],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert_eq!(world.advance().unwrap().started.len(), 2);
                let state = world.snapshot();
                let mut workers: Vec<_> =
                    state.physical.work.iter().map(|w| w.job.worker).collect();
                workers.sort_unstable();
                assert_eq!(workers, vec![1, 4]);
                assert_eq!(state.work_sites.as_ref().unwrap().bindings.len(), 2);
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn a_multislot_job_can_use_a_larger_site_after_a_small_one() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.people[0].seek_meals = false;
    initial.work_sites = Some(WorkSites {
        next_id: 0,
        sites: vec![
            WorkSite {
                id: 1,
                region: 1,
                kind: "workshop".into(),
                capacity: 1,
            },
            WorkSite {
                id: 2,
                region: 1,
                kind: "workshop".into(),
                capacity: 2,
            },
        ],
        requirements: vec![WorkSiteRequirement {
            recipe: 1,
            kind: "workshop".into(),
            slots: 2,
        }],
        bindings: vec![],
    });
    let mut world = Economy::compile(initial).unwrap();
    assert_eq!(world.advance().unwrap().started.len(), 1);
    let state = world.snapshot();
    assert_eq!(state.work_sites.as_ref().unwrap().bindings[0].site, 2);
    assert_eq!(state.work_sites.as_ref().unwrap().bindings[0].slots, 2);
    Economy::compile(state).unwrap();
}

fn construction_fixture() -> State {
    let mut state = fixture();
    state.agreements = None;
    for person in &mut state.people {
        person.seek_meals = false;
    }
    state.work_sites = Some(WorkSites {
        next_id: 0,
        sites: vec![],
        requirements: vec![WorkSiteRequirement {
            recipe: 1,
            kind: "workshop".into(),
            slots: 1,
        }],
        bindings: vec![],
    });
    state.construction = Some(Construction {
        materials: vec![],
        salvaging: vec![],
        salvage_events: vec![],
        blueprints: vec![BuildingBlueprint {
            salvage: 0,
            id: 1,
            name: "workshop kit".into(),
            kind: "workshop".into(),
            capacity: 1,
            kit: 3,
            quantity: 1,
        }],
        policies: vec![ConstructionPolicy {
            person: 2,
            blueprint: 1,
            target_sites: 1,
            willing: true,
            pressured_by: None,
            pressure_threshold: None,
            disregard_claims: false,
        }],
        events: vec![],
        demolition: vec![],
        demolition_events: vec![],
        goal_builders: vec![],
    });
    let stock = state
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 3 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![Lot {
            good: 3,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    state
}

fn claimed_work_sites_fixture() -> State {
    use game_parallel_economy_claims::Claims;
    let mut state = fixture();
    state.agreements = None;
    state.physical.recipes[0].ticks = 3;
    for person in &mut state.people {
        person.seek_meals = false;
        person.willing_to_work = true;
        person.hunger = 0;
    }
    let stock = state
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 2 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![Lot {
            good: 2,
            quantity: 3,
            expires_at: None,
        }],
    )
    .unwrap();
    state.autowork.as_mut().unwrap().policies = [1, 2, 3]
        .into_iter()
        .map(|person| WorkPolicy {
            person,
            recipe: 1,
            enabled: true,
            disregard_claims: person == 3,
        })
        .collect();
    state.work_sites = Some(WorkSites {
        next_id: 0,
        sites: vec![
            WorkSite {
                id: 7,
                region: 1,
                kind: "workshop".into(),
                capacity: 2,
            },
            WorkSite {
                id: 8,
                region: 1,
                kind: "workshop".into(),
                capacity: 1,
            },
        ],
        requirements: vec![WorkSiteRequirement {
            recipe: 1,
            kind: "workshop".into(),
            slots: 1,
        }],
        bindings: vec![],
    });
    state.dynamics = Some(Dynamics {
        personality: None,
        social: None,
        disease: None,
        treatment: vec![],
        claims: Some(Claims::default()),
    });
    state
}

#[test]
fn site_assertions_shape_individual_choices_without_granting_authority() {
    let initial = claimed_work_sites_fixture();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let stale = world.prepare().unwrap();
                world.claim_site(1, 7).unwrap();
                assert!(world.commit(stale).is_err());
                assert!(world.claim_site(1, 7).is_err());
                assert_eq!(world.advance().unwrap().started.len(), 3);
                let state = world.snapshot();
                let bindings = &state.work_sites.as_ref().unwrap().bindings;
                let site_for = |worker| {
                    let job = state
                        .physical
                        .work
                        .iter()
                        .find(|w| w.job.worker == worker)
                        .unwrap()
                        .job
                        .id;
                    bindings
                        .iter()
                        .find(|binding| binding.job == job)
                        .unwrap()
                        .site
                };
                assert_eq!((site_for(1), site_for(2), site_for(3)), (7, 8, 7));
                let observations = &state
                    .dynamics
                    .as_ref()
                    .unwrap()
                    .claims
                    .as_ref()
                    .unwrap()
                    .site_events;
                assert_eq!(observations.len(), 1);
                assert_eq!(
                    (
                        observations[0].claimant,
                        observations[0].person,
                        observations[0].site,
                        observations[0].destroyed
                    ),
                    (1, 3, 7, false)
                );
                Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn conflicting_site_claims_do_not_imply_permission_from_other_claimants() {
    let initial = claimed_work_sites_fixture();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.claim_site(1, 7).unwrap();
                world.claim_site(4, 7).unwrap();
                assert_eq!(world.advance().unwrap().started.len(), 2);
                let state = world.snapshot();
                let site7 = state
                    .work_sites
                    .as_ref()
                    .unwrap()
                    .bindings
                    .iter()
                    .find(|b| b.site == 7)
                    .unwrap();
                let worker = state
                    .physical
                    .work
                    .iter()
                    .find(|w| w.job.id == site7.job)
                    .unwrap()
                    .job
                    .worker;
                assert_eq!(worker, 3);
                assert_eq!(
                    state
                        .work_sites
                        .as_ref()
                        .unwrap()
                        .bindings
                        .iter()
                        .filter(|b| b.site == 8)
                        .count(),
                    1
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn witnessed_site_use_changes_affinity_only_as_each_person_interprets_it() {
    let mut initial = claimed_work_sites_fixture();
    let opinions = initial.opinions.as_mut().unwrap();
    opinions.rules.affinity = Some(game_parallel_opinions::AffinityRules {
        limit: 100,
        encounter_change: 0,
        pressured_encounter_change: 0,
        help_change: 0,
        harm_change_per_damage: 0,
    });
    let sensitivity = |person, rate| OpinionSensitivity {
        person,
        report_reception: None,
        affinity_report_discrepancy: None,
        trust_report_discrepancy: None,
        trust_promise_fulfilled: None,
        trust_promise_unfulfilled: None,
        fear_per_damage: None,
        gratitude_gain: None,
        promise_obligation_gain: None,
        obligation_relief: None,
        theft_loss: None,
        confrontation: None,
        affinity_help: None,
        affinity_harm_per_damage: None,
        affinity_encounter: None,
        affinity_pressured_encounter: None,
        affinity_site_use: Some(rate),
        affinity_site_destruction: None,
        affinity_site_salvage: None,
        affinity_work_success: None,
        affinity_work_refusal: None,
        relationship_cases: vec![],
        obligation_development: None,
    };
    let mut first = sensitivity(1, 4);
    first.relationship_cases.push(RelationshipCase {
        target: 3,
        report_discrepancy: None,
        report_discrepancy_trust: None,
        promise_fulfilled_trust: None,
        promise_unfulfilled_trust: None,
        help: None,
        harm_per_damage: None,
        encounter: None,
        pressured_encounter: None,
        site_use: Some(6),
        site_destruction: None,
        site_salvage: None,
        work_success: None,
        work_refusal: None,
        obligation_gain: None,
        promise_obligation_gain: None,
        obligation_relief: None,
    });
    opinions.sensitivities = vec![first, sensitivity(4, -3)].into();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.claim_site(1, 7).unwrap();
                world.claim_site(4, 7).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let opinions = state.opinions.as_ref().unwrap().index();
                assert_eq!(opinions.affinity(1, 3), 6);
                assert_eq!(opinions.affinity(4, 3), -3);
                assert_eq!(opinions.obligation(1, 3), 0);
                assert_eq!(opinions.obligation(4, 3), 0);
                let mut old = serde_json::to_value(&state).unwrap();
                for sensitivity in old["opinions"]["sensitivities"].as_array_mut().unwrap() {
                    let row = sensitivity.as_object_mut().unwrap();
                    row.remove("affinity_site_use");
                    row.remove("affinity_site_destruction");
                    for case in row["relationship_cases"].as_array_mut().unwrap() {
                        case.as_object_mut().unwrap().remove("site_use");
                        case.as_object_mut().unwrap().remove("site_destruction");
                    }
                }
                Economy::compile(serde_json::from_value(old).unwrap()).unwrap();
                Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn a_remote_claimant_does_not_observe_site_use() {
    let mut world = Economy::compile(claimed_work_sites_fixture()).unwrap();
    world.claim_site(1, 7).unwrap();
    let mut state = world.snapshot();
    state.people[0].region = Some(2);
    state
        .physical
        .custody
        .containers
        .push(Container::new(Place::Stock { region: 2, good: 2 }, 10, vec![]).unwrap());
    state
        .physical
        .custody
        .containers
        .sort_by_key(|container| container.place);
    let mut world = Economy::compile(state).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    let claims = state.dynamics.as_ref().unwrap().claims.as_ref().unwrap();
    assert!(claims.site_events.is_empty());
    assert!(
        state
            .work_sites
            .as_ref()
            .unwrap()
            .bindings
            .iter()
            .any(|b| b.site == 7)
    );
}

#[test]
fn a_controlled_person_can_use_another_persons_claimed_site() {
    let mut initial = claimed_work_sites_fixture();
    initial.people[0].controlled = true;
    initial.autowork.as_mut().unwrap().policies.clear();
    initial
        .work_sites
        .as_mut()
        .unwrap()
        .sites
        .retain(|site| site.id == 7);
    let mut world = Economy::compile(initial).unwrap();
    world.claim_site(4, 7).unwrap();
    assert_eq!(world.start_work(1, 1, 1).unwrap().started.len(), 1);
    assert_eq!(
        world.snapshot().work_sites.as_ref().unwrap().bindings[0].site,
        7
    );
}

#[test]
fn site_claims_influence_goal_building_without_forcing_compliance() {
    use game_parallel_goals::Goal;
    let mut initial = construction_fixture();
    initial.work_sites.as_mut().unwrap().sites.push(WorkSite {
        id: 7,
        region: 1,
        kind: "workshop".into(),
        capacity: 1,
    });
    initial.autowork.as_mut().unwrap().policies.clear();
    initial.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: true,
        events: vec![],
        people: vec![Goal {
            person: 2,
            recipe: 1,
            priority: 1,
            target: 1,
            progress: 0,
            next_review: 0,
            enabled: true,
            recognize_help: false,
        }],
    });
    let construction = initial.construction.as_mut().unwrap();
    construction.policies.clear();
    construction.goal_builders = vec![GoalBuilder {
        person: 2,
        enabled: true,
        disregard_claims: false,
    }];
    initial.dynamics = Some(Dynamics {
        personality: None,
        social: None,
        disease: None,
        treatment: vec![],
        claims: Some(Default::default()),
    });
    let mut respecting = Economy::compile(initial.clone()).unwrap();
    respecting.claim_site(1, 7).unwrap();
    respecting.advance().unwrap();
    assert_eq!(
        respecting
            .snapshot()
            .work_sites
            .as_ref()
            .unwrap()
            .sites
            .len(),
        2
    );
    assert_eq!(
        respecting.snapshot().construction.as_ref().unwrap().events[0].person,
        2
    );
    assert_eq!(respecting.advance().unwrap().started.len(), 1);
    assert_ne!(
        respecting.snapshot().work_sites.as_ref().unwrap().bindings[0].site,
        7
    );

    let mut disregarding = Economy::compile(initial).unwrap();
    disregarding.claim_site(1, 7).unwrap();
    disregarding.set_claim_choice(2, true).unwrap();
    assert_eq!(disregarding.advance().unwrap().started.len(), 1);
    let state = disregarding.snapshot();
    assert_eq!(state.work_sites.as_ref().unwrap().sites.len(), 1);
    assert_eq!(state.work_sites.as_ref().unwrap().bindings[0].site, 7);
}

#[test]
fn demolition_respects_personal_site_claims_unless_the_actor_disregards_them() {
    let mut initial = claimed_work_sites_fixture();
    initial.autowork = None;
    initial.construction = Some(Construction {
        materials: vec![],
        salvaging: vec![],
        salvage_events: vec![],
        blueprints: vec![],
        policies: vec![],
        events: vec![],
        goal_builders: vec![],
        demolition_events: vec![],
        demolition: vec![DemolitionPolicy {
            person: 2,
            site: 7,
            willing: true,
            pressured_by: None,
            pressure_threshold: None,
            disregard_claims: false,
        }],
    });
    let mut world = Economy::compile(initial).unwrap();
    world.claim_site(1, 7).unwrap();
    world.advance().unwrap();
    assert!(
        world
            .snapshot()
            .work_sites
            .as_ref()
            .unwrap()
            .sites
            .iter()
            .any(|site| site.id == 7)
    );
    world.set_claim_choice(2, true).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    assert!(
        state
            .work_sites
            .as_ref()
            .unwrap()
            .sites
            .iter()
            .all(|site| site.id != 7)
    );
    assert!(state.construction.as_ref().unwrap().demolition_events[0].destroyed);
    let observations = &state
        .dynamics
        .as_ref()
        .unwrap()
        .claims
        .as_ref()
        .unwrap()
        .site_events;
    assert_eq!(
        (
            observations[0].claimant,
            observations[0].person,
            observations[0].site,
            observations[0].destroyed
        ),
        (1, 2, 7, true)
    );
    let mut restored =
        Economy::compile(serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap())
            .unwrap();
    restored.withdraw_site_claim(1, 7).unwrap();
    assert!(
        restored
            .snapshot()
            .dynamics
            .unwrap()
            .claims
            .unwrap()
            .site_claims
            .is_empty()
    );
}

#[test]
fn a_person_can_enjoy_witnessing_their_claimed_site_destroyed() {
    let mut initial = claimed_work_sites_fixture();
    initial.autowork = None;
    initial.construction = Some(Construction {
        materials: vec![],
        salvaging: vec![],
        salvage_events: vec![],
        blueprints: vec![],
        policies: vec![],
        events: vec![],
        goal_builders: vec![],
        demolition_events: vec![],
        demolition: vec![DemolitionPolicy {
            person: 2,
            site: 7,
            willing: true,
            pressured_by: None,
            pressure_threshold: None,
            disregard_claims: true,
        }],
    });
    let opinions = initial.opinions.as_mut().unwrap();
    opinions.rules.affinity = Some(game_parallel_opinions::AffinityRules {
        limit: 100,
        encounter_change: 0,
        pressured_encounter_change: 0,
        help_change: 0,
        harm_change_per_damage: 0,
    });
    opinions.sensitivities = vec![OpinionSensitivity {
        person: 1,
        report_reception: None,
        affinity_report_discrepancy: None,
        trust_report_discrepancy: None,
        trust_promise_fulfilled: None,
        trust_promise_unfulfilled: None,
        fear_per_damage: None,
        gratitude_gain: Some(0),
        promise_obligation_gain: Some(0),
        obligation_relief: None,
        theft_loss: None,
        confrontation: None,
        affinity_help: None,
        affinity_harm_per_damage: None,
        affinity_encounter: None,
        affinity_pressured_encounter: None,
        affinity_site_use: None,
        affinity_site_destruction: Some(5),
        affinity_site_salvage: None,
        affinity_work_success: None,
        affinity_work_refusal: None,
        relationship_cases: vec![],
        obligation_development: None,
    }]
    .into();
    let mut world = Economy::compile(initial).unwrap();
    world.claim_site(1, 7).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    assert_eq!(state.opinions.as_ref().unwrap().index().affinity(1, 2), 5);
    assert_eq!(state.opinions.as_ref().unwrap().index().obligation(1, 2), 0);
    assert!(
        state
            .work_sites
            .as_ref()
            .unwrap()
            .sites
            .iter()
            .all(|site| site.id != 7)
    );
}

#[test]
fn a_person_builds_a_real_site_from_a_kit_before_work_can_use_it() {
    let initial = construction_fixture();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.advance().unwrap().started.is_empty());
                let built = world.snapshot();
                let sites = built.work_sites.as_ref().unwrap();
                assert_eq!(
                    (sites.sites.len(), sites.sites[0].id, sites.next_id),
                    (1, 1, 2)
                );
                assert_eq!(built.construction.as_ref().unwrap().events[0].site, Some(1));
                assert_eq!(
                    built
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Stock { region: 1, good: 3 })
                        .unwrap()
                        .used,
                    0
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&built).unwrap()).unwrap(),
                )
                .unwrap();
                assert_eq!(world.advance().unwrap().started.len(), 1);
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                assert_eq!(
                    world.snapshot().work_sites.as_ref().unwrap().bindings[0].site,
                    1
                );
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn builders_can_choose_to_claim_only_sites_they_actually_build() {
    let mut initial = construction_fixture();
    initial.autowork = None;
    initial.dynamics = Some(Dynamics {
        personality: None,
        social: None,
        disease: None,
        treatment: vec![],
        claims: Some(Default::default()),
    });
    let template = initial.construction.as_ref().unwrap().policies[0].clone();
    initial
        .construction
        .as_mut()
        .unwrap()
        .policies
        .push(ConstructionPolicy {
            person: 3,
            ..template
        });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.set_built_site_claim_choice(2, true).unwrap();
                world.set_built_site_claim_choice(3, true).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let events = &state.construction.as_ref().unwrap().events;
                assert_eq!(events.len(), 2);
                let built = events.iter().find(|event| event.site.is_some()).unwrap();
                let claims = &state
                    .dynamics
                    .as_ref()
                    .unwrap()
                    .claims
                    .as_ref()
                    .unwrap()
                    .site_claims;
                assert_eq!(claims.len(), 1);
                assert_eq!(
                    (claims[0].person, Some(claims[0].site)),
                    (built.person, built.site)
                );
                let mut old = serde_json::to_value(&state).unwrap();
                for preference in old["dynamics"]["claims"]["preferences"]
                    .as_array_mut()
                    .unwrap()
                {
                    preference
                        .as_object_mut()
                        .unwrap()
                        .remove("assert_built_sites");
                }
                Economy::compile(serde_json::from_value(old).unwrap()).unwrap();
                Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                state
            })
    };
    assert_eq!(run(1), run(4));

    let mut refusing = Economy::compile(initial).unwrap();
    refusing.set_built_site_claim_choice(2, true).unwrap();
    refusing.set_built_site_claim_choice(2, false).unwrap();
    refusing.advance().unwrap();
    assert!(
        refusing
            .snapshot()
            .dynamics
            .unwrap()
            .claims
            .unwrap()
            .site_claims
            .is_empty()
    );
}

#[test]
fn a_builder_who_dies_cannot_assert_a_new_claim_after_death() {
    let mut initial = construction_fixture();
    initial.autowork = None;
    initial.rules.starvation_threshold = 1;
    initial.rules.starvation_damage = 1;
    initial.people[1].health = 1;
    initial.dynamics = Some(Dynamics {
        personality: None,
        social: None,
        disease: None,
        treatment: vec![],
        claims: Some(Default::default()),
    });
    let mut world = Economy::compile(initial).unwrap();
    world.set_built_site_claim_choice(2, true).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    assert_eq!(state.people[1].health, 0);
    assert_eq!(state.work_sites.as_ref().unwrap().sites.len(), 1);
    assert!(
        state
            .dynamics
            .unwrap()
            .claims
            .unwrap()
            .site_claims
            .is_empty()
    );
}

#[test]
fn a_person_builds_a_needed_site_for_their_own_work_goal() {
    use game_parallel_goals::Goal;
    let mut initial = construction_fixture();
    initial.autowork.as_mut().unwrap().policies.clear();
    initial.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: false,
        events: vec![],
        people: vec![Goal {
            person: 2,
            recipe: 1,
            priority: 1,
            target: 1,
            progress: 0,
            next_review: 0,
            enabled: true,
            recognize_help: false,
        }],
    });
    let construction = initial.construction.as_mut().unwrap();
    construction.policies.clear();
    construction.goal_builders = vec![GoalBuilder {
        person: 2,
        enabled: true,
        disregard_claims: false,
    }];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.advance().unwrap().started.is_empty());
                let built = world.snapshot();
                assert_eq!(built.work_sites.as_ref().unwrap().sites.len(), 1);
                assert_eq!(built.construction.as_ref().unwrap().events[0].person, 2);
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&built).unwrap()).unwrap(),
                )
                .unwrap();
                assert_eq!(world.advance().unwrap().started.len(), 1);
                restored.advance().unwrap();
                assert_eq!(world.advance().unwrap().finished.len(), 1);
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(state.goals.as_ref().unwrap().people[0].progress, 1);
                state
            })
    };
    assert_eq!(run(1), run(4));

    let mut refusing = Economy::compile(initial).unwrap();
    refusing.set_goal_building_choice(2, false).unwrap();
    refusing.advance().unwrap();
    assert!(
        refusing
            .snapshot()
            .work_sites
            .as_ref()
            .unwrap()
            .sites
            .is_empty()
    );
}

#[test]
fn goal_builders_choose_a_sufficient_blueprint_with_an_available_kit() {
    use game_parallel_goals::Goal;
    let mut initial = construction_fixture();
    initial.autowork.as_mut().unwrap().policies.clear();
    initial.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: false,
        events: vec![],
        people: vec![Goal {
            person: 2,
            recipe: 1,
            priority: 1,
            target: 1,
            progress: 0,
            next_review: 0,
            enabled: true,
            recognize_help: false,
        }],
    });
    initial.work_sites.as_mut().unwrap().requirements[0].slots = 2;
    let construction = initial.construction.as_mut().unwrap();
    construction.policies.clear();
    construction.goal_builders = vec![GoalBuilder {
        person: 2,
        enabled: true,
        disregard_claims: false,
    }];
    construction.blueprints.push(BuildingBlueprint {
        salvage: 0,
        id: 2,
        name: "unavailable".into(),
        kind: "workshop".into(),
        capacity: 2,
        kit: 1,
        quantity: 1,
    });
    construction.blueprints.push(BuildingBlueprint {
        salvage: 0,
        id: 3,
        name: "usable".into(),
        kind: "workshop".into(),
        capacity: 2,
        kit: 3,
        quantity: 1,
    });
    let mut world = Economy::compile(initial).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    assert_eq!(state.construction.as_ref().unwrap().events[0].blueprint, 3);
    assert_eq!(state.work_sites.as_ref().unwrap().sites[0].capacity, 2);
}

#[test]
fn a_person_plans_a_tool_and_kit_before_building_for_their_work_goal() {
    use game_parallel_goals::Goal;
    let mut initial = construction_fixture();
    initial.autowork.as_mut().unwrap().policies.clear();
    initial.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: true,
        events: vec![],
        people: vec![Goal {
            person: 2,
            recipe: 1,
            priority: 1,
            target: 1,
            progress: 0,
            next_review: 0,
            enabled: true,
            recognize_help: false,
        }],
    });
    initial.physical.recipes.push(Recipe {
        id: 4,
        name: "make building kit".into(),
        ticks: 1,
        inputs: vec![Amount {
            good: 2,
            quantity: 1,
        }],
        outputs: vec![Amount {
            good: 3,
            quantity: 1,
        }],
    });
    initial.work_tools = vec![WorkTool {
        recipe: 4,
        good: 1,
        quantity: 1,
    }];
    let kit = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 3 })
        .unwrap();
    *kit = Container::new(kit.place, kit.capacity, vec![]).unwrap();
    let tools = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *tools = Container::new(
        tools.place,
        tools.capacity,
        vec![Lot {
            good: 1,
            quantity: 1,
            expires_at: Some(20),
        }],
    )
    .unwrap();
    let construction = initial.construction.as_mut().unwrap();
    construction.policies.clear();
    construction.goal_builders = vec![GoalBuilder {
        person: 2,
        enabled: true,
        disregard_claims: false,
    }];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.advance().unwrap().started.is_empty());
                assert_eq!(
                    world
                        .snapshot()
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Person(2))
                        .unwrap()
                        .used,
                    1
                );
                assert_eq!(world.advance().unwrap().started.len(), 1);
                assert_eq!(world.snapshot().physical.work[0].job.recipe, 4);
                assert_eq!(world.advance().unwrap().finished.len(), 1);
                assert_eq!(
                    world.snapshot().goals.as_ref().unwrap().people[0].progress,
                    0
                );
                let saved = world.snapshot();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                for _ in 0..4 {
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(state.work_sites.as_ref().unwrap().sites.len(), 1);
                assert_eq!(state.goals.as_ref().unwrap().people[0].progress, 1);
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn a_person_builds_an_intermediate_site_to_make_their_goal_site_kit() {
    use game_parallel_goals::Goal;
    let mut initial = construction_fixture();
    initial.autowork.as_mut().unwrap().policies.clear();
    initial.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: true,
        events: vec![],
        people: vec![Goal {
            person: 2,
            recipe: 1,
            priority: 1,
            target: 1,
            progress: 0,
            next_review: 0,
            enabled: true,
            recognize_help: false,
        }],
    });
    initial.physical.recipes.push(Recipe {
        id: 4,
        name: "make workshop kit".into(),
        ticks: 1,
        inputs: vec![Amount {
            good: 2,
            quantity: 1,
        }],
        outputs: vec![Amount {
            good: 3,
            quantity: 1,
        }],
    });
    let materials = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 2 })
        .unwrap();
    *materials = Container::new(
        materials.place,
        materials.capacity,
        vec![Lot {
            good: 2,
            quantity: 3,
            expires_at: None,
        }],
    )
    .unwrap();
    let kit = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 3 })
        .unwrap();
    *kit = Container::new(kit.place, kit.capacity, vec![]).unwrap();
    initial
        .work_sites
        .as_mut()
        .unwrap()
        .requirements
        .push(WorkSiteRequirement {
            recipe: 4,
            kind: "forge".into(),
            slots: 1,
        });
    let construction = initial.construction.as_mut().unwrap();
    construction.policies.clear();
    construction.goal_builders = vec![GoalBuilder {
        person: 2,
        enabled: true,
        disregard_claims: false,
    }];
    construction.blueprints.push(BuildingBlueprint {
        salvage: 0,
        id: 2,
        name: "forge kit".into(),
        kind: "forge".into(),
        capacity: 1,
        kit: 2,
        quantity: 1,
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let saved = world.snapshot();
                assert_eq!(saved.work_sites.as_ref().unwrap().sites[0].kind, "forge");
                assert_eq!(saved.goals.as_ref().unwrap().people[0].progress, 0);
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                for _ in 0..6 {
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert!(
                    state
                        .work_sites
                        .as_ref()
                        .unwrap()
                        .sites
                        .iter()
                        .any(|site| site.kind == "workshop")
                );
                assert_eq!(state.goals.as_ref().unwrap().people[0].progress, 1);
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn mutually_dependent_sites_do_not_create_kits_or_loop() {
    use game_parallel_goals::Goal;
    let mut initial = construction_fixture();
    initial.autowork.as_mut().unwrap().policies.clear();
    initial.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: true,
        events: vec![],
        people: vec![Goal {
            person: 2,
            recipe: 1,
            priority: 1,
            target: 1,
            progress: 0,
            next_review: 0,
            enabled: true,
            recognize_help: false,
        }],
    });
    for good in [2, 3] {
        let stock = initial
            .physical
            .custody
            .containers
            .iter_mut()
            .find(|c| c.place == Place::Stock { region: 1, good })
            .unwrap();
        *stock = Container::new(stock.place, stock.capacity, vec![]).unwrap();
    }
    initial.physical.recipes.push(Recipe {
        id: 4,
        name: "workshop kit".into(),
        ticks: 1,
        inputs: vec![Amount {
            good: 1,
            quantity: 1,
        }],
        outputs: vec![Amount {
            good: 3,
            quantity: 1,
        }],
    });
    initial.physical.recipes.push(Recipe {
        id: 5,
        name: "forge kit".into(),
        ticks: 1,
        inputs: vec![Amount {
            good: 1,
            quantity: 1,
        }],
        outputs: vec![Amount {
            good: 2,
            quantity: 1,
        }],
    });
    initial.work_sites.as_mut().unwrap().requirements.extend([
        WorkSiteRequirement {
            recipe: 4,
            kind: "forge".into(),
            slots: 1,
        },
        WorkSiteRequirement {
            recipe: 5,
            kind: "workshop".into(),
            slots: 1,
        },
    ]);
    let construction = initial.construction.as_mut().unwrap();
    construction.policies.clear();
    construction.goal_builders = vec![GoalBuilder {
        person: 2,
        enabled: true,
        disregard_claims: false,
    }];
    construction.blueprints.push(BuildingBlueprint {
        salvage: 0,
        id: 2,
        name: "forge kit".into(),
        kind: "forge".into(),
        capacity: 1,
        kit: 2,
        quantity: 1,
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                for _ in 0..4 {
                    assert!(world.advance().unwrap().started.is_empty());
                }
                let state = world.snapshot();
                assert!(state.work_sites.as_ref().unwrap().sites.is_empty());
                assert!(state.physical.work.is_empty());
                assert_eq!(state.goals.as_ref().unwrap().people[0].progress, 0);
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn a_goal_plan_can_build_two_layers_of_intermediate_sites() {
    use game_parallel_goals::Goal;
    let mut initial = construction_fixture();
    initial.rules.hunger_per_tick = 0;
    initial.autowork.as_mut().unwrap().policies.clear();
    initial.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: true,
        events: vec![],
        people: vec![Goal {
            person: 2,
            recipe: 1,
            priority: 1,
            target: 1,
            progress: 0,
            next_review: 0,
            enabled: true,
            recognize_help: false,
        }],
    });
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 2 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![Lot {
            good: 2,
            quantity: 4,
            expires_at: None,
        }],
    )
    .unwrap();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 3 })
        .unwrap();
    *stock = Container::new(stock.place, stock.capacity, vec![]).unwrap();
    initial.physical.recipes.extend([
        Recipe {
            id: 4,
            name: "workshop kit".into(),
            ticks: 1,
            inputs: vec![Amount {
                good: 2,
                quantity: 1,
            }],
            outputs: vec![Amount {
                good: 3,
                quantity: 1,
            }],
        },
        Recipe {
            id: 5,
            name: "forge kit".into(),
            ticks: 1,
            inputs: vec![Amount {
                good: 2,
                quantity: 1,
            }],
            outputs: vec![Amount {
                good: 1,
                quantity: 1,
            }],
        },
    ]);
    initial.work_sites.as_mut().unwrap().requirements.extend([
        WorkSiteRequirement {
            recipe: 4,
            kind: "forge".into(),
            slots: 1,
        },
        WorkSiteRequirement {
            recipe: 5,
            kind: "kiln".into(),
            slots: 1,
        },
    ]);
    let construction = initial.construction.as_mut().unwrap();
    construction.policies.clear();
    construction.goal_builders = vec![GoalBuilder {
        person: 2,
        enabled: true,
        disregard_claims: false,
    }];
    construction.blueprints.extend([
        BuildingBlueprint {
            salvage: 0,
            id: 2,
            name: "forge kit".into(),
            kind: "forge".into(),
            capacity: 1,
            kit: 1,
            quantity: 1,
        },
        BuildingBlueprint {
            salvage: 0,
            id: 3,
            name: "kiln kit".into(),
            kind: "kiln".into(),
            capacity: 1,
            kit: 2,
            quantity: 1,
        },
    ]);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                assert_eq!(
                    world.snapshot().work_sites.as_ref().unwrap().sites[0].kind,
                    "kiln"
                );
                let saved = world.snapshot();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                for _ in 0..10 {
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(state.work_sites.as_ref().unwrap().sites.len(), 3);
                assert_eq!(state.goals.as_ref().unwrap().people[0].progress, 1);
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn repeated_goal_reviews_eventually_try_later_kit_recipes() {
    use game_parallel_goals::Goal;
    let mut initial = construction_fixture();
    initial.rules.hunger_per_tick = 0;
    initial.autowork.as_mut().unwrap().policies.clear();
    initial.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: true,
        events: vec![],
        people: vec![Goal {
            person: 2,
            recipe: 1,
            priority: 1,
            target: 1,
            progress: 0,
            next_review: 0,
            enabled: true,
            recognize_help: false,
        }],
    });
    let kit = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 3 })
        .unwrap();
    *kit = Container::new(kit.place, kit.capacity, vec![]).unwrap();
    initial.physical.recipes.extend((4..=70).map(|id| Recipe {
        id,
        name: format!("kit alternative {id}"),
        ticks: 1,
        inputs: vec![Amount {
            good: if id == 70 { 2 } else { 1 },
            quantity: 1,
        }],
        outputs: vec![Amount {
            good: 3,
            quantity: 1,
        }],
    }));
    let construction = initial.construction.as_mut().unwrap();
    construction.policies.clear();
    construction.goal_builders = vec![GoalBuilder {
        person: 2,
        enabled: true,
        disregard_claims: false,
    }];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                assert!(world.snapshot().physical.work.is_empty());
                let saved = world.snapshot();
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                for _ in 0..30 {
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(state.goals.as_ref().unwrap().people[0].progress, 1);
                assert_eq!(state.work_sites.as_ref().unwrap().sites.len(), 1);
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn goal_builders_do_not_build_when_a_suitable_site_already_exists() {
    use game_parallel_goals::Goal;
    let mut initial = construction_fixture();
    initial.autowork.as_mut().unwrap().policies.clear();
    initial.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: false,
        events: vec![],
        people: vec![Goal {
            person: 2,
            recipe: 1,
            priority: 1,
            target: 1,
            progress: 0,
            next_review: 0,
            enabled: true,
            recognize_help: false,
        }],
    });
    initial.work_sites.as_mut().unwrap().sites.push(WorkSite {
        id: 7,
        region: 1,
        kind: "workshop".into(),
        capacity: 1,
    });
    let construction = initial.construction.as_mut().unwrap();
    construction.policies.clear();
    construction.goal_builders = vec![GoalBuilder {
        person: 2,
        enabled: true,
        disregard_claims: false,
    }];
    let mut world = Economy::compile(initial).unwrap();
    assert_eq!(world.advance().unwrap().started.len(), 1);
    let state = world.snapshot();
    assert_eq!(state.work_sites.as_ref().unwrap().sites.len(), 1);
    assert!(state.construction.as_ref().unwrap().events.is_empty());
    assert_eq!(
        state
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Stock { region: 1, good: 3 })
            .unwrap()
            .used,
        1
    );
}

#[test]
fn construction_remains_a_personal_choice_and_may_be_motivated_by_fear() {
    let mut initial = construction_fixture();
    initial.autowork = None;
    initial.construction.as_mut().unwrap().policies[0].willing = false;
    let mut revoked = initial.clone();
    revoked.construction.as_mut().unwrap().policies[0].willing = true;
    let mut revoked = Economy::compile(revoked).unwrap();
    revoked.set_construction_choice(2, false).unwrap();
    revoked.advance().unwrap();
    assert!(
        revoked
            .snapshot()
            .work_sites
            .as_ref()
            .unwrap()
            .sites
            .is_empty()
    );
    let mut refusing = Economy::compile(initial.clone()).unwrap();
    refusing.advance().unwrap();
    assert!(
        refusing
            .snapshot()
            .work_sites
            .as_ref()
            .unwrap()
            .sites
            .is_empty()
    );
    assert_eq!(
        refusing
            .snapshot()
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Stock { region: 1, good: 3 })
            .unwrap()
            .used,
        1
    );
    initial.construction.as_mut().unwrap().policies[0].pressured_by = Some(1);
    initial.construction.as_mut().unwrap().policies[0].pressure_threshold = Some(3);
    initial.opinions.as_mut().unwrap().records = vec![Opinion {
        observer: 2,
        target: 1,
        trust: 0,
        fear: 4,
        obligation: 0,
        affinity: 0,
    }];
    let mut pressured = Economy::compile(initial).unwrap();
    pressured.advance().unwrap();
    let state = pressured.snapshot();
    assert!(!state.construction.as_ref().unwrap().policies[0].willing);
    assert_eq!(state.opinions.as_ref().unwrap().index().obligation(2, 1), 0);
    assert_eq!(state.work_sites.as_ref().unwrap().sites.len(), 1);
}

#[test]
fn controlled_construction_consumes_a_kit_and_overflow_rolls_back() {
    let mut initial = construction_fixture();
    initial.autowork = None;
    initial.people[0].controlled = true;
    initial.construction.as_mut().unwrap().policies.clear();
    let mut world = Economy::compile(initial.clone()).unwrap();
    assert!(world.build_site(1, 1).unwrap());
    let state = world.snapshot();
    assert_eq!(state.work_sites.as_ref().unwrap().sites.len(), 1);
    assert_eq!(
        state
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Stock { region: 1, good: 3 })
            .unwrap()
            .used,
        0
    );
    let before = world.snapshot();
    assert!(world.build_site(1, 1).is_err());
    assert_eq!(world.snapshot(), before);
    initial.work_sites.as_mut().unwrap().next_id = u64::MAX;
    let mut world = Economy::compile(initial).unwrap();
    let before = world.snapshot();
    assert!(world.build_site(1, 1).is_err());
    assert_eq!(world.snapshot(), before);
}

#[test]
fn site_materials_keep_real_cohorts_after_building_and_destruction() {
    let mut initial = construction_fixture();
    initial.people[0].controlled = true;
    initial.autowork = None;
    let owner = initial.construction.as_mut().unwrap();
    owner.policies.clear();
    owner.blueprints[0].quantity = 2;
    owner.blueprints[0].salvage = 1;
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 3 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![
            Lot {
                good: 3,
                quantity: 1,
                expires_at: Some(10),
            },
            Lot {
                good: 3,
                quantity: 1,
                expires_at: Some(20),
            },
        ],
    )
    .unwrap();
    let run =
        |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut world = Economy::compile(initial.clone()).unwrap();
                    assert!(world.build_site(1, 1).unwrap());
                    let built = world.snapshot();
                    let material = built
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Site(1))
                        .unwrap();
                    assert_eq!(
                        material.lots,
                        vec![Lot {
                            good: 3,
                            quantity: 1,
                            expires_at: Some(10)
                        }]
                    );
                    assert!(!built.construction.as_ref().unwrap().materials[0].debris);
                    assert!(world.destroy_site(1, 1).unwrap());
                    let destroyed = world.snapshot();
                    assert!(destroyed.construction.as_ref().unwrap().materials[0].debris);
                    assert!(destroyed.work_sites.as_ref().unwrap().sites.is_empty());
                    let restored = Economy::compile(
                        serde_json::from_str(&serde_json::to_string(&destroyed).unwrap()).unwrap(),
                    )
                    .unwrap();
                    assert_eq!(restored.snapshot(), destroyed);
                    let mut invalid = destroyed.clone();
                    invalid.construction.as_mut().unwrap().materials[0].debris = false;
                    assert!(Economy::compile(invalid).is_err());
                    assert!(!world.salvage_site(1, 1, 3, 100).unwrap());
                    assert!(world.salvage_site(1, 1, 3, 1).unwrap());
                    let recovered = world.snapshot();
                    let holder = recovered
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Person(1))
                        .unwrap();
                    assert!(holder.lots.iter().any(|lot| lot.good == 3
                        && lot.quantity == 1
                        && lot.expires_at == Some(10)));
                    assert!(
                        recovered
                            .physical
                            .custody
                            .containers
                            .iter()
                            .all(|row| row.place != Place::Site(1))
                    );
                    assert!(
                        recovered
                            .construction
                            .as_ref()
                            .unwrap()
                            .materials
                            .is_empty()
                    );
                    recovered
                })
        };
    assert_eq!(run(1), run(4));
}

#[test]
fn competing_salvagers_cannot_recover_the_same_material_twice() {
    let mut initial = construction_fixture();
    initial.people[0].controlled = true;
    initial.autowork = None;
    let construction = initial.construction.as_mut().unwrap();
    construction.policies.clear();
    construction.blueprints[0].salvage = 1;
    let mut world = Economy::compile(initial).unwrap();
    assert!(world.build_site(1, 1).unwrap());
    assert!(world.destroy_site(1, 1).unwrap());
    let mut initial = world.snapshot();
    initial.people[0].controlled = false;
    initial.construction.as_mut().unwrap().salvaging = [2, 3]
        .into_iter()
        .map(|person| SalvagePolicy {
            person,
            site: 1,
            quantity: 1,
            willing: true,
            pressured_by: None,
            pressure_threshold: None,
            disregard_claims: true,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let events = &state.construction.as_ref().unwrap().salvage_events;
                assert_eq!(events.len(), 2);
                assert_eq!(events.iter().filter(|event| event.recovered).count(), 1);
                let total: u64 = state
                    .physical
                    .custody
                    .containers
                    .iter()
                    .filter(|row| matches!(row.place, Place::Person(2 | 3)))
                    .flat_map(|row| &row.lots)
                    .filter(|lot| lot.good == 3)
                    .map(|lot| lot.quantity)
                    .sum();
                assert_eq!(total, 1);
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn salvage_can_be_motivated_by_fear_without_a_felt_obligation() {
    let mut initial = construction_fixture();
    initial.people[0].controlled = true;
    initial.autowork = None;
    let owner = initial.construction.as_mut().unwrap();
    owner.policies.clear();
    owner.blueprints[0].salvage = 1;
    let mut world = Economy::compile(initial).unwrap();
    assert!(world.build_site(1, 1).unwrap());
    assert!(world.destroy_site(1, 1).unwrap());
    let mut state = world.snapshot();
    state.opinions.as_mut().unwrap().records = vec![Opinion {
        observer: 2,
        target: 1,
        trust: 0,
        fear: 5,
        obligation: 0,
        affinity: 0,
    }];
    state.construction.as_mut().unwrap().salvaging = vec![SalvagePolicy {
        person: 2,
        site: 1,
        quantity: 1,
        willing: false,
        pressured_by: Some(1),
        pressure_threshold: Some(5),
        disregard_claims: true,
    }];
    let mut world = Economy::compile(state).unwrap();
    world.set_salvage_choice(2, false).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    assert!(state.construction.as_ref().unwrap().salvage_events[0].recovered);
    assert_eq!(state.opinions.unwrap().index().obligation(2, 1), 0);
}

#[test]
fn witnessed_salvage_changes_affinity_only_as_each_claimant_interprets_it() {
    use game_parallel_economy_claims::Claims;
    let mut initial = construction_fixture();
    initial.people[0].controlled = true;
    initial.autowork = None;
    let owner = initial.construction.as_mut().unwrap();
    owner.policies.clear();
    owner.blueprints[0].salvage = 1;
    initial.dynamics = Some(Dynamics {
        personality: None,
        social: None,
        disease: None,
        treatment: vec![],
        claims: Some(Claims::default()),
    });
    let opinions = initial.opinions.as_mut().unwrap();
    opinions.rules.affinity = Some(game_parallel_opinions::AffinityRules {
        limit: 100,
        encounter_change: 0,
        pressured_encounter_change: 0,
        help_change: 0,
        harm_change_per_damage: 0,
    });
    let sensitivity = |person, rate| OpinionSensitivity {
        person,
        report_reception: None,
        affinity_report_discrepancy: None,
        trust_report_discrepancy: None,
        trust_promise_fulfilled: None,
        trust_promise_unfulfilled: None,
        fear_per_damage: None,
        gratitude_gain: None,
        promise_obligation_gain: None,
        obligation_relief: None,
        theft_loss: None,
        confrontation: None,
        affinity_help: None,
        affinity_harm_per_damage: None,
        affinity_encounter: None,
        affinity_pressured_encounter: None,
        affinity_site_use: None,
        affinity_site_destruction: None,
        affinity_site_salvage: Some(rate),
        affinity_work_success: None,
        affinity_work_refusal: None,
        relationship_cases: vec![],
        obligation_development: None,
    };
    let mut first = sensitivity(1, 4);
    first.relationship_cases.push(RelationshipCase {
        target: 2,
        report_discrepancy: None,
        report_discrepancy_trust: None,
        promise_fulfilled_trust: None,
        promise_unfulfilled_trust: None,
        help: None,
        harm_per_damage: None,
        encounter: None,
        pressured_encounter: None,
        site_use: None,
        site_destruction: None,
        site_salvage: Some(7),
        work_success: None,
        work_refusal: None,
        obligation_gain: None,
        promise_obligation_gain: None,
        obligation_relief: None,
    });
    opinions.sensitivities = vec![first, sensitivity(3, -3)].into();
    let run =
        |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut world = Economy::compile(initial.clone()).unwrap();
                    assert!(world.build_site(1, 1).unwrap());
                    world.claim_site(1, 1).unwrap();
                    world.claim_site(3, 1).unwrap();
                    assert!(world.destroy_site(1, 1).unwrap());
                    let mut state = world.snapshot();
                    state.construction.as_mut().unwrap().salvaging = vec![SalvagePolicy {
                        person: 2,
                        site: 1,
                        quantity: 1,
                        willing: true,
                        pressured_by: None,
                        pressure_threshold: None,
                        disregard_claims: true,
                    }];
                    let mut remote = state.clone();
                    remote.people[2].region = Some(2);
                    remote.physical.custody.containers.push(
                        Container::new(Place::Stock { region: 2, good: 1 }, 10, vec![]).unwrap(),
                    );
                    remote
                        .physical
                        .custody
                        .containers
                        .sort_unstable_by_key(|row| row.place);
                    let mut remote = Economy::compile(remote).unwrap();
                    remote.advance().unwrap();
                    let remote = remote.snapshot();
                    assert_eq!(
                        remote
                            .dynamics
                            .as_ref()
                            .unwrap()
                            .claims
                            .as_ref()
                            .unwrap()
                            .site_events
                            .len(),
                        1
                    );
                    assert_eq!(remote.opinions.as_ref().unwrap().index().affinity(3, 2), 0);

                    let mut blocked = state.clone();
                    let holder = blocked
                        .physical
                        .custody
                        .containers
                        .iter_mut()
                        .find(|row| row.place == Place::Person(2))
                        .unwrap();
                    *holder = Container::new(
                        holder.place,
                        holder.capacity,
                        vec![Lot {
                            good: 1,
                            quantity: holder.capacity,
                            expires_at: None,
                        }],
                    )
                    .unwrap();
                    let mut blocked = Economy::compile(blocked).unwrap();
                    blocked.advance().unwrap();
                    let blocked = blocked.snapshot();
                    assert!(
                        blocked
                            .dynamics
                            .as_ref()
                            .unwrap()
                            .claims
                            .as_ref()
                            .unwrap()
                            .site_events
                            .is_empty()
                    );
                    assert_eq!(blocked.opinions.as_ref().unwrap().index().affinity(1, 2), 0);
                    let mut world = Economy::compile(
                        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                    )
                    .unwrap();
                    world.advance().unwrap();
                    let state = world.snapshot();
                    let events = &state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .claims
                        .as_ref()
                        .unwrap()
                        .site_events;
                    assert_eq!(events.len(), 2);
                    assert!(
                        events
                            .iter()
                            .all(|event| event.person == 2 && event.salvaged && !event.destroyed)
                    );
                    let index = state.opinions.as_ref().unwrap().index();
                    assert_eq!(index.affinity(1, 2), 7);
                    assert_eq!(index.affinity(3, 2), -3);
                    assert_eq!(index.obligation(1, 2), 0);
                    state
                })
        };
    assert_eq!(run(1), run(4));
}

#[test]
fn rejected_competing_construction_retires_its_empty_site_custody() {
    let mut initial = construction_fixture();
    initial.autowork = None;
    let owner = initial.construction.as_mut().unwrap();
    owner.blueprints[0].salvage = 1;
    let mut second = owner.policies[0].clone();
    second.person = 3;
    owner.policies.push(second);
    let mut world = Economy::compile(initial).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    assert_eq!(state.work_sites.as_ref().unwrap().sites.len(), 1);
    assert_eq!(state.construction.as_ref().unwrap().materials.len(), 1);
    assert_eq!(
        state
            .physical
            .custody
            .containers
            .iter()
            .filter(|row| matches!(row.place, Place::Site(_)))
            .count(),
        1
    );
    Economy::compile(state).unwrap();
}

#[test]
fn expired_site_material_cannot_be_recovered() {
    let mut initial = construction_fixture();
    initial.people[0].controlled = true;
    initial.autowork = None;
    let owner = initial.construction.as_mut().unwrap();
    owner.policies.clear();
    owner.blueprints[0].salvage = 1;
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 3 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![Lot {
            good: 3,
            quantity: 1,
            expires_at: Some(3),
        }],
    )
    .unwrap();
    let mut world = Economy::compile(initial).unwrap();
    assert!(world.build_site(1, 1).unwrap());
    assert!(world.destroy_site(1, 1).unwrap());
    assert!(!world.salvage_site(1, 1, 3, 1).unwrap());
    let state = world.snapshot();
    assert!(state.construction.as_ref().unwrap().materials.is_empty());
    assert!(
        state
            .physical
            .custody
            .containers
            .iter()
            .all(|row| row.place != Place::Site(1))
    );
    assert!(
        state
            .physical
            .custody
            .containers
            .iter()
            .filter(|row| row.place == Place::Person(1))
            .flat_map(|row| &row.lots)
            .all(|lot| lot.good != 3)
    );
}

#[test]
fn salvage_cleanup_preserves_simultaneous_finished_job_cleanup() {
    let mut initial = construction_fixture();
    initial.people[0].controlled = true;
    initial.people[1].willing_to_work = false;
    initial.work_sites.as_mut().unwrap().requirements.clear();
    let owner = initial.construction.as_mut().unwrap();
    owner.policies.clear();
    owner.blueprints[0].salvage = 1;
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![WorkPolicy {
            person: 2,
            recipe: 1,
            enabled: true,
            disregard_claims: false,
        }],
        requests: None,
    });
    let mut world = Economy::compile(initial).unwrap();
    assert!(world.build_site(1, 1).unwrap());
    assert!(world.destroy_site(1, 1).unwrap());
    world.set_work_choice(2, true).unwrap();
    assert_eq!(world.advance().unwrap().started, vec![1]);
    assert!(world.salvage_site(1, 1, 3, 1).unwrap());
    let state = world.snapshot();
    assert!(
        state
            .physical
            .custody
            .containers
            .iter()
            .all(|row| !matches!(row.place, Place::Site(1) | Place::JobInput(1)))
    );
    Economy::compile(state).unwrap();
}

#[test]
fn later_failure_rolls_back_staged_site_material_and_construction() {
    use game_parallel_dreams::Dream;
    let mut initial = construction_fixture();
    initial.people[0].controlled = true;
    let owner = initial.construction.as_mut().unwrap();
    owner.policies.clear();
    owner.blueprints[0].salvage = 1;
    initial.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: false,
        people: vec![],
        events: vec![],
    });
    initial.dreams = Some(Dreams {
        inherit_births: false,
        learning: vec![],
        people: vec![Dream {
            person: 2,
            recipe: 1,
            priority: 1,
            batch: 1,
            review_interval: u64::MAX,
            next_review: 0,
            enabled: true,
            recognize_help: false,
        }],
        events: vec![],
    });
    let mut world = Economy::compile(initial).unwrap();
    let before = world.snapshot();
    assert!(world.build_site(1, 1).is_err());
    assert_eq!(world.snapshot(), before);
    assert!(
        world
            .snapshot()
            .physical
            .custody
            .containers
            .iter()
            .all(|row| !matches!(row.place, Place::Site(_)))
    );
}

#[test]
fn controlled_site_destruction_requires_an_idle_local_site_and_never_creates_salvage() {
    let mut initial = construction_fixture();
    initial.people[0].controlled = true;
    initial.autowork = None;
    initial.construction.as_mut().unwrap().policies.clear();
    let mut world = Economy::compile(initial).unwrap();
    assert!(world.build_site(1, 1).unwrap());
    let built = world.snapshot();
    let mut historical = built.clone();
    historical.work_sites.as_mut().unwrap().sites.clear();
    Economy::compile(historical).unwrap();
    assert!(world.destroy_site(1, 1).unwrap());
    let destroyed = world.snapshot();
    assert!(destroyed.work_sites.as_ref().unwrap().sites.is_empty());
    assert_eq!(destroyed.work_sites.as_ref().unwrap().next_id, 2);
    assert_eq!(
        destroyed
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Stock { region: 1, good: 3 })
            .unwrap()
            .used,
        0
    );
    Economy::compile(serde_json::from_str(&serde_json::to_string(&destroyed).unwrap()).unwrap())
        .unwrap();
    let before = world.snapshot();
    assert!(world.destroy_site(1, 1).is_err());
    assert_eq!(world.snapshot(), before);
}

#[test]
fn site_destruction_cannot_race_a_start_or_an_occupied_job() {
    let mut initial = construction_fixture();
    initial.people[0].controlled = true;
    initial.physical.recipes[0].ticks = 5;
    initial.construction.as_mut().unwrap().policies.clear();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.build_site(1, 1).unwrap());
                assert!(!world.destroy_site(1, 1).unwrap());
                assert_eq!(
                    world.snapshot().work_sites.as_ref().unwrap().bindings.len(),
                    1
                );
                assert!(!world.destroy_site(1, 1).unwrap());
                world.set_work_choice(2, false).unwrap();
                assert!(!world.destroy_site(1, 1).unwrap());
                assert!(
                    world
                        .snapshot()
                        .work_sites
                        .as_ref()
                        .unwrap()
                        .bindings
                        .is_empty()
                );
                assert!(world.destroy_site(1, 1).unwrap());
                let state = world.snapshot();
                Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn autonomous_site_destruction_is_a_revocable_choice_or_a_response_to_fear() {
    let mut initial = construction_fixture();
    initial.autowork = None;
    initial.work_sites.as_mut().unwrap().sites.push(WorkSite {
        id: 7,
        region: 1,
        kind: "workshop".into(),
        capacity: 1,
    });
    let construction = initial.construction.as_mut().unwrap();
    construction.policies.clear();
    construction.demolition = vec![DemolitionPolicy {
        person: 2,
        site: 7,
        willing: false,
        pressured_by: None,
        pressure_threshold: None,
        disregard_claims: false,
    }];
    let mut refusing = Economy::compile(initial.clone()).unwrap();
    refusing.advance().unwrap();
    assert_eq!(
        refusing.snapshot().work_sites.as_ref().unwrap().sites.len(),
        1
    );
    refusing.set_demolition_choice(2, true).unwrap();
    refusing.advance().unwrap();
    assert!(
        refusing
            .snapshot()
            .work_sites
            .as_ref()
            .unwrap()
            .sites
            .is_empty()
    );
    assert!(
        refusing
            .snapshot()
            .construction
            .as_ref()
            .unwrap()
            .demolition_events[0]
            .destroyed
    );

    let policy = &mut initial.construction.as_mut().unwrap().demolition[0];
    policy.pressured_by = Some(1);
    policy.pressure_threshold = Some(3);
    initial.opinions.as_mut().unwrap().records = vec![Opinion {
        observer: 2,
        target: 1,
        trust: 0,
        fear: 4,
        obligation: 0,
        affinity: 0,
    }];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert!(state.work_sites.as_ref().unwrap().sites.is_empty());
                assert!(!state.construction.as_ref().unwrap().demolition[0].willing);
                assert_eq!(state.opinions.as_ref().unwrap().index().obligation(2, 1), 0);
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn construction_saves_without_demolition_preferences_still_load() {
    let initial = construction_fixture();
    let mut value = serde_json::to_value(initial).unwrap();
    let construction = value["construction"].as_object_mut().unwrap();
    construction.remove("demolition");
    construction.remove("demolition_events");
    construction.remove("goal_builders");
    let world = Economy::compile(serde_json::from_value(value).unwrap()).unwrap();
    let state = world.snapshot();
    let construction = state.construction.unwrap();
    assert!(construction.demolition.is_empty());
    assert!(construction.demolition_events.is_empty());
    assert!(construction.goal_builders.is_empty());
}

#[test]
fn competing_personal_site_destructions_remove_the_site_only_once() {
    let mut initial = construction_fixture();
    initial.autowork = None;
    initial.work_sites.as_mut().unwrap().sites.push(WorkSite {
        id: 7,
        region: 1,
        kind: "workshop".into(),
        capacity: 1,
    });
    let construction = initial.construction.as_mut().unwrap();
    construction.policies.clear();
    construction.demolition = [2, 3, 4]
        .into_iter()
        .map(|person| DemolitionPolicy {
            person,
            site: 7,
            willing: true,
            pressured_by: None,
            pressure_threshold: None,
            disregard_claims: false,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let events = &state.construction.as_ref().unwrap().demolition_events;
                assert_eq!(events.len(), 3);
                assert_eq!(events.iter().filter(|event| event.destroyed).count(), 1);
                assert!(state.work_sites.as_ref().unwrap().sites.is_empty());
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn personal_demolition_cannot_remove_a_site_used_by_simultaneous_work() {
    let mut initial = construction_fixture();
    initial.work_sites.as_mut().unwrap().sites.push(WorkSite {
        id: 7,
        region: 1,
        kind: "workshop".into(),
        capacity: 1,
    });
    let construction = initial.construction.as_mut().unwrap();
    construction.policies.clear();
    construction.demolition = vec![DemolitionPolicy {
        person: 3,
        site: 7,
        willing: true,
        pressured_by: None,
        pressure_threshold: None,
        disregard_claims: false,
    }];
    let mut world = Economy::compile(initial).unwrap();
    assert_eq!(world.advance().unwrap().started.len(), 1);
    let state = world.snapshot();
    assert_eq!(state.work_sites.as_ref().unwrap().sites.len(), 1);
    assert!(!state.construction.as_ref().unwrap().demolition_events[0].destroyed);
    Economy::compile(serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap())
        .unwrap();
}

fn concentrated_site_destruction(count: u64) -> State {
    let mut initial = field_workload(count);
    initial.agriculture = None;
    initial.work_sites = Some(WorkSites {
        next_id: 0,
        sites: (1..=32)
            .map(|id| WorkSite {
                id,
                region: 1,
                kind: "shed".into(),
                capacity: 1,
            })
            .collect(),
        requirements: vec![],
        bindings: vec![],
    });
    initial.construction = Some(Construction {
        materials: vec![],
        salvaging: vec![],
        salvage_events: vec![],
        blueprints: vec![],
        policies: vec![],
        events: vec![],
        demolition_events: vec![],
        goal_builders: vec![],
        demolition: (1..=count)
            .map(|person| DemolitionPolicy {
                person,
                site: 1 + (person % 32),
                willing: true,
                pressured_by: None,
                pressure_threshold: None,
                disregard_claims: false,
            })
            .collect(),
    });
    initial
}

#[test]
fn concentrated_site_destruction_resolves_each_physical_site_once() {
    let count = 4096;
    let initial = concentrated_site_destruction(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert!(state.work_sites.as_ref().unwrap().sites.is_empty());
                let events = &state.construction.as_ref().unwrap().demolition_events;
                assert_eq!(events.len(), count as usize);
                assert_eq!(events.iter().filter(|e| e.destroyed).count(), 32);
                Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore = "performance measurement"]
fn concentrated_site_destruction_benchmark() {
    let workers: usize = std::env::var("LEGACY_WORKERS")
        .unwrap_or_else(|_| "1".into())
        .parse()
        .unwrap();
    let initial = concentrated_site_destruction(100_000);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut world = Economy::compile(initial).unwrap();
            let started = std::time::Instant::now();
            world.advance().unwrap();
            eprintln!(
                "workers={workers} people=100000 ticks=1 elapsed={:.6}",
                started.elapsed().as_secs_f64()
            );
            assert_eq!(
                world
                    .snapshot()
                    .construction
                    .unwrap()
                    .demolition_events
                    .iter()
                    .filter(|event| event.destroyed)
                    .count(),
                32
            );
        });
}

fn concentrated_site_construction(count: u64) -> State {
    let mut state = field_workload(count);
    state.agriculture = None;
    state.people[0].controlled = true;
    state.work_sites = Some(WorkSites {
        next_id: 0,
        sites: vec![],
        requirements: vec![],
        bindings: vec![],
    });
    state.construction = Some(Construction {
        materials: vec![],
        salvaging: vec![],
        salvage_events: vec![],
        blueprints: vec![BuildingBlueprint {
            salvage: 0,
            id: 1,
            name: "workshop kit".into(),
            kind: "workshop".into(),
            capacity: 1,
            kit: 2,
            quantity: 1,
        }],
        policies: (2..=count)
            .map(|person| ConstructionPolicy {
                person,
                blueprint: 1,
                target_sites: 1,
                willing: true,
                pressured_by: None,
                pressure_threshold: None,
                disregard_claims: false,
            })
            .collect(),
        events: vec![],
        demolition: vec![],
        demolition_events: vec![],
        goal_builders: vec![],
    });
    let stock = state
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 2 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![Lot {
            good: 2,
            quantity: count / 8,
            expires_at: None,
        }],
    )
    .unwrap();
    state
}

fn concentrated_site_salvage(count: u64) -> State {
    let mut state = concentrated_site_construction(count);
    state.work_sites.as_mut().unwrap().next_id = 2;
    let owner = state.construction.as_mut().unwrap();
    owner.policies.clear();
    owner.materials = vec![game_parallel_economy_building::Material {
        site: 1,
        region: 1,
        debris: true,
    }];
    owner.salvaging = (2..=count)
        .map(|person| SalvagePolicy {
            person,
            site: 1,
            quantity: 1,
            willing: true,
            pressured_by: None,
            pressure_threshold: None,
            disregard_claims: true,
        })
        .collect();
    let stock = state
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 2 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![Lot {
            good: 2,
            quantity: count / 8 - 1,
            expires_at: None,
        }],
    )
    .unwrap();
    state.physical.custody.containers.push(
        Container::new(
            Place::Site(1),
            1,
            vec![Lot {
                good: 2,
                quantity: 1,
                expires_at: None,
            }],
        )
        .unwrap(),
    );
    state
        .physical
        .custody
        .containers
        .sort_unstable_by_key(|row| row.place);
    state
}

#[test]
fn concentrated_salvage_has_one_winner_across_worker_counts() {
    let initial = concentrated_site_salvage(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state
                        .construction
                        .as_ref()
                        .unwrap()
                        .salvage_events
                        .iter()
                        .filter(|row| row.recovered)
                        .count(),
                    1
                );
                assert!(state.construction.as_ref().unwrap().materials.is_empty());
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore = "performance measurement"]
fn concentrated_site_salvage_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let mut initial = concentrated_site_salvage(count);
    let claimed = std::env::var("LEGACY_CLAIMS").is_ok_and(|value| value == "1");
    if claimed {
        use game_parallel_economy_claims::{Claims, SiteClaim};
        initial.dynamics = Some(Dynamics {
            personality: None,
            social: None,
            disease: None,
            treatment: vec![],
            claims: Some(Claims {
                site_claims: (1..=count)
                    .map(|person| SiteClaim { person, site: 1 })
                    .collect(),
                ..Default::default()
            }),
        });
        let mut opinions = fixture().opinions.unwrap();
        opinions.records.clear();
        opinions.rules.affinity = Some(game_parallel_opinions::AffinityRules {
            limit: 100,
            encounter_change: 0,
            pressured_encounter_change: 0,
            help_change: 0,
            harm_change_per_damage: 0,
        });
        opinions.sensitivities = (1..=count)
            .map(|person| OpinionSensitivity {
                person,
                report_reception: None,
                affinity_report_discrepancy: None,
                trust_report_discrepancy: None,
                trust_promise_fulfilled: None,
                trust_promise_unfulfilled: None,
                fear_per_damage: None,
                gratitude_gain: None,
                promise_obligation_gain: None,
                obligation_relief: None,
                theft_loss: None,
                confrontation: None,
                affinity_help: None,
                affinity_harm_per_damage: None,
                affinity_encounter: None,
                affinity_pressured_encounter: None,
                affinity_site_use: None,
                affinity_site_destruction: None,
                affinity_site_salvage: Some(1),
                affinity_work_success: None,
                affinity_work_refusal: None,
                relationship_cases: vec![],
                obligation_development: None,
            })
            .collect::<Vec<_>>()
            .into();
        initial.opinions = Some(opinions);
    }
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                let elapsed = started.elapsed().as_secs_f64();
                let state = world.snapshot();
                assert_eq!(
                    state
                        .construction
                        .unwrap()
                        .salvage_events
                        .iter()
                        .filter(|row| row.recovered)
                        .count(),
                    1
                );
                if claimed {
                    let observations = state.dynamics.unwrap().claims.unwrap().site_events;
                    assert_eq!(observations.len(), 4);
                    assert!(observations.iter().all(|event| event.salvaged));
                    assert_eq!(state.opinions.unwrap().records.len(), 4);
                }
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} claimed={claimed} ticks=1 salvage_seconds={:.6}",
                samples[1]
            );
        });
}

fn concentrated_built_site_claims(count: u64) -> State {
    use game_parallel_economy_claims::{Claims, Preference};
    let mut state = concentrated_site_construction(count);
    state.dynamics = Some(Dynamics {
        personality: None,
        social: None,
        disease: None,
        treatment: vec![],
        claims: Some(Claims {
            inherit_births: false,
            preferences: (2..=count)
                .map(|person| Preference {
                    person,
                    disregard_others: false,
                    assert_outputs: false,
                    assert_built_sites: true,
                    desperate_hunger: None,
                })
                .collect(),
            ..Default::default()
        }),
    });
    state
}

#[test]
fn concentrated_builders_claim_only_physically_completed_sites() {
    let initial = concentrated_built_site_claims(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let sites = &state.work_sites.as_ref().unwrap().sites;
                let claims = &state
                    .dynamics
                    .as_ref()
                    .unwrap()
                    .claims
                    .as_ref()
                    .unwrap()
                    .site_claims;
                assert_eq!(sites.len(), 512);
                assert_eq!(claims.len(), 512);
                let mut claimed: Vec<_> = claims.iter().map(|claim| claim.site).collect();
                claimed.sort_unstable();
                assert_eq!(
                    claimed,
                    sites.iter().map(|site| site.id).collect::<Vec<_>>()
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore = "performance measurement"]
fn concentrated_built_site_claims_benchmark() {
    let workers: usize = std::env::var("LEGACY_WORKERS")
        .unwrap_or_else(|_| "1".into())
        .parse()
        .unwrap();
    let initial = concentrated_built_site_claims(100_000);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut world = Economy::compile(initial).unwrap();
            let started = std::time::Instant::now();
            world.advance().unwrap();
            eprintln!(
                "workers={workers} people=100000 ticks=1 elapsed={:.6}",
                started.elapsed().as_secs_f64()
            );
            assert_eq!(
                world
                    .snapshot()
                    .dynamics
                    .unwrap()
                    .claims
                    .unwrap()
                    .site_claims
                    .len(),
                12_500
            );
        });
}

fn concentrated_goal_construction(count: u64) -> State {
    use game_parallel_goals::Goal;
    let mut state = concentrated_site_construction(count);
    state.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: None,
    });
    state.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: false,
        events: vec![],
        people: (2..=count)
            .map(|person| Goal {
                person,
                recipe: 1,
                priority: 1,
                target: 1,
                progress: 0,
                next_review: 0,
                enabled: true,
                recognize_help: false,
            })
            .collect(),
    });
    state.work_sites.as_mut().unwrap().requirements = vec![WorkSiteRequirement {
        recipe: 1,
        kind: "workshop".into(),
        slots: 1,
    }];
    let construction = state.construction.as_mut().unwrap();
    construction.policies.clear();
    construction.goal_builders = (2..=count)
        .map(|person| GoalBuilder {
            person,
            enabled: true,
            disregard_claims: false,
        })
        .collect();
    for person in &mut state.people {
        person.willing_to_work = true;
    }
    state
}

fn concentrated_goal_kit_production(count: u64) -> State {
    let mut state = concentrated_goal_construction(count);
    state.goals.as_mut().unwrap().plan_inputs = true;
    state.construction.as_mut().unwrap().blueprints[0].kit = 3;
    state.physical.recipes.push(Recipe {
        id: 4,
        name: "make site kit".into(),
        ticks: 1,
        inputs: vec![Amount {
            good: 2,
            quantity: 1,
        }],
        outputs: vec![Amount {
            good: 3,
            quantity: 1,
        }],
    });
    state
}

fn concentrated_intermediate_site_planning(count: u64) -> State {
    let mut state = concentrated_goal_kit_production(count);
    state
        .work_sites
        .as_mut()
        .unwrap()
        .requirements
        .push(WorkSiteRequirement {
            recipe: 4,
            kind: "forge".into(),
            slots: 1,
        });
    state
        .construction
        .as_mut()
        .unwrap()
        .blueprints
        .push(BuildingBlueprint {
            salvage: 0,
            id: 2,
            name: "forge kit".into(),
            kind: "forge".into(),
            capacity: 1,
            kit: 2,
            quantity: 1,
        });
    state
}

#[test]
fn concentrated_intermediate_site_plans_consume_only_available_kits() {
    let initial = concentrated_intermediate_site_planning(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert!(world.advance().unwrap().started.is_empty());
                let state = world.snapshot();
                let sites = &state.work_sites.as_ref().unwrap().sites;
                assert_eq!(sites.len(), 512);
                assert!(sites.iter().all(|site| site.kind == "forge"));
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore = "performance measurement"]
fn concentrated_intermediate_site_planning_benchmark() {
    let workers: usize = std::env::var("LEGACY_WORKERS")
        .unwrap_or_else(|_| "1".into())
        .parse()
        .unwrap();
    let initial = concentrated_intermediate_site_planning(100_000);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut world = Economy::compile(initial).unwrap();
            let started = std::time::Instant::now();
            world.advance().unwrap();
            eprintln!(
                "workers={workers} people=100000 ticks=1 elapsed={:.6}",
                started.elapsed().as_secs_f64()
            );
            assert_eq!(world.snapshot().work_sites.unwrap().sites.len(), 12_500);
        });
}

#[test]
fn concentrated_missing_kit_plans_compete_for_actual_materials() {
    let initial = concentrated_goal_kit_production(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let events = world.advance().unwrap();
                assert_eq!(events.started.len(), 512);
                let state = world.snapshot();
                assert!(state.work_sites.as_ref().unwrap().sites.is_empty());
                assert!(state.physical.work.iter().all(|work| work.job.recipe == 4));
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn missing_kit_planning_does_not_use_a_recipe_requiring_the_missing_site() {
    let mut initial = concentrated_goal_kit_production(16);
    initial.physical.recipes.retain(|recipe| recipe.id != 4);
    let mut world = Economy::compile(initial).unwrap();
    for _ in 0..4 {
        assert!(world.advance().unwrap().started.is_empty());
    }
    let state = world.snapshot();
    assert!(state.work_sites.as_ref().unwrap().sites.is_empty());
    assert!(state.physical.work.is_empty());
    assert_eq!(
        state
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Stock { region: 1, good: 2 })
            .unwrap()
            .used,
        2
    );
}

#[test]
#[ignore = "performance measurement"]
fn concentrated_missing_kit_planning_benchmark() {
    let workers: usize = std::env::var("LEGACY_WORKERS")
        .unwrap_or_else(|_| "1".into())
        .parse()
        .unwrap();
    let initial = concentrated_goal_kit_production(100_000);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut world = Economy::compile(initial).unwrap();
            let started = std::time::Instant::now();
            let events = world.advance().unwrap();
            eprintln!(
                "workers={workers} people=100000 ticks=1 elapsed={:.6}",
                started.elapsed().as_secs_f64()
            );
            assert_eq!(events.started.len(), 12_500);
        });
}

#[test]
fn concentrated_goal_builders_consume_only_real_kits() {
    let initial = concentrated_goal_construction(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state.work_sites.as_ref().unwrap().sites.len(), 512);
                assert_eq!(
                    state
                        .construction
                        .as_ref()
                        .unwrap()
                        .events
                        .iter()
                        .filter(|event| event.site.is_some())
                        .count(),
                    512
                );
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Stock { region: 1, good: 2 })
                        .unwrap()
                        .used,
                    0
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore = "performance measurement"]
fn concentrated_goal_construction_benchmark() {
    let workers: usize = std::env::var("LEGACY_WORKERS")
        .unwrap_or_else(|_| "1".into())
        .parse()
        .unwrap();
    let initial = concentrated_goal_construction(100_000);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut world = Economy::compile(initial).unwrap();
            let started = std::time::Instant::now();
            world.advance().unwrap();
            eprintln!(
                "workers={workers} people=100000 ticks=1 elapsed={:.6}",
                started.elapsed().as_secs_f64()
            );
            assert_eq!(world.snapshot().work_sites.unwrap().sites.len(), 12_500);
        });
}

#[test]
fn concentrated_construction_creates_only_sites_backed_by_consumed_kits() {
    let count = 4096;
    let initial = concentrated_site_construction(count);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state.work_sites.as_ref().unwrap().sites.len(),
                    count as usize / 8
                );
                assert_eq!(
                    state
                        .construction
                        .as_ref()
                        .unwrap()
                        .events
                        .iter()
                        .filter(|e| e.site.is_some())
                        .count(),
                    count as usize / 8
                );
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Stock { region: 1, good: 2 })
                        .unwrap()
                        .used,
                    0
                );
                Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_site_construction_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let mut initial = concentrated_site_construction(count);
    let retain = std::env::var("LEGACY_SALVAGE").is_ok_and(|value| value == "1");
    if retain {
        initial.construction.as_mut().unwrap().blueprints[0].salvage = 1;
    }
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = Economy::compile(initial.clone()).unwrap();
                let started = std::time::Instant::now();
                world.advance().unwrap();
                let elapsed = started.elapsed().as_secs_f64();
                if retain {
                    assert_eq!(
                        world.snapshot().construction.unwrap().materials.len(),
                        count as usize / 8
                    );
                }
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} retained={retain} ticks=1 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn construction_respects_personal_claims_and_expiring_materials() {
    use game_parallel_economy_claims::{Claim, Claims};
    let mut initial = construction_fixture();
    initial.autowork = None;
    initial.dynamics = Some(Dynamics {
        personality: None,
        social: None,
        disease: None,
        treatment: vec![],
        claims: Some(Claims {
            inherit_births: false,
            site_claims: vec![],
            site_events: vec![],
            claims: vec![Claim {
                person: 1,
                region: 1,
                good: 3,
            }],
            preferences: vec![],
            events: vec![],
        }),
    });
    let mut respecting = Economy::compile(initial.clone()).unwrap();
    respecting.advance().unwrap();
    assert!(
        respecting
            .snapshot()
            .work_sites
            .as_ref()
            .unwrap()
            .sites
            .is_empty()
    );
    initial.construction.as_mut().unwrap().policies[0].disregard_claims = true;
    let mut disregarding = Economy::compile(initial.clone()).unwrap();
    disregarding.advance().unwrap();
    let state = disregarding.snapshot();
    assert_eq!(state.work_sites.as_ref().unwrap().sites.len(), 1);
    assert!(
        state
            .dynamics
            .as_ref()
            .unwrap()
            .claims
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|e| e.person == 2 && e.good == 3)
    );
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 3 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![Lot {
            good: 3,
            quantity: 1,
            expires_at: Some(1),
        }],
    )
    .unwrap();
    let mut expired = Economy::compile(initial).unwrap();
    expired.advance().unwrap();
    let state = expired.snapshot();
    assert!(state.work_sites.as_ref().unwrap().sites.is_empty());
    assert_eq!(state.work_sites.as_ref().unwrap().next_id, 1);
}

#[test]
fn a_newly_built_site_is_available_to_other_workers_on_the_next_boundary() {
    let mut initial = construction_fixture();
    initial.people[2].willing_to_work = true;
    initial.autowork.as_mut().unwrap().policies = vec![WorkPolicy {
        person: 3,
        recipe: 1,
        enabled: true,
        disregard_claims: false,
    }];
    let mut world = Economy::compile(initial).unwrap();
    assert!(world.advance().unwrap().started.is_empty());
    assert_eq!(world.snapshot().work_sites.as_ref().unwrap().sites.len(), 1);
    assert_eq!(world.advance().unwrap().started.len(), 1);
    assert_eq!(world.snapshot().physical.work[0].job.worker, 3);
}

#[test]
fn old_saved_site_state_receives_a_safe_next_identity() {
    let mut initial = fixture();
    initial.agreements = None;
    initial.autowork = None;
    initial.work_sites = Some(WorkSites {
        next_id: 0,
        sites: vec![WorkSite {
            id: 7,
            region: 1,
            kind: "workshop".into(),
            capacity: 1,
        }],
        requirements: vec![],
        bindings: vec![],
    });
    let mut encoded = serde_json::to_value(initial).unwrap();
    encoded["work_sites"]
        .as_object_mut()
        .unwrap()
        .remove("next_id");
    let world = Economy::compile(serde_json::from_value(encoded).unwrap()).unwrap();
    assert_eq!(world.snapshot().work_sites.as_ref().unwrap().next_id, 8);
}

#[test]
fn controlled_construction_uses_carried_kits_without_taking_claimed_stock() {
    use game_parallel_economy_claims::{Claim, Claims};
    let mut initial = construction_fixture();
    initial.autowork = None;
    initial.people[0].controlled = true;
    initial.construction.as_mut().unwrap().policies.clear();
    initial.dynamics = Some(Dynamics {
        personality: None,
        social: None,
        disease: None,
        treatment: vec![],
        claims: Some(Claims {
            inherit_births: false,
            site_claims: vec![],
            site_events: vec![],
            claims: vec![Claim {
                person: 2,
                region: 1,
                good: 3,
            }],
            preferences: vec![],
            events: vec![],
        }),
    });
    let holder = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(1))
        .unwrap();
    *holder = Container::new(
        holder.place,
        holder.capacity,
        vec![Lot {
            good: 3,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut world = Economy::compile(initial).unwrap();
    assert!(world.build_site(1, 1).unwrap());
    let state = world.snapshot();
    assert_eq!(
        state
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(1))
            .unwrap()
            .used,
        0
    );
    assert_eq!(
        state
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Stock { region: 1, good: 3 })
            .unwrap()
            .used,
        1
    );
    assert!(
        state
            .dynamics
            .as_ref()
            .unwrap()
            .claims
            .as_ref()
            .unwrap()
            .events
            .is_empty()
    );
}

#[test]
fn people_produce_a_kit_build_a_site_and_then_use_it_for_work() {
    let mut initial = construction_fixture();
    initial.physical.recipes.push(Recipe {
        id: 4,
        name: "make workshop kit".into(),
        ticks: 1,
        inputs: vec![Amount {
            good: 2,
            quantity: 1,
        }],
        outputs: vec![Amount {
            good: 3,
            quantity: 1,
        }],
    });
    initial.people[0].willing_to_work = true;
    initial.people[2].willing_to_work = true;
    initial.autowork.as_mut().unwrap().policies = vec![
        WorkPolicy {
            person: 1,
            recipe: 4,
            enabled: true,
            disregard_claims: false,
        },
        WorkPolicy {
            person: 3,
            recipe: 1,
            enabled: true,
            disregard_claims: false,
        },
    ];
    let kit_stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Stock { region: 1, good: 3 })
        .unwrap();
    *kit_stock = Container::new(kit_stock.place, kit_stock.capacity, vec![]).unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = Economy::compile(initial.clone()).unwrap();
                assert_eq!(world.advance().unwrap().started.len(), 1);
                assert_eq!(world.snapshot().physical.work[0].job.recipe, 4);
                assert_eq!(world.advance().unwrap().finished.len(), 1);
                world.set_work_choice(1, false).unwrap();
                assert!(
                    world
                        .snapshot()
                        .work_sites
                        .as_ref()
                        .unwrap()
                        .sites
                        .is_empty()
                );
                world.advance().unwrap();
                let delivered = world.snapshot();
                assert_eq!(
                    delivered
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == Place::Stock { region: 1, good: 3 })
                        .unwrap()
                        .used,
                    1
                );
                let mut restored = Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&delivered).unwrap()).unwrap(),
                )
                .unwrap();
                assert!(world.advance().unwrap().started.is_empty());
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                assert_eq!(world.snapshot().work_sites.as_ref().unwrap().sites.len(), 1);
                assert_eq!(world.advance().unwrap().started.len(), 1);
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert!(
                    state
                        .physical
                        .work
                        .iter()
                        .any(|w| w.job.worker == 3 && w.job.recipe == 1)
                );
                assert_eq!(state.work_sites.as_ref().unwrap().bindings.len(), 1);
                state
            })
    };
    assert_eq!(run(1), run(4));
}
