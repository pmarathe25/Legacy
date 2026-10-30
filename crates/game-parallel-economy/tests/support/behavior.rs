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
            sensitivities: vec![],
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
        food_search: None,
        giving: None,
        medical: None,
        hauling: None,
    }
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
    initial.autowork = None;
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
        fear_per_damage: None,
        gratitude_gain: Some(0),
        obligation_relief: None,
        theft_loss: None,
        confrontation: None,
        affinity_help: Some(-2),
        affinity_harm_per_damage: None,
        affinity_encounter: None,
        affinity_pressured_encounter: None,
        relationship_cases: vec![],
        obligation_development: None,
        promise_obligation_gain: None,
    }];
    let mut world = Economy::compile(wrong).unwrap();
    assert!(!world.treat_other(1, 2, 3, 1).unwrap());
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
        policies: vec![MedicalPolicy {
            person: 4,
            patient: 3,
            medicine: 2,
            quantity: 1,
            willing: false,
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
        policies: (2..=count)
            .map(|person| MedicalPolicy {
                person,
                patient: 1,
                medicine: 2,
                quantity: 1,
                willing: true,
                pressured_by: None,
                pressure_threshold: None,
            })
            .collect(),
    });
    state
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
            pressured_by: None,
            pressure_threshold: None,
        },
        MedicalPolicy {
            person: 3,
            patient: 1,
            medicine: 2,
            quantity: 1,
            willing: true,
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
            pressured_by: None,
            pressure_threshold: None,
        })
        .collect();
    state
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
            claims: vec![Claim {
                person: 1,
                region: 1,
                good: 1,
            }],
            preferences: vec![Preference {
                person: 2,
                disregard_others: false,
                assert_outputs: false,
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
    let initial = giving_workload(count);
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
fn people_make_promises_but_can_change_their_minds_before_physical_delivery() {
    let mut state = fixture();
    state.agreements = Some(Agreements {
        offering: vec![],
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
    let initial = concentrated_combat(count);
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
fn recipient_can_automatically_acknowledge_a_promise_without_compelling_its_maker() {
    let mut state = fixture();
    state.agreements = Some(Agreements {
        offering: vec![],
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
        people: vec![Dream {
            person: 2,
            recipe: 1,
            priority: 4,
            batch: 1,
            review_interval: 2,
            next_review: 0,
            enabled: true,
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
            },
            Goal {
                person: 3,
                recipe: 1,
                priority: 1,
                target: 1,
                progress: 0,
                next_review: 0,
                enabled: true,
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
            preferences: (1..=count)
                .map(|person| Preference {
                    person,
                    disregard_others: false,
                    assert_outputs: true,
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
            next_id: 2,
            memory_limit: 0,
            work_links: vec![],
            memories: vec![],
            preferences: vec![Preference {
                person: 2,
                effort: 5,
                help_value: 0,
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
            next_id: 2,
            memory_limit: 2,
            work_links: vec![],
            memories: vec![],
            preferences: vec![Preference {
                person: 2,
                effort: 5,
                help_value: 0,
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
        }],
        events: vec![],
    });
    initial.autowork = Some(Autowork {
        next_id: 1,
        policies: vec![],
        requests: Some(Requests {
            next_id: 1,
            memory_limit: 2,
            work_links: vec![],
            memories: vec![],
            preferences: vec![Preference {
                person: 2,
                effort: 5,
                help_value: 0,
                disregard_claims: false,
            }],
            askers: vec![Asker {
                person: 1,
                interval: 10,
                lifetime: 5,
            }],
            pending: vec![],
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
                state
            })
    };
    assert_eq!(run(1), run(4));
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
                records: vec![],
            },
            Mind {
                person: 2,
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
                records: vec![],
            },
            Mind {
                person: 4,
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
        fear_per_damage: None,
        gratitude_gain: Some(0),
        obligation_relief: None,
        theft_loss: None,
        confrontation: None,
        affinity_help: None,
        affinity_harm_per_damage: None,
        affinity_encounter: None,
        affinity_pressured_encounter: None,
        relationship_cases: vec![],
        obligation_development: None,
        promise_obligation_gain: None,
    }];
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
        fear_per_damage: None,
        gratitude_gain: Some(0),
        obligation_relief: None,
        theft_loss: None,
        confrontation: None,
        affinity_help: None,
        affinity_harm_per_damage: None,
        affinity_encounter: None,
        affinity_pressured_encounter: None,
        relationship_cases: vec![],
        obligation_development: None,
        promise_obligation_gain: None,
    }];
    opinions.records = vec![Opinion {
        observer: 2,
        target: 1,
        trust: 0,
        fear: 5,
        obligation: 0,
        affinity: 0,
    }];
    initial.giving = Some(Giving {
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
        fear_per_damage: None,
        gratitude_gain: Some(0),
        obligation_relief: None,
        theft_loss: None,
        confrontation: None,
        affinity_help: Some(-1),
        affinity_harm_per_damage: Some(2),
        affinity_encounter: None,
        affinity_pressured_encounter: None,
        relationship_cases: vec![],
        obligation_development: None,
        promise_obligation_gain: None,
    };
    let before = world.snapshot();
    sensitivity.relationship_cases.push(RelationshipCase {
        target: 999,
        help: None,
        harm_per_damage: Some(-1),
        encounter: None,
        pressured_encounter: None,
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
        vec![sensitivity]
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
        fear_per_damage: None,
        gratitude_gain: Some(0),
        obligation_relief: None,
        theft_loss: None,
        confrontation: None,
        affinity_help: None,
        affinity_harm_per_damage: Some(1),
        affinity_encounter: None,
        affinity_pressured_encounter: None,
        relationship_cases: vec![],
        obligation_development: None,
        promise_obligation_gain: None,
    }];
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
        fear_per_damage: None,
        gratitude_gain: Some(2),
        obligation_relief: None,
        theft_loss: None,
        confrontation: None,
        affinity_help: None,
        affinity_harm_per_damage: None,
        affinity_encounter: None,
        affinity_pressured_encounter: None,
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
        vec![sensitivity(3, -1, 2), sensitivity(4, 1, -2)];
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
            fear_per_damage: None,
            gratitude_gain: Some(2),
            obligation_relief: None,
            theft_loss: None,
            confrontation: None,
            affinity_help: None,
            affinity_harm_per_damage: None,
            affinity_encounter: None,
            affinity_pressured_encounter: None,
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
        .collect();
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
    initial.opinions.as_mut().unwrap().sensitivities.clear();
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
        relationship_cases: vec![],
        obligation_development: None,
    };
    opinions.sensitivities = vec![sensitivity(2, 0), sensitivity(4, 3)];
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
        relationship_cases: vec![],
        obligation_development: None,
    }];
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
            relationship_cases: vec![],
            obligation_development: None,
        })
        .collect();
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
            next_id: 2,
            memory_limit: 2,
            work_links: vec![],
            memories: vec![],
            preferences: vec![Preference {
                person: 2,
                effort: 5,
                help_value: 0,
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
