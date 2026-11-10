use game_economy::core_interval::{CoreHost, CoreHostSnapshot, CoreInterval, CoreSnapshot};
use game_economy_person::{
    Person, Rules,
    travel::{Journey, Route, Travel},
};
use game_lot_diet::Effects;
use game_lot_runtime::{Runtime, Snapshot};
use game_lots::{Container, Lot, Place, State};
use game_production_model::{Amount, Good, Recipe, Start};

fn fixture() -> CoreSnapshot {
    let container = |region, good, quantity| {
        Container::new(
            Place::Stock { region, good },
            10,
            if quantity == 0 {
                vec![]
            } else {
                vec![Lot {
                    item: None,
                    good,
                    quantity,
                    expires_at: None,
                }]
            },
        )
        .unwrap()
    };
    let mut runtime = Runtime::compile(Snapshot {
        custody: State {
            tick: 0,
            revision: 0,
            containers: vec![
                container(1, 1, 2),
                container(1, 2, 0),
                container(2, 1, 0),
                container(2, 2, 0),
            ],
        },
        goods: vec![
            Good {
                id: 1,
                name: "input".into(),
                food: None,
            },
            Good {
                id: 2,
                name: "output".into(),
                food: None,
            },
        ],
        recipes: vec![Recipe {
            id: 1,
            name: "work".into(),
            ticks: 1,
            inputs: vec![Amount {
                good: 1,
                quantity: 2,
            }],
            outputs: vec![Amount {
                good: 2,
                quantity: 1,
            }],
        }],
        tick_millis: 1000,
        work: vec![],
        work_interval: None,
    })
    .unwrap();
    runtime
        .start(
            &[Start {
                id: 1,
                worker: 1,
                region: 1,
                recipe: 1,
                units: 1,
            }],
            7,
        )
        .unwrap();
    let person = |id, region| Person {
        id,
        region,
        health: 100,
        hunger: 2,
        hunger_remainder: 0,
        immortal: None,
        willing_to_work: false,
        seek_meals: false,
        controlled: false,
    };
    CoreSnapshot {
        seed: 7,
        physical: runtime.snapshot(),
        people: vec![person(1, Some(1)), person(2, None)],
        rules: Rules {
            hunger_per_tick: 1,
            meal_threshold: 1,
            starvation_threshold: 3,
            starvation_damage: 4,
            max_health: 100,
            carrying_capacity: 10,
            max_work_hunger: 10,
        },
        travel: Some(Travel {
            routes: vec![Route {
                source: 1,
                destination: 2,
                ticks: 1,
            }],
            journeys: vec![Journey {
                person: 2,
                source: 1,
                destination: 2,
                remaining: 1,
                progress_millis: 0,
                pending_arrival: false,
            }],
            ..Default::default()
        }),
        nutrition: Some(game_nutrition::Nutrition {
            rules: game_nutrition::Rules {
                interval: 1,
                capacity: [10; 6],
                initial: [2; 6],
                metabolism: [1; 6],
                minimum: [0; 6],
                deficiency_damage: 0,
                scalar_meal: [0; 6],
            },
            people: vec![
                game_nutrition::Person {
                    person: 1,
                    reserves: [2; 6],
                },
                game_nutrition::Person {
                    person: 2,
                    reserves: [2; 6],
                },
            ],
        }),
        pending_food: vec![game_production_model::FoodEffect {
            person: 2,
            at: 1,
            health: 1,
            nutrients: [1; 6],
        }],
    }
}

fn timed_fatigue() -> game_economy_fatigue::TimedFatigue {
    game_economy_fatigue::TimedFatigue {
        fatigue: game_economy_fatigue::Fatigue {
            rules: game_economy_fatigue::Rules {
                capacity: 10,
                baseline_gain: 0,
                work_gain: 4,
                travel_gain: 2,
                idle_recovery: 0,
                rest_recovery: 4,
                rest_at: 2,
                resume_at: 0,
                exhaustion_at: 10,
                exhaustion_damage: 1,
            },
            people: vec![],
        },
        remainders: vec![],
    }
}

#[test]
fn timed_fatigue_tracks_early_completion_across_save_and_workers() {
    let run = |workers, split| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let snapshot = CoreHostSnapshot {
                    core: fixture(),
                    autowork: None,
                    tool_wear: None,
                    tool_requirements: vec![],
                    paused_work_rates: vec![],
                    fatigue: Some(timed_fatigue()),
                    activity: vec![],
                    agriculture: None,
                    health: None,
                    knowledge: None,
                    social: None,
                    claims: None,
                    food_search: None,
                    opinions: None,
                    goals: None,
                    agreements: None,
                    sites: None,
                    schedule: None,
                };
                let mut host = CoreHost::begin(snapshot, &[(1, 2)]).unwrap();
                host.enable_schedule(1000, 1).unwrap();
                if split {
                    assert_eq!(host.advance_continuous(250, &[]).unwrap().failure, None);
                    let saved = serde_json::to_string(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                }
                assert_eq!(
                    host.advance_continuous(if split { 750 } else { 1000 }, &[])
                        .unwrap()
                        .failure,
                    None
                );
                let state = host.snapshot();
                let owner = state.fatigue.as_ref().unwrap();
                let worker = owner.fatigue.people.iter().find(|row| row.id == 1).unwrap();
                assert_eq!((worker.fatigue, worker.resting), (2, true));
                assert!(state.activity.iter().all(|row| row.alive_millis == 0));
                state
            })
    };
    assert_eq!(run(1, false), run(6, true));
}

#[test]
fn timed_fatigue_rest_pauses_and_resumes_unfinished_work() {
    let mut initial = fixture();
    initial.physical.recipes[0].ticks = 2;
    initial.physical.work[0].job.remaining = 2;
    let snapshot = CoreHostSnapshot {
        core: initial,
        autowork: None,
        tool_wear: None,
        tool_requirements: vec![],
        paused_work_rates: vec![],
        fatigue: Some(timed_fatigue()),
        activity: vec![],
        agriculture: None,
        health: None,
        knowledge: None,
        social: None,
        claims: None,
        food_search: None,
        opinions: None,
        goals: None,
        agreements: None,
        sites: None,
        schedule: None,
    };
    let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
    host.enable_schedule(1000, 1).unwrap();
    assert_eq!(host.advance_continuous(1000, &[]).unwrap().failure, None);
    let state = host.snapshot();
    assert!(
        state
            .fatigue
            .unwrap()
            .fatigue
            .people
            .iter()
            .find(|p| p.id == 1)
            .unwrap()
            .resting
    );
    assert_eq!(state.core.physical.work[0].job.remaining, 1);
    assert_eq!(host.advance_continuous(1000, &[]).unwrap().failure, None);
    let state = host.snapshot();
    assert!(
        state
            .fatigue
            .unwrap()
            .fatigue
            .people
            .iter()
            .all(|p| p.id != 1 || !p.resting)
    );
    assert_eq!(state.core.physical.work[0].job.remaining, 1);
    assert_eq!(host.advance_continuous(1000, &[]).unwrap().failure, None);
    let state = host.snapshot();
    assert_eq!(state.core.physical.work[0].job.remaining, 0);
    assert_eq!(
        state.core.physical.work[0].phase,
        game_lot_runtime::Phase::Waiting
    );
}

#[test]
fn external_occupation_does_not_count_as_rest() {
    let mut initial = fixture();
    initial.people[1].region = Some(1);
    initial.people[1].controlled = true;
    initial.travel = None;
    let snapshot = CoreHostSnapshot {
        core: initial,
        autowork: None,
        tool_wear: None,
        tool_requirements: vec![],
        paused_work_rates: vec![],
        fatigue: Some(timed_fatigue()),
        activity: vec![],
        agriculture: None,
        health: None,
        knowledge: None,
        social: None,
        claims: None,
        food_search: None,
        opinions: None,
        goals: None,
        agreements: None,
        sites: None,
        schedule: None,
    };
    let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
    host.set_controlled_rest(2, true).unwrap();
    host.enable_schedule(1000, 1).unwrap();
    assert_eq!(host.advance_scheduled(500, &[2]).unwrap().failure, None);
    assert_eq!(host.snapshot().activity[1].rest_millis, 0);
    assert_eq!(host.advance_scheduled(250, &[]).unwrap().failure, None);
    assert_eq!(host.snapshot().activity[1].rest_millis, 250);
}

#[test]
fn chosen_departure_progresses_from_its_offset_and_survives_reload() {
    let run = |workers, reload| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.people[1].region = Some(1);
                initial.travel.as_mut().unwrap().journeys.clear();
                initial.physical.custody.containers.push(
                    Container::new(
                        Place::Person(2),
                        10,
                        vec![Lot {
                            item: None,
                            good: 2,
                            quantity: 1,
                            expires_at: None,
                        }],
                    )
                    .unwrap(),
                );
                initial
                    .physical
                    .custody
                    .containers
                    .sort_by_key(|row| row.place);
                let snapshot = CoreHostSnapshot {
                    core: initial,
                    autowork: None,
                    tool_wear: None,
                    tool_requirements: vec![],
                    paused_work_rates: vec![],
                    fatigue: Some(timed_fatigue()),
                    activity: vec![],
                    agriculture: None,
                    health: None,
                    knowledge: None,
                    social: None,
                    claims: None,
                    food_search: None,
                    opinions: None,
                    goals: None,
                    agreements: None,
                    sites: None,
                    schedule: None,
                };
                let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
                host.enable_schedule(1000, 1).unwrap();
                assert_eq!(host.advance_continuous(250, &[]).unwrap().failure, None);
                let before = host.snapshot();
                let choice = game_economy_person::travel::Departure {
                    person: 2,
                    destination: 2,
                };
                assert!(host.depart(&[choice], &[2]).is_err());
                assert_eq!(host.snapshot(), before);
                assert!(host.depart(&[choice], &[]).unwrap()[0].departed);
                assert_eq!(host.advance_continuous(750, &[]).unwrap().failure, None);
                let at_boundary = host.snapshot();
                assert_eq!(
                    at_boundary.core.travel.as_ref().unwrap().journeys[0].progress_millis,
                    750
                );
                assert_eq!(
                    at_boundary.fatigue.as_ref().unwrap().remainders,
                    vec![(2, 500)]
                );
                if reload {
                    let saved = serde_json::to_string(&at_boundary).unwrap();
                    host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                }
                assert_eq!(host.advance_continuous(250, &[]).unwrap().failure, None);
                let result = host.snapshot();
                assert_eq!(result.core.people[1].region, Some(2));
                assert!(
                    result
                        .core
                        .physical
                        .custody
                        .containers
                        .iter()
                        .any(|c| c.place == Place::Person(2) && !c.lots.is_empty())
                );
                result
            })
    };
    assert_eq!(run(1, false), run(6, true));
}

#[test]
#[ignore = "manual concurrent fractional departures benchmark"]
fn core_host_departure_benchmark() {
    use std::hash::{Hash, Hasher};
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(50_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    assert!(count > 1);
    let automatic = std::env::var_os("LEGACY_AUTO_TRAVEL").is_some();
    let acquire = std::env::var_os("LEGACY_FOOD_ACQUIRE").is_some();
    let claimed = std::env::var_os("LEGACY_FOOD_CLAIMS").is_some();
    assert!(!claimed || acquire);
    let food_search = std::env::var_os("LEGACY_FOOD_SEARCH").is_some() || acquire;
    assert!(!(automatic && food_search));
    let mut initial = fixture();
    let template = initial.people[0].clone();
    initial.people = (1..=count)
        .map(|id| {
            let mut p = template.clone();
            p.id = id;
            p
        })
        .collect();
    initial.travel.as_mut().unwrap().journeys.clear();
    if automatic {
        initial.travel.as_mut().unwrap().destinations = (2..=count)
            .map(|person| game_economy_person::travel::DestinationChoice {
                person,
                destination: 2,
                enabled: true,
            })
            .collect();
    }
    initial.nutrition = None;
    initial.pending_food.clear();
    let search = food_search.then(|| {
        initial.physical.goods[1].food = Some(game_production_model::FoodProfile {
            portion: 1,
            hunger_relief: 1,
            nutrients: Default::default(),
            health_effect: 0,
            shelf_days: None,
        });
        if acquire {
            let stock = initial
                .physical
                .custody
                .containers
                .iter_mut()
                .find(|row| row.place == Place::Stock { region: 1, good: 2 })
                .unwrap();
            *stock = Container::new(
                stock.place,
                count,
                vec![Lot {
                    item: None,
                    good: 2,
                    quantity: count / 2,
                    expires_at: None,
                }],
            )
            .unwrap();
        }
        initial.people.iter_mut().skip(1).for_each(|person| {
            person.hunger = 5;
            person.seek_meals = true;
        });
        initial.physical.custody.containers.extend(
            (2..=count).map(|person| Container::new(Place::Person(person), 10, vec![]).unwrap()),
        );
        initial
            .physical
            .custody
            .containers
            .sort_by_key(|row| row.place);
        game_economy::core_interval::CoreFoodSearch {
            claims: claimed.then(|| game_economy_claims::Claims {
                claims: vec![game_economy_claims::Claim {
                    person: 1,
                    region: 1,
                    good: 2,
                }],
                preferences: (2..=count)
                    .map(|person| game_economy_claims::Preference {
                        person,
                        disregard_others: true,
                        assert_outputs: false,
                        assert_built_sites: false,
                        assert_cultivated_fields: false,
                        desperate_hunger: None,
                    })
                    .collect(),
                ..Default::default()
            }),
            search: game_economy::FoodSearch {
                min_hunger: 5,
                min_report_food: 1,
                max_report_age: 20,
                retry_ticks: 3,
                inherit_births: false,
                people: (2..=count)
                    .map(|person| game_economy::FoodSearchPreference {
                        person,
                        enabled: true,
                        next_review: 0,
                        target: None,
                    })
                    .collect(),
            },
            knowledge: Some(game_economy_knowledge::Knowledge {
                max_records: 2,
                retention: vec![],
                minds: (2..=count)
                    .map(|person| game_economy_knowledge::Mind {
                        person,
                        remember_people: false,
                        people_seen: vec![],
                        people_reporting: None,
                        people_reports: vec![],
                        reporting: None,
                        misreport: None,
                        records: vec![game_economy_knowledge::Record {
                            region: 2,
                            food: 10,
                            observed_at: 0,
                            witness: 1,
                            learned_from: Some(1),
                        }],
                    })
                    .collect(),
            }),
        }
    });
    let snapshot = CoreHostSnapshot {
        core: initial,
        autowork: None,
        tool_wear: None,
        tool_requirements: vec![],
        paused_work_rates: vec![],
        fatigue: Some(timed_fatigue()),
        activity: vec![],
        agriculture: None,
        health: None,
        knowledge: None,
        social: None,
        claims: None,
        food_search: search,
        opinions: None,
        goals: None,
        agreements: None,
        sites: None,
        schedule: None,
    };
    let choices: Vec<_> = (2..=count)
        .map(|person| game_economy_person::travel::Departure {
            person,
            destination: 2,
        })
        .collect();
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
            let start = std::time::Instant::now();
            if acquire {
                let results = host.acquire_food_autonomously(&[]).unwrap();
                let elapsed = start.elapsed();
                assert_eq!(results.outcomes.len(), (count - 1) as usize);
                assert_eq!(
                    results.outcomes.iter().filter(|row| row.accepted).count(),
                    (count / 2) as usize
                );
                if claimed {
                    assert_eq!(
                        host.snapshot()
                            .food_search
                            .unwrap()
                            .claims
                            .unwrap()
                            .events
                            .len(),
                        (count / 2) as usize
                    );
                }
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                serde_json::to_vec(&host.snapshot())
                    .unwrap()
                    .hash(&mut hash);
                println!(
                    "workers={workers} acquisition_attempts={} elapsed={:.6} fingerprint={:016x}",
                    count - 1,
                    elapsed.as_secs_f64(),
                    hash.finish()
                );
                return;
            }
            let outcomes = if food_search {
                host.seek_food_autonomously(&[]).unwrap()
            } else if automatic {
                host.depart_autonomously(&[]).unwrap()
            } else {
                host.depart(&choices, &[]).unwrap()
            };
            host.progress(1000).unwrap();
            host.settle().unwrap();
            let elapsed = start.elapsed();
            assert_eq!(outcomes.len(), (count - 1) as usize);
            assert!(outcomes.iter().all(|row| row.departed));
            let result = host.snapshot();
            assert!(result.core.people[1..].iter().all(|p| p.region == Some(2)));
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            serde_json::to_vec(&result).unwrap().hash(&mut hash);
            println!(
                "workers={workers} departures={} elapsed={:.6} fingerprint={:016x}",
                count - 1,
                elapsed.as_secs_f64(),
                hash.finish()
            );
        });
}

#[test]
fn closed_route_is_observed_without_moving_or_consuming_time() {
    let mut initial = fixture();
    initial.people[1].region = Some(1);
    let travel = initial.travel.as_mut().unwrap();
    travel.journeys.clear();
    travel.closures.push(game_economy_person::travel::Closure {
        source: 1,
        destination: 2,
        start: 0,
        end: 1,
    });
    let snapshot = CoreHostSnapshot {
        core: initial,
        autowork: None,
        tool_wear: None,
        tool_requirements: vec![],
        paused_work_rates: vec![],
        fatigue: None,
        activity: vec![],
        agriculture: None,
        health: None,
        knowledge: None,
        social: None,
        claims: None,
        food_search: None,
        opinions: None,
        goals: None,
        agreements: None,
        sites: None,
        schedule: None,
    };
    let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
    let outcome = host
        .depart(
            &[game_economy_person::travel::Departure {
                person: 2,
                destination: 2,
            }],
            &[],
        )
        .unwrap();
    assert!(!outcome[0].departed);
    let result = host.snapshot();
    assert_eq!(result.core.people[1].region, Some(1));
    let travel = result.core.travel.unwrap();
    assert!(travel.journeys.is_empty());
    assert_eq!(travel.observations.len(), 1);
    assert!(!travel.observations[0].open);
    assert_eq!(
        result.core.physical.work_interval.unwrap().elapsed_millis,
        0
    );
}

#[test]
fn scheduled_personal_destinations_respect_work_rest_and_control() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
        let mut initial = fixture();
        let template = initial.people[0].clone();
        initial.people = (1..=6).map(|id| { let mut p = template.clone(); p.id = id; p.controlled = id == 3; p }).collect();
        initial.nutrition = None;
        initial.pending_food.clear();
        let travel = initial.travel.as_mut().unwrap();
        travel.journeys.clear();
        travel.destinations = (1..=6).map(|person| game_economy_person::travel::DestinationChoice {
            person, destination: 2, enabled: person != 5,
        }).collect();
        let mut fatigue = timed_fatigue();
        fatigue.fatigue.people.push(game_economy_fatigue::FatiguePerson { id: 6, fatigue: 2, resting: true });
        let snapshot = CoreHostSnapshot {
            core: initial, autowork: None, tool_wear: None, tool_requirements: vec![],
            paused_work_rates: vec![], fatigue: Some(fatigue), activity: vec![], agriculture: None, health: None, knowledge: None, social: None, claims: None, food_search: None,
            opinions: None, goals: None, agreements: None, sites: None, schedule: None,
        };
        let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
        host.enable_schedule(1000, 1).unwrap();
        let result = host.advance_scheduled(0, &[4]).unwrap();
        assert_eq!(result.failure, None);
        assert!(result.events.iter().any(|e| matches!(e, game_economy::core_interval::CoreRunEvent::Departures(rows) if rows.len() == 1 && rows[0].person == 2 && rows[0].departed)));
        let state = host.snapshot();
        assert_eq!(state.core.travel.as_ref().unwrap().journeys.iter().map(|j| j.person).collect::<Vec<_>>(), vec![2]);
        let saved = serde_json::to_string(&state).unwrap();
        assert_eq!(CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap().snapshot(), state);
        state
    })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn food_search_follows_personal_report_even_when_it_is_stale() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
        let mut initial = fixture();
        initial.people[1].region = Some(1);
        initial.people[1].hunger = 5;
        initial.people[1].seek_meals = true;
        initial.travel.as_mut().unwrap().journeys.clear();
        initial.physical.goods[1].food = Some(game_production_model::FoodProfile {
            portion: 1, hunger_relief: 1, nutrients: Default::default(), health_effect: 0, shelf_days: None,
        });
        initial.physical.custody.containers.push(Container::new(Place::Person(2), 10, vec![]).unwrap());
        initial.physical.custody.containers.sort_by_key(|row| row.place);
        let knowledge = game_economy_knowledge::Knowledge {
            max_records: 4, retention: vec![], minds: vec![game_economy_knowledge::Mind {
                person: 2, remember_people: false, people_seen: vec![], people_reporting: None,
                people_reports: vec![], reporting: None, misreport: None,
                records: vec![game_economy_knowledge::Record {region: 2, food: 10, observed_at: 0, witness: 1, learned_from: Some(1)}],
            }],
        };
        let search = game_economy::FoodSearch {
            min_hunger: 5, min_report_food: 1, max_report_age: 20, retry_ticks: 3,
            inherit_births: false, people: vec![game_economy::FoodSearchPreference {
                person: 2, enabled: true, next_review: 0, target: None,
            }],
        };
        let snapshot = CoreHostSnapshot {
            core: initial, autowork: None, tool_wear: None, tool_requirements: vec![], paused_work_rates: vec![],
            fatigue: None, activity: vec![], agriculture: None, health: None, knowledge: None, social: None, claims: None, food_search: Some(game_economy::core_interval::CoreFoodSearch {search, knowledge: Some(knowledge), claims: None}),
            opinions: None, goals: None, agreements: None, sites: None, schedule: None,
        };
        let mut failing = snapshot.clone();
        failing.core.physical.custody.tick = 1;
        failing.core.pending_food.clear();
        failing.food_search.as_mut().unwrap().search.retry_ticks = u64::MAX;
        let mut failed_host = CoreHost::begin(failing, &[(1, 1)]).unwrap();
        let before = failed_host.snapshot();
        assert!(failed_host.seek_food_autonomously(&[]).is_err());
        assert_eq!(failed_host.snapshot(), before);
        let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
        host.enable_schedule(1000, 1).unwrap();
        let run = host.advance_continuous(500, &[]).unwrap();
        assert_eq!(run.failure, None);
        assert!(run.events.iter().any(|event| matches!(event, game_economy::core_interval::CoreRunEvent::FoodSearch(rows) if rows.len() == 1 && rows[0].person == 2 && rows[0].departed)));
        let state = host.snapshot();
        assert_eq!(state.core.travel.as_ref().unwrap().journeys[0].progress_millis, 500);
        assert_eq!(state.food_search.as_ref().unwrap().search.people[0].target, Some(2));
        let saved = serde_json::to_string(&state).unwrap();
        let mut restored = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
        assert_eq!(restored.snapshot(), state);
        assert_eq!(host.advance_continuous(1000, &[]).unwrap().failure, None);
        assert_eq!(restored.advance_continuous(1000, &[]).unwrap().failure, None);
        let result = host.snapshot();
        assert_eq!(restored.snapshot(), result);
        assert_eq!(result.core.people[1].region, Some(2));
        let records = &result.knowledge.as_ref().unwrap().minds[0].records;
        assert_eq!(records.iter().find(|row| row.region == 2).unwrap().food, 0);
        result
    })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn personal_food_acquisition_contends_for_real_stock_then_eats() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
        let mut initial = fixture();
        initial.people[1].region = Some(1);
        initial.people[1].hunger = 5;
        initial.people[1].seek_meals = true;
        let mut other = initial.people[1].clone();
        other.id = 3;
        initial.people.push(other);
        initial.travel.as_mut().unwrap().journeys.clear();
        initial.nutrition = None;
        initial.pending_food.clear();
        initial.physical.goods[1].food = Some(game_production_model::FoodProfile {
            portion: 1, hunger_relief: 2, nutrients: Default::default(), health_effect: 0, shelf_days: None,
        });
        let stock = initial.physical.custody.containers.iter_mut().find(|row| row.place == Place::Stock {region: 1, good: 2}).unwrap();
        *stock = Container::new(stock.place, 10, vec![Lot {item: None, good: 2, quantity: 1, expires_at: None}]).unwrap();
        initial.physical.custody.containers.extend([2,3].map(|id| Container::new(Place::Person(id), 10, vec![]).unwrap()));
        initial.physical.custody.containers.sort_by_key(|row| row.place);
        let snapshot = CoreHostSnapshot {
            core: initial, autowork: None, tool_wear: None, tool_requirements: vec![], paused_work_rates: vec![],
            fatigue: None, activity: vec![], agriculture: None, health: None, knowledge: None, social: None, claims: None, food_search: Some(game_economy::core_interval::CoreFoodSearch {
                claims: None,
                search: game_economy::FoodSearch {min_hunger: 5, min_report_food: 1, max_report_age: 10, retry_ticks: 1, inherit_births: false, people: vec![]},
                knowledge: Some(game_economy_knowledge::Knowledge {max_records: 1, minds: vec![], retention: vec![]}),
            }), opinions: None, goals: None, agreements: None, sites: None, schedule: None,
        };
        let mut claim_snapshot = snapshot.clone();
        claim_snapshot.food_search.as_mut().unwrap().claims = Some(game_economy_claims::Claims {
            claims: vec![game_economy_claims::Claim {person: 1, region: 1, good: 2}],
            preferences: vec![game_economy_claims::Preference {
                person: 3, disregard_others: true, assert_outputs: false, assert_built_sites: false,
                assert_cultivated_fields: false, desperate_hunger: None,
            }],
            ..Default::default()
        });
        let mut manual = CoreHost::begin(claim_snapshot.clone(), &[(1, 1)]).unwrap();
        manual.progress(1).unwrap();
        let taking = game_lots::Transaction {
            id: 99,
            movements: vec![game_lots::Movement {id: 99, from: Place::Stock {region: 1, good: 2}, to: Some(Place::Person(2)), good: 2, quantity: 1, item: None}],
            creations: vec![],
        };
        assert!(manual.transfer_goods(&[taking]).unwrap().transactions[0].accepted);
        let mut disputed = claim_snapshot.clone();
        let claims = disputed.food_search.as_mut().unwrap().claims.as_mut().unwrap();
        let mut second_preference = claims.preferences[0].clone();
        second_preference.person = 2;
        claims.preferences.push(second_preference);
        let mut disputed = CoreHost::begin(disputed, &[(1, 1)]).unwrap();
        let attempts = disputed.acquire_food_autonomously(&[]).unwrap();
        assert_eq!(attempts.actors.len(), 2);
        assert_eq!(attempts.outcomes.iter().filter(|row| row.accepted).count(), 1);
        assert_eq!(disputed.snapshot().claims.unwrap().events.len(), 1);
        let mut claimed = CoreHost::begin(claim_snapshot, &[(1, 1)]).unwrap();
        let provision = claimed.acquire_food_autonomously(&[]).unwrap();
        assert_eq!(provision.actors, vec![3]);
        assert!(provision.outcomes[0].accepted);
        let claims = claimed.snapshot().claims.unwrap();
        assert_eq!(claims.events.len(), 1);
        assert_eq!((claims.events[0].claimant, claims.events[0].person), (1, 3));
        let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
        host.enable_schedule(500, 1).unwrap();
        let run = host.advance_scheduled(0, &[]).unwrap();
        assert_eq!(run.failure, None);
        assert!(run.events.iter().any(|event| matches!(event, game_economy::core_interval::CoreRunEvent::Provisions(rows) if rows.len() == 2 && rows.iter().filter(|row| row.accepted).count() == 1)));
        let saved = serde_json::to_string(&host.snapshot()).unwrap();
        let mut restored = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
        assert_eq!(host.advance_scheduled(500, &[]).unwrap().failure, None);
        assert_eq!(restored.advance_scheduled(500, &[]).unwrap().failure, None);
        let state = host.snapshot();
        assert_eq!(restored.snapshot(), state);
        assert_eq!(state.core.people[1..].iter().filter(|p| p.hunger == 3).count(), 1);
        state
    })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn autonomous_work_respects_individual_claim_choices_but_explicit_work_can_take_inputs() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.people[1].region = Some(1);
                initial.people[1].willing_to_work = true;
                let mut third = initial.people[1].clone();
                third.id = 3;
                initial.people.push(third);
                initial.travel = None;
                initial.nutrition = None;
                initial.pending_food.clear();
                let stock = initial
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|row| row.place == Place::Stock { region: 1, good: 1 })
                    .unwrap();
                *stock = Container::new(
                    stock.place,
                    10,
                    vec![Lot {
                        item: None,
                        good: 1,
                        quantity: 2,
                        expires_at: None,
                    }],
                )
                .unwrap();
                let claims = game_economy_claims::Claims {
                    claims: vec![game_economy_claims::Claim {
                        person: 1,
                        region: 1,
                        good: 1,
                    }],
                    ..Default::default()
                };
                let auto = game_autowork::Autowork {
                    next_id: 2,
                    requests: None,
                    policies: vec![
                        game_autowork::Policy {
                            person: 2,
                            recipe: 1,
                            enabled: true,
                            disregard_claims: false,
                        },
                        game_autowork::Policy {
                            person: 3,
                            recipe: 1,
                            enabled: true,
                            disregard_claims: true,
                        },
                    ],
                };
                let snapshot = CoreHostSnapshot {
                    core: initial,
                    autowork: Some(auto),
                    tool_wear: None,
                    tool_requirements: vec![],
                    paused_work_rates: vec![],
                    fatigue: None,
                    activity: vec![],
                    agriculture: None,
                    health: None,
                    knowledge: None,
                    social: None,
                    food_search: None,
                    claims: Some(claims),
                    opinions: None,
                    goals: None,
                    agreements: None,
                    sites: None,
                    schedule: None,
                };
                let mut manual = CoreHost::begin(snapshot.clone(), &[(1, 1)]).unwrap();
                assert_eq!(
                    manual
                        .start_chosen_work(&[(
                            Start {
                                id: 2,
                                worker: 2,
                                region: 1,
                                recipe: 1,
                                units: 1
                            },
                            1
                        )])
                        .unwrap(),
                    vec![(2, true)]
                );
                let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
                assert_eq!(host.start_autonomous_work(&[], 1).unwrap(), vec![(2, true)]);
                let state = host.snapshot();
                assert_eq!(state.core.physical.work[1].job.worker, 3);
                let claims = state.claims.as_ref().unwrap();
                assert_eq!(claims.events.len(), 1);
                assert_eq!((claims.events[0].claimant, claims.events[0].person), (1, 3));
                state
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
#[ignore = "manual partial-interval transfer scaling benchmark"]
fn interval_shared_transfer_benchmark() {
    use std::hash::{Hash, Hasher};
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(50_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    assert!(count > 0);
    let mut initial = fixture();
    let autonomous = std::env::var_os("LEGACY_AUTO_MEALS").is_some();
    let eat = autonomous || std::env::var_os("LEGACY_MEALS").is_some();
    if eat {
        initial.physical.goods[0].food = Some(game_production_model::FoodProfile {
            portion: 1,
            hunger_relief: 1,
            nutrients: Default::default(),
            health_effect: 0,
            shelf_days: None,
        });
    }
    let person = initial.people[0].clone();
    initial.people = (1..=count)
        .map(|id| {
            let mut p = person.clone();
            p.id = id;
            p.seek_meals = autonomous;
            p
        })
        .collect();
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        count,
        vec![Lot {
            item: None,
            good: 1,
            quantity: count,
            expires_at: None,
        }],
    )
    .unwrap();
    initial
        .physical
        .custody
        .containers
        .extend((1..=count).map(|id| Container::new(Place::Person(id), 1, vec![]).unwrap()));
    initial
        .physical
        .custody
        .containers
        .sort_by_key(|row| row.place);
    let actions: Vec<_> = (1..=count)
        .map(|id| game_lots::Transaction {
            id,
            movements: vec![game_lots::Movement {
                id,
                from: Place::Stock { region: 1, good: 1 },
                to: Some(Place::Person(id)),
                good: 1,
                quantity: 1,
                item: None,
            }],
            creations: vec![],
        })
        .collect();
    let meals: Vec<_> = (1..=count)
        .map(|id| game_lot_diet::Meal {
            action: id,
            person: id,
            good: 1,
        })
        .collect();
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            let mut meal_samples = Vec::new();
            for round in 0..4 {
                let mut core = CoreInterval::begin(initial.clone(), &[(1, 1)]).unwrap();
                core.progress(300).unwrap();
                let start = std::time::Instant::now();
                let outcomes = core.transfer_goods(0, 300, &actions).unwrap();
                let elapsed = start.elapsed().as_secs_f64();
                assert_eq!(
                    outcomes
                        .transactions
                        .iter()
                        .filter(|row| row.accepted)
                        .count(),
                    count as usize
                );
                if eat {
                    let start = std::time::Instant::now();
                    let outcomes = if autonomous {
                        let (chosen, draft) = core.prepare_autonomous_meals(&[]).unwrap().unwrap();
                        assert_eq!(chosen.len(), count as usize);
                        core.commit_meals(draft).unwrap()
                    } else {
                        core.eat(0, 300, &meals).unwrap()
                    };
                    let elapsed = start.elapsed().as_secs_f64();
                    assert_eq!(
                        outcomes
                            .transactions
                            .iter()
                            .filter(|row| row.accepted)
                            .count(),
                        count as usize
                    );
                    if round > 0 {
                        meal_samples.push(elapsed);
                    }
                }
                if round == 3 {
                    let mut hash = std::collections::hash_map::DefaultHasher::new();
                    serde_json::to_vec(&core.snapshot())
                        .unwrap()
                        .hash(&mut hash);
                    println!("fingerprint={:016x}", hash.finish());
                }
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} transfers={count} elapsed={:.6}",
                samples[1]
            );
            if eat {
                meal_samples.sort_by(f64::total_cmp);
                println!(
                    "workers={workers} meals={count} autonomous={autonomous} elapsed={:.6}",
                    meal_samples[1]
                );
            }
            #[cfg(feature = "profile")]
            if std::env::var_os("LEGACY_PROFILE").is_some() {
                for (name, sample) in game_diagnostics::snapshot() {
                    if name.starts_with("core.meal.") || name.starts_with("physical.") {
                        println!(
                            "{name}: {:.3} ms/call",
                            sample.nanoseconds as f64 / sample.calls as f64 / 1_000_000.0
                        );
                    }
                }
            }
        });
}

#[test]
#[ignore = "manual identified meal owner scaling benchmark"]
fn interval_identified_meals_benchmark() {
    use std::hash::{Hash, Hasher};
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(50_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    assert!(count > 0);
    let mut initial = fixture();
    initial.physical.goods[1].food = Some(game_production_model::FoodProfile {
        portion: 1,
        hunger_relief: 1,
        nutrients: Default::default(),
        health_effect: 0,
        shelf_days: None,
    });
    let person = initial.people[0].clone();
    initial.people = (1..=count)
        .map(|id| {
            let mut row = person.clone();
            row.id = id;
            row
        })
        .collect();
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    initial
        .physical
        .custody
        .containers
        .extend((1..=count).map(|id| {
            Container::new(
                Place::Person(id),
                1,
                vec![Lot {
                    item: Some(id),
                    good: 2,
                    quantity: 1,
                    expires_at: None,
                }],
            )
            .unwrap()
        }));
    initial
        .physical
        .custody
        .containers
        .sort_by_key(|row| row.place);
    let owner = game_economy_work::tool_wear::ToolWear {
        next_item: count + 1,
        rules: vec![game_economy_work::tool_wear::Rule {
            good: 2,
            durability: 10,
            wear: 1,
            repair_good: 1,
            repair_quantity: 1,
        }],
        conditions: (1..=count)
            .map(|item| game_economy_work::tool_wear::Condition {
                item,
                good: 2,
                damage: 0,
            })
            .collect(),
        repairs: vec![],
        events: vec![],
    };
    let meals: Vec<_> = (1..=count)
        .map(|id| game_lot_diet::Meal {
            action: id,
            person: id,
            good: 2,
        })
        .collect();
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut core = CoreInterval::begin(initial.clone(), &[(1, 1)]).unwrap();
                core.progress(300).unwrap();
                let start = std::time::Instant::now();
                let draft = core.prepare_meals(0, 300, &meals).unwrap().unwrap();
                let next_owner = owner
                    .stage_identified_outputs(&[], owner.next_item, draft.item_updates())
                    .unwrap();
                core.commit_meals(draft).unwrap();
                let elapsed = start.elapsed().as_secs_f64();
                assert!(next_owner.conditions.is_empty());
                if round == 3 {
                    let mut hash = std::collections::hash_map::DefaultHasher::new();
                    serde_json::to_vec(&(core.snapshot(), &next_owner))
                        .unwrap()
                        .hash(&mut hash);
                    println!("fingerprint={:016x}", hash.finish());
                }
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} identified_meals={count} elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
#[ignore = "manual saved core host work scaling benchmark"]
fn core_host_autonomous_work_benchmark() {
    use std::hash::{Hash, Hasher};
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(50_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let scheduled = std::env::var_os("LEGACY_SCHEDULED").is_some();
    let continuous = std::env::var_os("LEGACY_CONTINUOUS").is_some();
    let chosen = std::env::var_os("LEGACY_CHOSEN").is_some();
    assert!(
        [scheduled, continuous, chosen]
            .iter()
            .filter(|&&value| value)
            .count()
            <= 1
    );
    assert!(count > 1);
    let mut initial = fixture();
    let person = initial.people[0].clone();
    initial.people = (1..=count)
        .map(|id| {
            let mut row = person.clone();
            row.id = id;
            row.willing_to_work = !chosen;
            row
        })
        .collect();
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        count * 2,
        vec![Lot {
            item: None,
            good: 1,
            quantity: (count - 1) * 2,
            expires_at: None,
        }],
    )
    .unwrap();
    let snapshot = CoreHostSnapshot {
        core: initial,
        tool_wear: None,
        opinions: None,
        goals: None,
        agreements: None,
        sites: None,
        tool_requirements: vec![],
        paused_work_rates: vec![],
        fatigue: None,
        activity: vec![],
        agriculture: None,
        health: None,
        knowledge: None,
        social: None,
        claims: None,
        food_search: None,
        schedule: None,
        autowork: Some(game_autowork::Autowork {
            next_id: 2,
            policies: (2..=count)
                .map(|person| game_autowork::Policy {
                    person,
                    recipe: 1,
                    enabled: true,
                    disregard_claims: false,
                })
                .collect(),
            requests: None,
        }),
    };
    let chosen_requests: Vec<_> = (2..=count)
        .map(|id| {
            (
                Start {
                    id,
                    worker: id,
                    region: 1,
                    recipe: 1,
                    units: 1,
                },
                1,
            )
        })
        .collect();
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut host = CoreHost::begin(snapshot.clone(), &[(1, 1)]).unwrap();
                host.progress(400).unwrap();
                if scheduled || continuous {
                    host.enable_schedule(200, 1).unwrap();
                }
                let start = std::time::Instant::now();
                let outcomes = if continuous {
                    let run = host.advance_continuous(1500, &[]).unwrap();
                    assert_eq!(run.failure, None);
                    assert_eq!((run.elapsed_millis, run.decisions, run.boundaries), (1500, 8, 1));
                    run.events.into_iter().find_map(|event| match event {
                        game_economy::core_interval::CoreRunEvent::Starts(rows) => Some(rows),
                        _ => None,
                    }).unwrap()
                } else if scheduled {
                    let run = host.advance_scheduled(500, &[]).unwrap();
                    assert_eq!(run.failure, None);
                    assert_eq!((run.elapsed_millis, run.decisions), (500, 3));
                    run.events
                        .into_iter()
                        .find_map(|event| match event {
                            game_economy::core_interval::CoreRunEvent::Starts(rows) => Some(rows),
                            _ => None,
                        })
                        .unwrap()
                } else if chosen {
                    host.start_chosen_work(&chosen_requests).unwrap()
                } else {
                    host.start_autonomous_work(&[], 1).unwrap()
                };
                let elapsed = start.elapsed().as_secs_f64();
                assert_eq!(
                    outcomes.iter().filter(|row| row.1).count(),
                    (count - 1) as usize
                );
                if round == 3 {
                    let mut hash = std::collections::hash_map::DefaultHasher::new();
                    serde_json::to_vec(&host.snapshot())
                        .unwrap()
                        .hash(&mut hash);
                    println!("fingerprint={:016x}", hash.finish());
                }
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} host_autonomous_starts={} scheduled={scheduled} continuous={continuous} chosen={chosen} elapsed={:.6}",
                count - 1,
                samples[1]
            );
        });
}

#[test]
#[ignore = "manual scheduled personal work-request scaling benchmark"]
fn core_host_work_requests_benchmark() {
    use game_autowork::requests::{OfferValue, Preference, Request, Requests};
    use std::hash::{Hash, Hasher};
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(50_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let use_offers = std::env::var_os("LEGACY_OFFERS").is_some();
    let request_claims = std::env::var_os("LEGACY_REQUEST_CLAIMS").is_some();
    assert!(count > 1);
    let mut initial = fixture();
    let person = initial.people[0].clone();
    initial.people = (1..=count)
        .map(|id| {
            let mut row = person.clone();
            row.id = id;
            row.willing_to_work = false;
            row
        })
        .collect();
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        count * 2,
        vec![Lot {
            item: None,
            good: 1,
            quantity: (count - 1) * 2,
            expires_at: None,
        }],
    )
    .unwrap();
    let requests = Requests {
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
                offer_value: use_offers.then_some(OfferValue {
                    good: 2,
                    value_per_unit: 1,
                    max_quantity: 1,
                }),
                disregard_claims: request_claims,
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
    };
    let rules = serde_json::from_value(serde_json::json!({"minimum":-10,"maximum":10,"initial":0,"fulfilled_gain":1,"default_loss":1})).unwrap();
    let opinions = game_opinions::Opinions {
        rules,
        sensitivities: Default::default(),
        records: (2..=count)
            .map(|observer| game_opinions::Opinion {
                observer,
                target: 1,
                trust: 0,
                fear: 2,
                obligation: 0,
                affinity: 0,
            })
            .collect(),
        events: vec![],
    };
    let snapshot = CoreHostSnapshot {
        core: initial,
        autowork: Some(game_autowork::Autowork {
            next_id: 2,
            policies: vec![],
            requests: Some(requests),
        }),
        tool_wear: None,
        opinions: (!use_offers).then_some(opinions),
        goals: None,
        agreements: use_offers.then(|| game_agreements::Agreements {
            next_id: count,
            policies: vec![],
            offering: vec![],
            work_offering: vec![],
            bulk_debtors: vec![],
            contracts: (2..=count)
                .map(|creditor| game_agreements::Contract {
                    id: creditor - 1,
                    debtor: 1,
                    creditor,
                    good: 2,
                    quantity: 1,
                    delivered: 0,
                    due: 5,
                    status: game_agreements::Status::Proposed,
                })
                .collect(),
            events: vec![],
        }),
        sites: None,
        tool_requirements: vec![],
        paused_work_rates: vec![],
        fatigue: None,
        activity: vec![],
        agriculture: None,
        health: None,
        knowledge: None,
        social: None,
        claims: request_claims.then(|| game_economy_claims::Claims {
            claims: vec![game_economy_claims::Claim {
                person: 1,
                region: 1,
                good: 1,
            }],
            ..Default::default()
        }),
        food_search: None,
        schedule: None,
    };
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut host = CoreHost::begin(snapshot.clone(), &[(1, 1)]).unwrap();
                let start = std::time::Instant::now();
                let result = host.respond_to_work_requests(&[], 1).unwrap();
                let elapsed = start.elapsed().as_secs_f64();
                assert_eq!(result.events.len(), (count - 1) as usize);
                if round == 3 {
                    let mut hash = std::collections::hash_map::DefaultHasher::new();
                    serde_json::to_vec(&host.snapshot())
                        .unwrap()
                        .hash(&mut hash);
                    println!("fingerprint={:016x}", hash.finish());
                }
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} requests={} elapsed={:.6}",
                count - 1,
                samples[1]
            );
        });
}

#[test]
fn interval_abandonment_stops_progress_and_returns_real_inputs_at_boundary() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut core = CoreInterval::begin(fixture(), &[(1, 1)]).unwrap();
                core.progress(300).unwrap();
                let before = core.snapshot();
                assert!(core.abandon_work(0, 299, &[(1, 1)]).is_err());
                assert!(core.abandon_work(0, 300, &[(1, 2)]).is_err());
                assert!(core.abandon_work(0, 300, &[(1, 1), (1, 1)]).is_err());
                assert_eq!(core.snapshot(), before);
                core.abandon_work(0, 300, &[(1, 1)]).unwrap();
                let stopped = core.snapshot();
                assert_eq!(
                    stopped.physical.work[0].phase,
                    game_lot_runtime::Phase::Returning
                );
                assert!(
                    stopped
                        .physical
                        .work_interval
                        .as_ref()
                        .unwrap()
                        .rates
                        .is_empty()
                );
                let input = stopped
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|row| row.place == Place::JobInput(1))
                    .unwrap();
                assert_eq!(input.lots[0].quantity, 2);
                let saved = serde_json::to_string(&stopped).unwrap();
                let mut core = CoreInterval::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                core.progress(700).unwrap();
                assert_eq!(
                    core.snapshot().physical.work[0].job.remaining,
                    stopped.physical.work[0].job.remaining
                );
                let events = core.settle().unwrap();
                assert_eq!(events.returned, vec![1]);
                assert!(events.finished.is_empty());
                let result = core.snapshot();
                let stock = result
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|row| row.place == Place::Stock { region: 1, good: 1 })
                    .unwrap();
                assert_eq!(stock.lots[0].quantity, 2);
                (result, events)
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn abandoning_interval_work_frees_the_worker_without_freeing_reserved_goods() {
    let mut initial = fixture();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        10,
        vec![Lot {
            item: None,
            good: 1,
            quantity: 2,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
    core.progress(300).unwrap();
    core.abandon_work(0, 300, &[(1, 1)]).unwrap();
    let start = Start {
        id: 2,
        worker: 1,
        region: 1,
        recipe: 1,
        units: 1,
    };
    assert_eq!(
        core.start_work(0, 300, &[(start, 1)]).unwrap(),
        vec![(2, true)]
    );
    let snapshot = core.snapshot();
    assert_eq!(
        snapshot.physical.work[0].phase,
        game_lot_runtime::Phase::Returning
    );
    assert_eq!(
        snapshot.physical.work[1].phase,
        game_lot_runtime::Phase::Working
    );
    assert_eq!(snapshot.physical.work_interval.unwrap().rates, vec![(2, 1)]);
    assert_eq!(
        snapshot
            .physical
            .custody
            .containers
            .iter()
            .find(|row| row.place == Place::JobInput(1))
            .unwrap()
            .used,
        2
    );
    assert_eq!(
        snapshot
            .physical
            .custody
            .containers
            .iter()
            .find(|row| row.place == Place::JobInput(2))
            .unwrap()
            .used,
        2
    );
}

#[test]
fn local_interval_transfer_moves_real_custody_and_preserves_inflight_work() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.people[1].region = Some(1);
                initial.travel = None;
                initial.physical.custody.containers.extend([
                    Container::new(
                        Place::Person(1),
                        5,
                        vec![Lot {
                            item: None,
                            good: 1,
                            quantity: 1,
                            expires_at: None,
                        }],
                    )
                    .unwrap(),
                    Container::new(Place::Person(2), 5, vec![]).unwrap(),
                ]);
                initial
                    .physical
                    .custody
                    .containers
                    .sort_by_key(|row| row.place);
                let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
                core.progress(300).unwrap();
                let action = game_lots::Transaction {
                    id: 1,
                    movements: vec![game_lots::Movement {
                        id: 1,
                        from: Place::Person(1),
                        to: Some(Place::Person(2)),
                        good: 1,
                        quantity: 1,
                        item: None,
                    }],
                    creations: vec![],
                };
                let before = core.snapshot();
                assert!(
                    core.transfer_goods(0, 299, std::slice::from_ref(&action))
                        .is_err()
                );
                let mut invalid = action.clone();
                invalid.movements[0].to = Some(Place::Stock { region: 2, good: 1 });
                assert!(core.transfer_goods(0, 300, &[invalid]).is_err());
                let mut invalid = action.clone();
                invalid.movements[0].to = None;
                assert!(core.transfer_goods(0, 300, &[invalid]).is_err());
                let mut invalid = action.clone();
                invalid.creations.push(game_lots::Creation {
                    id: 2,
                    to: Place::Person(2),
                    lot: Lot {
                        item: None,
                        good: 1,
                        quantity: 1,
                        expires_at: None,
                    },
                });
                assert!(core.transfer_goods(0, 300, &[invalid]).is_err());
                assert_eq!(core.snapshot(), before);
                assert!(
                    core.transfer_goods(0, 300, std::slice::from_ref(&action))
                        .unwrap()
                        .transactions[0]
                        .accepted
                );
                assert!(!core.transfer_goods(0, 300, &[action]).unwrap().transactions[0].accepted);
                let saved = core.snapshot();
                assert_eq!(
                    saved.physical.work_interval.as_ref().unwrap().rates,
                    vec![(1, 1)]
                );
                assert_eq!(
                    saved
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Person(2))
                        .unwrap()
                        .used,
                    1
                );
                let mut core = CoreInterval::resume(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                core.progress(700).unwrap();
                let events = core.settle().unwrap();
                assert_eq!(events.finished, vec![1]);
                (core.snapshot(), events)
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn interval_meals_consume_real_food_and_apply_effects_at_the_right_time() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.physical.goods[1].food = Some(game_production_model::FoodProfile {
                    portion: 1,
                    hunger_relief: 1,
                    nutrients: Default::default(),
                    health_effect: -3,
                    shelf_days: None,
                });
                initial.physical.custody.containers.extend([
                    Container::new(
                        Place::Person(1),
                        2,
                        vec![Lot {
                            item: None,
                            good: 2,
                            quantity: 1,
                            expires_at: None,
                        }],
                    )
                    .unwrap(),
                    Container::new(Place::Person(2), 2, vec![]).unwrap(),
                ]);
                initial
                    .physical
                    .custody
                    .containers
                    .sort_by_key(|row| row.place);
                initial.nutrition = None;
                initial.pending_food.clear();
                let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
                core.progress(500).unwrap();
                let meals = [
                    game_lot_diet::Meal {
                        action: 1,
                        person: 1,
                        good: 2,
                    },
                    game_lot_diet::Meal {
                        action: 2,
                        person: 2,
                        good: 2,
                    },
                ];
                let before = core.snapshot();
                assert!(core.eat(0, 499, &meals).is_err());
                assert!(
                    core.eat(
                        0,
                        500,
                        &[game_lot_diet::Meal {
                            action: 3,
                            person: 1,
                            good: 1
                        }]
                    )
                    .is_err()
                );
                assert_eq!(core.snapshot(), before);
                let outcomes = core.eat(0, 500, &meals).unwrap();
                assert!(outcomes.transactions[0].accepted);
                assert!(!outcomes.transactions[1].accepted);
                let saved = core.snapshot();
                assert_eq!(
                    (saved.people[0].hunger, saved.people[0].hunger_remainder),
                    (1, 500)
                );
                assert_eq!(saved.people[0].health, 100);
                assert_eq!(saved.pending_food.len(), 1);
                assert_eq!(saved.pending_food[0].at, 1);
                assert_eq!(
                    saved
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Person(1))
                        .unwrap()
                        .used,
                    0
                );
                let mut core = CoreInterval::resume(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                core.progress(500).unwrap();
                core.settle().unwrap();
                let result = core.snapshot();
                assert_eq!(result.people[0].hunger, 2);
                assert_eq!(result.people[0].health, 97);
                assert!(result.pending_food.is_empty());
                result
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn identified_meals_stage_item_removal_with_hunger_and_later_production() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.people[0].seek_meals = true;
                initial.rules.meal_threshold = 2;
                initial.physical.goods[1].food = Some(game_production_model::FoodProfile {
                    portion: 1,
                    hunger_relief: 1,
                    nutrients: Default::default(),
                    health_effect: 0,
                    shelf_days: None,
                });
                initial.physical.custody.containers.push(
                    Container::new(
                        Place::Person(1),
                        2,
                        vec![Lot {
                            item: Some(10),
                            good: 2,
                            quantity: 1,
                            expires_at: None,
                        }],
                    )
                    .unwrap(),
                );
                initial
                    .physical
                    .custody
                    .containers
                    .sort_by_key(|row| row.place);
                let mut owner = game_economy_work::tool_wear::ToolWear {
                    next_item: 11,
                    rules: vec![game_economy_work::tool_wear::Rule {
                        good: 2,
                        durability: 10,
                        wear: 1,
                        repair_good: 1,
                        repair_quantity: 1,
                    }],
                    conditions: vec![game_economy_work::tool_wear::Condition {
                        item: 10,
                        good: 2,
                        damage: 0,
                    }],
                    repairs: vec![],
                    events: vec![],
                };
                let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
                core.progress(400).unwrap();
                let meal = [game_lot_diet::Meal {
                    action: 1,
                    person: 1,
                    good: 2,
                }];
                let before = core.snapshot();
                assert!(core.eat(0, 400, &meal).is_err());
                assert_eq!(core.snapshot(), before);
                let stale = core.prepare_meals(0, 400, &meal).unwrap().unwrap();
                let (chosen, draft) = core.prepare_autonomous_meals(&[]).unwrap().unwrap();
                assert_eq!(
                    chosen.iter().map(|meal| meal.person).collect::<Vec<_>>(),
                    vec![1]
                );
                assert_eq!(draft.item_updates(), &[(10, None)]);
                let next_owner = owner
                    .stage_identified_outputs(&[], owner.next_item, draft.item_updates())
                    .unwrap();
                core.validate_meals(&draft).unwrap();
                assert!(core.commit_meals(draft).unwrap().transactions[0].accepted);
                owner = next_owner;
                assert!(owner.conditions.is_empty());
                let after = core.snapshot();
                assert_eq!(after.people[0].hunger, 1);
                assert!(core.commit_meals(stale).is_err());
                assert_eq!(core.snapshot(), after);
                let saved = serde_json::to_string(&(after, owner)).unwrap();
                let (snapshot, mut owner): (CoreSnapshot, game_economy_work::tool_wear::ToolWear) =
                    serde_json::from_str(&saved).unwrap();
                let mut core = CoreInterval::resume(snapshot).unwrap();
                core.progress(600).unwrap();
                let draft = core
                    .prepare_settlement(Some(game_lot_runtime::IdentifiedProduction {
                        goods: &[2],
                        next_item: owner.next_item,
                    }))
                    .unwrap();
                let next_owner = owner
                    .stage_identified_outputs(
                        draft.produced_items(),
                        draft.next_produced_item().unwrap(),
                        draft.item_updates(),
                    )
                    .unwrap();
                core.commit_settlement(draft).unwrap();
                owner = next_owner;
                let result = core.snapshot();
                let physical = Runtime::compile(result.physical.clone()).unwrap();
                assert_eq!(physical.item_place(10), None);
                assert_eq!(physical.item_place(11), Some(Place::JobOutput(1)));
                owner
                    .validate(
                        &physical,
                        &[1, 2].into_iter().collect(),
                        &[1, 2].into_iter().collect(),
                        1,
                    )
                    .unwrap();
                (result, owner)
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn autonomous_interval_meals_follow_personal_preferences_and_occupancy() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.travel = None;
                initial.nutrition = None;
                initial.pending_food.clear();
                initial.rules.meal_threshold = 2;
                initial.physical.goods[1].food = Some(game_production_model::FoodProfile {
                    portion: 2,
                    hunger_relief: 1,
                    nutrients: Default::default(),
                    health_effect: 0,
                    shelf_days: None,
                });
                let mut people: Vec<_> = (1..=5)
                    .map(|id| {
                        let mut p = initial.people[0].clone();
                        p.id = id;
                        p.seek_meals = true;
                        p
                    })
                    .collect();
                people[1].controlled = true;
                people[2].seek_meals = false;
                people[3].hunger = 0;
                initial.people = people;
                initial
                    .physical
                    .custody
                    .containers
                    .extend((1..=5).map(|id| {
                        Container::new(
                            Place::Person(id),
                            2,
                            vec![
                                Lot {
                                    item: None,
                                    good: 2,
                                    quantity: 1,
                                    expires_at: Some(1),
                                },
                                Lot {
                                    item: None,
                                    good: 2,
                                    quantity: 1,
                                    expires_at: Some(3),
                                },
                            ],
                        )
                        .unwrap()
                    }));
                initial
                    .physical
                    .custody
                    .containers
                    .sort_by_key(|row| row.place);
                let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
                core.progress(400).unwrap();
                let before = core.snapshot();
                assert!(core.prepare_autonomous_meals(&[2, 1]).is_err());
                assert_eq!(core.snapshot(), before);
                let (meals, draft) = core.prepare_autonomous_meals(&[1]).unwrap().unwrap();
                assert_eq!(
                    meals
                        .iter()
                        .map(|meal| (meal.person, meal.good))
                        .collect::<Vec<_>>(),
                    vec![(5, 2)]
                );
                assert!(draft.item_updates().is_empty());
                assert!(core.commit_meals(draft).unwrap().transactions[0].accepted);
                let after = core.snapshot();
                assert_eq!(after.people[4].hunger, 1);
                assert_eq!(after.people[0].hunger, 2);
                assert!(core.prepare_autonomous_meals(&[1]).unwrap().is_none());
                let saved = serde_json::to_string(&after).unwrap();
                let resumed = CoreInterval::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                assert!(resumed.prepare_autonomous_meals(&[1]).unwrap().is_none());
                resumed.snapshot()
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn autonomous_interval_work_respects_personal_choices_and_real_stock() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.travel = None;
                initial.nutrition = None;
                initial.pending_food.clear();
                initial.people = (1..=7)
                    .map(|id| {
                        let mut person = initial.people[0].clone();
                        person.id = id;
                        person.willing_to_work = true;
                        person
                    })
                    .collect();
                initial.people[2].willing_to_work = false;
                initial.people[3].controlled = true;
                initial.people[5].hunger = 11;
                let stock = initial
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|row| row.place == Place::Stock { region: 1, good: 1 })
                    .unwrap();
                *stock = Container::new(
                    stock.place,
                    10,
                    vec![Lot {
                        item: None,
                        good: 1,
                        quantity: 2,
                        expires_at: None,
                    }],
                )
                .unwrap();
                let mut owner = game_autowork::Autowork {
                    next_id: 2,
                    policies: (2..=7)
                        .map(|person| game_autowork::Policy {
                            person,
                            recipe: 1,
                            enabled: person != 5,
                            disregard_claims: false,
                        })
                        .collect(),
                    requests: None,
                };
                let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
                core.progress(300).unwrap();
                let before = core.snapshot();
                let owner_before = owner.clone();
                assert!(
                    core.start_autonomous_work(0, 299, &mut owner, &[], 2)
                        .is_err()
                );
                assert!(
                    core.start_autonomous_work(0, 300, &mut owner, &[7, 2], 2)
                        .is_err()
                );
                assert_eq!(core.snapshot(), before);
                assert_eq!(owner, owner_before);
                let mut invalid = owner.clone();
                invalid.policies.push(invalid.policies[0].clone());
                let invalid_before = invalid.clone();
                assert!(
                    core.start_autonomous_work(0, 300, &mut invalid, &[], 2)
                        .is_err()
                );
                assert_eq!(invalid, invalid_before);
                assert_eq!(core.snapshot(), before);
                assert_eq!(
                    core.start_autonomous_work(0, 300, &mut owner, &[], 2)
                        .unwrap(),
                    vec![(2, true)]
                );
                assert_eq!(owner.next_id, 3);
                let after = core.snapshot();
                assert_eq!(
                    after
                        .physical
                        .work
                        .iter()
                        .map(|row| row.job.worker)
                        .collect::<Vec<_>>(),
                    vec![1, 2]
                );
                assert_eq!(
                    after
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
                        .unwrap()
                        .used,
                    0
                );
                let saved = serde_json::to_string(&(after, owner)).unwrap();
                let (snapshot, owner): (CoreSnapshot, game_autowork::Autowork) =
                    serde_json::from_str(&saved).unwrap();
                let mut core = CoreInterval::resume(snapshot).unwrap();
                core.progress(500).unwrap();
                assert_eq!(
                    core.publish_early_work().unwrap().unwrap().finished,
                    vec![2]
                );
                (core.snapshot(), owner)
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn opening_offset_actions_do_not_receive_prior_progress() {
    let mut initial = fixture();
    initial.people[1].region = Some(1);
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        10,
        vec![Lot {
            item: None,
            good: 1,
            quantity: 2,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
    let start = Start {
        id: 2,
        worker: 2,
        region: 1,
        recipe: 1,
        units: 1,
    };
    assert_eq!(
        core.start_work(0, 0, &[(start, 2)]).unwrap(),
        vec![(2, true)]
    );
    let snapshot = core.snapshot();
    assert_eq!(snapshot.physical.work[1].job.remaining, 1);
    assert_eq!(snapshot.physical.work[1].progress_remainder, 0);
    core.progress(250).unwrap();
    assert_eq!(core.snapshot().physical.work[1].job.remaining, 1);
    assert_eq!(core.snapshot().physical.work[1].progress_remainder, 500);
}

#[test]
fn saved_core_host_coordinates_personal_work_meals_and_tool_ownership() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.people[0].willing_to_work = true;
                initial.people[0].seek_meals = true;
                initial.people[1].region = Some(1);
                initial.people[1].willing_to_work = true;
                initial.people[1].seek_meals = true;
                initial.travel = None;
                initial.nutrition = None;
                initial.pending_food.clear();
                initial.rules.meal_threshold = 2;
                initial.physical.goods[0].food = Some(game_production_model::FoodProfile {
                    portion: 1,
                    hunger_relief: 1,
                    nutrients: Default::default(),
                    health_effect: 0,
                    shelf_days: None,
                });
                let stock = initial
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|row| row.place == Place::Stock { region: 1, good: 1 })
                    .unwrap();
                *stock = Container::new(
                    stock.place,
                    10,
                    vec![Lot {
                        item: None,
                        good: 1,
                        quantity: 2,
                        expires_at: None,
                    }],
                )
                .unwrap();
                initial
                    .physical
                    .custody
                    .containers
                    .extend((1..=2).map(|id| {
                        Container::new(
                            Place::Person(id),
                            2,
                            vec![Lot {
                                item: None,
                                good: 1,
                                quantity: 1,
                                expires_at: None,
                            }],
                        )
                        .unwrap()
                    }));
                initial
                    .physical
                    .custody
                    .containers
                    .sort_by_key(|row| row.place);
                let snapshot = CoreHostSnapshot {
                    core: initial,
                    opinions: None,
                    goals: None,
                    agreements: None,
                    sites: None,
                    tool_requirements: vec![],
                    paused_work_rates: vec![],
                    fatigue: None,
                    activity: vec![],
                    agriculture: None,
                    health: None,
                    knowledge: None,
                    social: None,
                    claims: None,
                    food_search: None,
                    schedule: None,
                    autowork: Some(game_autowork::Autowork {
                        next_id: 2,
                        policies: vec![game_autowork::Policy {
                            person: 2,
                            recipe: 1,
                            enabled: true,
                            disregard_claims: false,
                        }],
                        requests: None,
                    }),
                    tool_wear: Some(game_economy_work::tool_wear::ToolWear {
                        next_item: 10,
                        rules: vec![game_economy_work::tool_wear::Rule {
                            good: 2,
                            durability: 10,
                            wear: 1,
                            repair_good: 1,
                            repair_quantity: 1,
                        }],
                        conditions: vec![],
                        repairs: vec![],
                        events: vec![],
                    }),
                };
                let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
                host.progress(300).unwrap();
                assert_eq!(
                    host.eat_autonomously(&[])
                        .unwrap()
                        .iter()
                        .map(|meal| meal.person)
                        .collect::<Vec<_>>(),
                    vec![2]
                );
                assert!(host.start_autonomous_work(&[2], 2).unwrap().is_empty());
                let saved = serde_json::to_string(&host.snapshot()).unwrap();
                let mut host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                host.progress(1).unwrap();
                assert_eq!(host.start_autonomous_work(&[], 2).unwrap(), vec![(2, true)]);
                host.progress(500).unwrap();
                assert_eq!(
                    host.publish_early_work().unwrap().unwrap().finished,
                    vec![2]
                );
                let saved = serde_json::to_string(&host.snapshot()).unwrap();
                let mut host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                assert_eq!(
                    host.snapshot().tool_wear.as_ref().unwrap().conditions[0].item,
                    10
                );
                host.progress(199).unwrap();
                let events = host.settle().unwrap();
                assert_eq!(events.finished, vec![1]);
                assert_eq!(events.delivered, vec![2]);
                let result = host.snapshot();
                let physical = Runtime::compile(result.core.physical.clone()).unwrap();
                assert_eq!(
                    physical.item_place(10),
                    Some(Place::Stock { region: 1, good: 2 })
                );
                assert_eq!(physical.item_place(11), Some(Place::JobOutput(1)));
                let owner = result.tool_wear.as_ref().unwrap();
                owner
                    .validate(
                        &physical,
                        &[1, 2].into_iter().collect(),
                        &[1, 2].into_iter().collect(),
                        1,
                    )
                    .unwrap();
                assert_eq!(result.autowork.as_ref().unwrap().next_id, 3);
                result
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn externally_chosen_work_does_not_require_willingness_and_can_be_abandoned() {
    let mut initial = fixture();
    initial.people[1].region = Some(1);
    initial.people[1].willing_to_work = false;
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        10,
        vec![Lot {
            item: None,
            good: 1,
            quantity: 2,
            expires_at: None,
        }],
    )
    .unwrap();
    let owner = game_autowork::Autowork {
        next_id: 2,
        policies: vec![game_autowork::Policy {
            person: 2,
            recipe: 1,
            enabled: true,
            disregard_claims: false,
        }],
        requests: None,
    };
    let mut host = CoreHost::begin(
        CoreHostSnapshot {
            core: initial,
            autowork: Some(owner),
            tool_wear: None,
            opinions: None,
            goals: None,
            agreements: None,
            sites: None,
            tool_requirements: vec![],
            paused_work_rates: vec![],
            fatigue: None,
            activity: vec![],
            agriculture: None,
            health: None,
            knowledge: None,
            social: None,
            claims: None,
            food_search: None,
            schedule: None,
        },
        &[(1, 1)],
    )
    .unwrap();
    host.progress(300).unwrap();
    assert!(host.start_autonomous_work(&[], 1).unwrap().is_empty());
    let request = Start {
        id: 2,
        worker: 2,
        region: 1,
        recipe: 1,
        units: 1,
    };
    assert_eq!(
        host.start_chosen_work(&[(request, 1)]).unwrap(),
        vec![(2, true)]
    );
    host.abandon_work(&[(2, 2)]).unwrap();
    let saved = serde_json::to_string(&host.snapshot()).unwrap();
    let mut host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
    host.progress(700).unwrap();
    let events = host.settle().unwrap();
    assert_eq!(events.returned, vec![2]);
    assert!(!events.finished.contains(&2));
    let snapshot = host.snapshot();
    assert_eq!(
        snapshot
            .core
            .physical
            .custody
            .containers
            .iter()
            .find(|row| row.place == Place::Stock { region: 1, good: 1 })
            .unwrap()
            .used,
        2
    );
}

#[test]
fn scheduled_requests_use_directed_fear_and_obligation_without_binding_work() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                use game_autowork::requests::{Outcome, Preference, Request, Requests};
                let mut initial = fixture();
                initial.travel = None;
                initial.nutrition = None;
                initial.pending_food.clear();
                initial.people = (1..=6)
                    .map(|id| {
                        let mut person = initial.people[0].clone();
                        person.id = id;
                        person.region = Some(if id == 5 { 2 } else { 1 });
                        person.willing_to_work = false;
                        person
                    })
                    .collect();
                let stock = initial
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|row| row.place == Place::Stock { region: 1, good: 1 })
                    .unwrap();
                *stock = Container::new(
                    stock.place,
                    10,
                    vec![Lot {
                        item: None,
                        good: 1,
                        quantity: 4,
                        expires_at: None,
                    }],
                )
                .unwrap();
                let preferences = (2..=4)
                    .map(|person| Preference {
                        person,
                        effort: 5,
                        help_value: 0,
                        affinity_weight: 0,
                        trust_weight: 0,
                        offer_value: None,
                        disregard_claims: false,
                    })
                    .collect();
                let request = |id, requester, worker| Request {
                    id,
                    requester,
                    worker,
                    region: 1,
                    recipe: 1,
                    created_at: 0,
                    expires: 5,
                };
                let requests = Requests {
                    next_id: 5,
                    inherit_births: false,
                    memory_limit: 4,
                    work_links: vec![],
                    memories: vec![],
                    preferences,
                    askers: vec![],
                    pending: vec![
                        request(1, 5, 2),
                        request(2, 1, 2),
                        request(3, 6, 3),
                        request(4, 1, 4),
                    ],
                    events: vec![],
                };
                let auto = game_autowork::Autowork {
                    next_id: 2,
                    policies: vec![],
                    requests: Some(requests),
                };
                let opinions: game_opinions::Opinions = serde_json::from_value(serde_json::json!({
            "rules": {"minimum":-10,"maximum":10,"initial":0,"fulfilled_gain":1,"default_loss":1},
            "records": [
                {"observer":2,"target":1,"trust":0,"fear":10},
                {"observer":4,"target":1,"trust":0,"obligation":10}
            ]
        })).unwrap();
                let snapshot = CoreHostSnapshot {
                    core: initial,
                    autowork: Some(auto),
                    tool_wear: None,
                    opinions: Some(opinions.clone()),
                    goals: None,
                    agreements: None,
                    sites: None,
                    tool_requirements: vec![],
                    paused_work_rates: vec![],
                    fatigue: None,
                    activity: vec![],
                    agriculture: None,
                    health: None,
                    knowledge: None,
                    social: None,
                    claims: None,
                    food_search: None,
                    schedule: None,
                };
                let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
                host.enable_schedule(500, 1).unwrap();
                let run = host.advance_scheduled(0, &[]).unwrap();
                assert_eq!(run.failure, None);
                let events = run
                    .events
                    .iter()
                    .find_map(|event| match event {
                        game_economy::core_interval::CoreRunEvent::Requests(events) => Some(events),
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(
                    events
                        .iter()
                        .map(|event| (event.request, event.outcome.clone()))
                        .collect::<Vec<_>>(),
                    vec![
                        (2, Outcome::Started { job: 2 }),
                        (3, Outcome::Refused),
                        (4, Outcome::Started { job: 3 })
                    ]
                );
                let started = host.snapshot();
                assert_eq!(started.opinions.as_ref(), Some(&opinions));
                assert_eq!(
                    started
                        .autowork
                        .as_ref()
                        .unwrap()
                        .requests
                        .as_ref()
                        .unwrap()
                        .pending
                        .iter()
                        .map(|request| request.id)
                        .collect::<Vec<_>>(),
                    vec![1]
                );
                let saved = serde_json::to_string(&started).unwrap();
                let mut host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                assert_eq!(host.advance_scheduled(100, &[]).unwrap().failure, None);
                host.abandon_work(&[(3, 4)]).unwrap();
                let after_stop = host.snapshot();
                let owner = after_stop
                    .autowork
                    .as_ref()
                    .unwrap()
                    .requests
                    .as_ref()
                    .unwrap();
                assert!(!owner.work_links.iter().any(|link| link.job == 3));
                assert!(owner.memories.iter().any(|memory| memory.observer == 1
                    && memory.worker == 4
                    && memory.failures == 1));
                let saved = serde_json::to_string(&after_stop).unwrap();
                let mut host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                let run = host.advance_continuous(900, &[]).unwrap();
                assert_eq!(run.failure, None);
                let snapshot = host.snapshot();
                let owner = snapshot
                    .autowork
                    .as_ref()
                    .unwrap()
                    .requests
                    .as_ref()
                    .unwrap();
                assert!(owner.memories.iter().any(|memory| memory.observer == 1
                    && memory.worker == 2
                    && memory.successes == 1));
                assert_eq!(snapshot.opinions.as_ref(), Some(&opinions));
                snapshot
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn willing_request_response_with_missing_stock_remains_pending() {
    use game_autowork::requests::{Preference, Request, Requests};
    let mut initial = fixture();
    initial.people[1].region = Some(1);
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    let requests = Requests {
        next_id: 2,
        inherit_births: false,
        memory_limit: 4,
        work_links: vec![],
        memories: vec![],
        preferences: vec![Preference {
            person: 2,
            effort: 0,
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
            expires: 5,
        }],
        events: vec![],
    };
    let auto = game_autowork::Autowork {
        next_id: 2,
        policies: vec![],
        requests: Some(requests),
    };
    let mut host = CoreHost::begin(
        CoreHostSnapshot {
            core: initial,
            autowork: Some(auto),
            tool_wear: None,
            opinions: None,
            goals: None,
            agreements: None,
            sites: None,
            tool_requirements: vec![],
            paused_work_rates: vec![],
            fatigue: None,
            activity: vec![],
            agriculture: None,
            health: None,
            knowledge: None,
            social: None,
            claims: None,
            food_search: None,
            schedule: None,
        },
        &[(1, 1)],
    )
    .unwrap();
    let before = host.snapshot();
    let result = host.respond_to_work_requests(&[], 1).unwrap();
    assert_eq!(result.actors, vec![2]);
    assert!(result.events.is_empty());
    let after = host.snapshot();
    assert_eq!(after.core, before.core);
    assert_eq!(after.autowork.as_ref().unwrap().next_id, 3);
    let owner = after.autowork.as_ref().unwrap().requests.as_ref().unwrap();
    assert_eq!(owner.pending.len(), 1);
    assert!(owner.memories.is_empty());
    assert!(owner.work_links.is_empty());
}

#[test]
fn contested_request_reservations_do_not_turn_shortage_into_refusal() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                use game_autowork::requests::{Outcome, Preference, Request, Requests};
                let mut initial = fixture();
                initial.people[1].region = Some(1);
                let mut third = initial.people[0].clone();
                third.id = 3;
                initial.people.push(third);
                initial.travel = None;
                initial.nutrition = None;
                initial.pending_food.clear();
                let stock = initial
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|row| row.place == Place::Stock { region: 1, good: 1 })
                    .unwrap();
                *stock = Container::new(
                    stock.place,
                    10,
                    vec![Lot {
                        item: None,
                        good: 1,
                        quantity: 2,
                        expires_at: None,
                    }],
                )
                .unwrap();
                let requests = Requests {
                    next_id: 3,
                    inherit_births: false,
                    memory_limit: 4,
                    work_links: vec![],
                    memories: vec![],
                    preferences: (2..=3)
                        .map(|person| Preference {
                            person,
                            effort: 0,
                            help_value: 1,
                            affinity_weight: 0,
                            trust_weight: 0,
                            offer_value: None,
                            disregard_claims: false,
                        })
                        .collect(),
                    askers: vec![],
                    pending: (2..=3)
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
                };
                let auto = game_autowork::Autowork {
                    next_id: 2,
                    policies: vec![],
                    requests: Some(requests),
                };
                let mut host = CoreHost::begin(
                    CoreHostSnapshot {
                        core: initial,
                        autowork: Some(auto),
                        tool_wear: None,
                        opinions: None,
                        goals: None,
                        agreements: None,
                        sites: None,
                        tool_requirements: vec![],
                        paused_work_rates: vec![],
                        fatigue: None,
                        activity: vec![],
                        agriculture: None,
                        health: None,
                        knowledge: None,
                        social: None,
                        claims: None,
                        food_search: None,
                        schedule: None,
                    },
                    &[(1, 1)],
                )
                .unwrap();
                let result = host.respond_to_work_requests(&[], 1).unwrap();
                assert_eq!(result.actors, vec![2, 3]);
                assert_eq!(result.events.len(), 1);
                assert!(matches!(result.events[0].outcome, Outcome::Started { .. }));
                let snapshot = host.snapshot();
                let owner = snapshot.autowork.as_ref().unwrap();
                assert_eq!(owner.next_id, 4);
                assert_eq!(owner.requests.as_ref().unwrap().pending.len(), 1);
                assert!(owner.requests.as_ref().unwrap().memories.is_empty());
                assert_eq!(owner.requests.as_ref().unwrap().work_links.len(), 1);
                snapshot
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn future_request_does_not_hide_an_existing_local_request() {
    use game_autowork::requests::{Outcome, Preference, Request, Requests};
    let mut initial = fixture();
    initial.people[1].region = Some(1);
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        10,
        vec![Lot {
            item: None,
            good: 1,
            quantity: 2,
            expires_at: None,
        }],
    )
    .unwrap();
    let request = |id, created_at| Request {
        id,
        requester: 1,
        worker: 2,
        region: 1,
        recipe: 1,
        created_at,
        expires: 5,
    };
    let requests = Requests {
        next_id: 3,
        inherit_births: false,
        memory_limit: 0,
        work_links: vec![],
        memories: vec![],
        preferences: vec![Preference {
            person: 2,
            effort: 0,
            help_value: 1,
            affinity_weight: 0,
            trust_weight: 0,
            offer_value: None,
            disregard_claims: false,
        }],
        askers: vec![],
        pending: vec![request(1, 1), request(2, 0)],
        events: vec![],
    };
    let auto = game_autowork::Autowork {
        next_id: 2,
        policies: vec![],
        requests: Some(requests),
    };
    let mut host = CoreHost::begin(
        CoreHostSnapshot {
            core: initial,
            autowork: Some(auto),
            tool_wear: None,
            opinions: None,
            goals: None,
            agreements: None,
            sites: None,
            tool_requirements: vec![],
            paused_work_rates: vec![],
            fatigue: None,
            activity: vec![],
            agriculture: None,
            health: None,
            knowledge: None,
            social: None,
            claims: None,
            food_search: None,
            schedule: None,
        },
        &[(1, 1)],
    )
    .unwrap();
    let result = host.respond_to_work_requests(&[], 1).unwrap();
    assert_eq!(result.events.len(), 1);
    assert_eq!(
        (result.events[0].request, &result.events[0].outcome),
        (2, &Outcome::Started { job: 2 })
    );
    assert_eq!(
        host.snapshot().autowork.unwrap().requests.unwrap().pending[0].id,
        1
    );
}

#[test]
fn requested_work_follows_the_workers_own_claim_preference() {
    use game_autowork::requests::{Outcome, Preference, Request, Requests};
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.people[1].region = Some(1);
                initial.travel = None;
                initial.nutrition = None;
                initial.pending_food.clear();
                let stock = initial
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|row| row.place == Place::Stock { region: 1, good: 1 })
                    .unwrap();
                *stock = Container::new(
                    stock.place,
                    10,
                    vec![Lot {
                        item: None,
                        good: 1,
                        quantity: 2,
                        expires_at: None,
                    }],
                )
                .unwrap();
                let requests = Requests {
                    next_id: 2,
                    inherit_births: false,
                    memory_limit: 4,
                    work_links: vec![],
                    memories: vec![],
                    preferences: vec![Preference {
                        person: 2,
                        effort: 0,
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
                        expires: 5,
                    }],
                    events: vec![],
                };
                let snapshot = CoreHostSnapshot {
                    core: initial,
                    autowork: Some(game_autowork::Autowork {
                        next_id: 2,
                        policies: vec![],
                        requests: Some(requests),
                    }),
                    tool_wear: None,
                    tool_requirements: vec![],
                    paused_work_rates: vec![],
                    fatigue: None,
                    activity: vec![],
                    agriculture: None,
                    health: None,
                    knowledge: None,
                    social: None,
                    food_search: None,
                    claims: Some(game_economy_claims::Claims {
                        claims: vec![game_economy_claims::Claim {
                            person: 1,
                            region: 1,
                            good: 1,
                        }],
                        ..Default::default()
                    }),
                    opinions: None,
                    goals: None,
                    agreements: None,
                    sites: None,
                    schedule: None,
                };
                let mut respectful = CoreHost::begin(snapshot.clone(), &[(1, 1)]).unwrap();
                let response = respectful.respond_to_work_requests(&[], 1).unwrap();
                assert_eq!(response.actors, vec![2]);
                assert!(response.events.is_empty());
                assert_eq!(
                    respectful
                        .snapshot()
                        .autowork
                        .unwrap()
                        .requests
                        .unwrap()
                        .pending
                        .len(),
                    1
                );
                let saved = serde_json::to_string(&respectful.snapshot()).unwrap();
                let mut disregard: CoreHostSnapshot = serde_json::from_str(&saved).unwrap();
                disregard
                    .autowork
                    .as_mut()
                    .unwrap()
                    .requests
                    .as_mut()
                    .unwrap()
                    .preferences[0]
                    .disregard_claims = true;
                let mut host = CoreHost::resume(disregard).unwrap();
                let response = host.respond_to_work_requests(&[], 1).unwrap();
                assert!(matches!(
                    response.events[0].outcome,
                    Outcome::Started { job: 2 }
                ));
                let state = host.snapshot();
                assert_eq!(state.claims.as_ref().unwrap().events.len(), 1);
                assert_eq!(state.claims.as_ref().unwrap().events[0].person, 2);
                state
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn pressured_meeting_can_share_a_false_personal_report_once_per_tick() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
        let mut initial = fixture();
        initial.physical.custody.tick = 1;
        initial.people[1].region = Some(1);
        let mut third = initial.people[1].clone();
        third.id = 3;
        initial.people.push(third);
        initial.travel.as_mut().unwrap().journeys.clear();
        initial.nutrition = None;
        initial.pending_food.clear();
        let minds = vec![
            game_economy_knowledge::Mind {
                person: 2, remember_people: false, people_seen: vec![], people_reporting: None, people_reports: vec![],
                reporting: Some(game_economy_knowledge::Reporting {willing: false, pressure_threshold: Some(1)}),
                misreport: Some(game_economy_knowledge::Misreport {target: Some(3), food_delta: 5}),
                records: vec![game_economy_knowledge::Record {region: 2, food: 10, observed_at: 0, witness: 2, learned_from: None}],
            },
            game_economy_knowledge::Mind {
                person: 3, remember_people: false, people_seen: vec![], people_reporting: None, people_reports: vec![],
                reporting: None, misreport: None, records: vec![],
            },
        ];
        let rules = serde_json::from_value(serde_json::json!({"minimum":-10,"maximum":10,"initial":0,"fulfilled_gain":1,"default_loss":1})).unwrap();
        let opinions = game_opinions::Opinions {rules, sensitivities: Default::default(), records: vec![game_opinions::Opinion {
            observer: 2, target: 3, trust: 0, fear: 2, obligation: 0, affinity: 0,
        }], events: vec![]};
        let social = game_economy::core_interval::CoreSocial {
            owner: game_social::Social {
                rules: game_social::Rules {interval: 1, min_health: 1, max_hunger: 10, familiarity_gain: 1, familiarity_max: 10, relationship_retention: None},
                participants: vec![game_social::Participant {person: 2, willing: false}, game_social::Participant {person: 3, willing: true}],
                pressures: vec![game_social::PressurePolicy {person: 2, fear_at_least: Some(1), obligation_at_least: None, enabled: true}],
                relationships: vec![], events: vec![], needs: None,
            }, last_tick: None,
        };
        let snapshot = CoreHostSnapshot {
            core: initial, autowork: None, tool_wear: None, tool_requirements: vec![], paused_work_rates: vec![],
            fatigue: None, activity: vec![], agriculture: None, health: None, claims: None, social: Some(social),
            food_search: None, knowledge: Some(game_economy_knowledge::Knowledge {max_records: 4, minds, retention: vec![]}),
            opinions: Some(opinions), goals: None, agreements: None, sites: None, schedule: None,
        };
        let mut failing = snapshot.clone();
        failing.social.as_mut().unwrap().owner.relationships.push(game_social::Relationship {
            a: 2, b: 3, familiarity: 1, encounters: u64::MAX, last_met: 0,
        });
        let mut failed = CoreHost::begin(failing, &[(1, 1)]).unwrap();
        let before = failed.snapshot();
        assert!(failed.meet_autonomously(&[]).is_err());
        assert_eq!(failed.snapshot(), before);
        let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
        host.enable_schedule(500, 1).unwrap();
        let result = host.advance_scheduled(0, &[]).unwrap();
        assert_eq!(result.failure, None);
        assert!(result.events.iter().any(|event| matches!(event, game_economy::core_interval::CoreRunEvent::Meetings(rows) if rows.len() == 1 && rows[0].pressured == Some(2))));
        let state = host.snapshot();
        let memory = &state.knowledge.as_ref().unwrap().minds.iter().find(|mind| mind.person == 3).unwrap().records;
        assert_eq!(memory.iter().find(|record| record.region == 2).unwrap().food, 15);
        assert_eq!(state.social.as_ref().unwrap().last_tick, Some(1));
        let saved = serde_json::to_string(&state).unwrap();
        let mut restored = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
        assert!(restored.meet_autonomously(&[]).unwrap().is_empty());
        assert_eq!(restored.snapshot(), state);
        state
    })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn disease_uses_actual_meetings_and_does_not_require_social_to_progress() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
        let mut initial = fixture();
        initial.physical.custody.tick = 1;
        initial.rules.starvation_damage = 0;
        initial.people[1].region = Some(1);
        let mut third = initial.people[1].clone(); third.id = 3; initial.people.push(third);
        initial.travel.as_mut().unwrap().journeys.clear();
        initial.nutrition = None;
        initial.pending_food.clear();
        let disease = game_disease::Disease {
            strains: vec![game_disease::Strain {
                id: 1, name: "fever".into(), incubation: 0, infectious_ticks: 5, immunity_ticks: 0,
                damage_per_tick: 4, transmission_per_million: 1_000_000,
                combat_transmission_per_million: 0, birth_transmission_per_million: 0,
            }], treatments: vec![], cases: vec![game_disease::Case {person: 2, strain: 1, infected_at: 0, source: None}], events: vec![],
        };
        let social = game_economy::core_interval::CoreSocial {
            owner: game_social::Social {
                rules: game_social::Rules {interval: 1, min_health: 1, max_hunger: 10, familiarity_gain: 1, familiarity_max: 10, relationship_retention: None},
                participants: vec![game_social::Participant {person: 2, willing: true}, game_social::Participant {person: 3, willing: true}],
                pressures: vec![], relationships: vec![], events: vec![], needs: None,
            }, last_tick: None,
        };
        let snapshot = CoreHostSnapshot {
            core: initial, autowork: None, tool_wear: None, tool_requirements: vec![], paused_work_rates: vec![],
            fatigue: None, activity: vec![], agriculture: None, health: Some(game_economy::core_interval::CoreHealth {owner: disease, treatment: vec![], last_tick: None}),
            claims: None, social: Some(social), knowledge: None, food_search: None,
            opinions: None, goals: None, agreements: None, sites: None, schedule: None,
        };
        let mut long_period = snapshot.clone();
        long_period.social = None;
        let mut long_period = CoreHost::begin(long_period, &[(1, 1)]).unwrap();
        long_period.enable_schedule(5000, 1).unwrap();
        assert_eq!(long_period.advance_continuous(2000, &[]).unwrap().failure, None);
        assert_eq!(long_period.snapshot().core.people[1].health, 88);
        assert_eq!(long_period.snapshot().health.unwrap().last_tick, Some(3));
        let mut alone = snapshot.clone();
        alone.social = None;
        alone.core.people[0].health = 3;
        alone.health.as_mut().unwrap().owner.cases[0].person = 1;
        let mut alone = CoreHost::begin(alone, &[(1, 1)]).unwrap();
        alone.enable_schedule(1000, 1).unwrap();
        assert_eq!(alone.advance_scheduled(500, &[]).unwrap().failure, None);
        let stopped = alone.snapshot();
        assert_eq!(stopped.core.people[0].health, 0);
        assert_eq!(stopped.paused_work_rates, vec![(1, 1)]);
        assert_eq!(stopped.core.physical.work[0].progress_remainder, 0);

        let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
        host.enable_schedule(1000, 1).unwrap();
        let result = host.advance_scheduled(0, &[]).unwrap();
        assert_eq!(result.failure, None);
        assert!(result.events.iter().any(|event| matches!(event, game_economy::core_interval::CoreRunEvent::Disease(rows) if rows.iter().any(|row| row.person == 3 && row.kind == game_disease::EventKind::Infected))));
        let state = host.snapshot();
        assert_eq!(state.core.people[1].health, 96);
        assert_eq!(state.core.people[2].health, 100);
        let saved = serde_json::to_string(&state).unwrap();
        let mut restored = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
        assert_eq!(restored.advance_scheduled(0, &[]).unwrap().failure, None);
        assert_eq!(restored.snapshot(), state);
        state
    })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn personal_treatment_consumes_medicine_and_only_clears_matching_episodes() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.physical.custody.tick = 1;
                initial.people[1].region = Some(1);
                initial.people[1].health = 90;
                let mut third = initial.people[1].clone();
                third.id = 3;
                third.controlled = true;
                initial.people.push(third);
                initial.travel = None;
                initial.nutrition = None;
                initial.pending_food.clear();
                initial.physical.custody.containers.extend([2, 3].map(|id| {
                    Container::new(
                        Place::Person(id),
                        10,
                        vec![Lot {
                            item: None,
                            good: 2,
                            quantity: 1,
                            expires_at: None,
                        }],
                    )
                    .unwrap()
                }));
                initial
                    .physical
                    .custody
                    .containers
                    .sort_by_key(|row| row.place);
                let strain = |id| game_disease::Strain {
                    id,
                    name: format!("strain-{id}"),
                    incubation: 0,
                    infectious_ticks: 5,
                    immunity_ticks: 0,
                    damage_per_tick: 4,
                    transmission_per_million: 0,
                    combat_transmission_per_million: 0,
                    birth_transmission_per_million: 0,
                };
                let health = game_economy::core_interval::CoreHealth {
                    owner: game_disease::Disease {
                        strains: vec![strain(1), strain(2)],
                        treatments: vec![game_disease::Treatment {
                            strain: 1,
                            medicine: 2,
                        }],
                        cases: vec![
                            game_disease::Case {
                                person: 2,
                                strain: 1,
                                infected_at: 0,
                                source: None,
                            },
                            game_disease::Case {
                                person: 3,
                                strain: 2,
                                infected_at: 0,
                                source: None,
                            },
                        ],
                        events: vec![],
                    },
                    treatment: vec![
                        game_economy_treatment::Policy {
                            person: 2,
                            medicine: 2,
                            quantity: 1,
                            willing: true,
                            health_at_most: None,
                        },
                        game_economy_treatment::Policy {
                            person: 3,
                            medicine: 2,
                            quantity: 1,
                            willing: false,
                            health_at_most: None,
                        },
                    ],
                    last_tick: None,
                };
                let snapshot = CoreHostSnapshot {
                    core: initial,
                    autowork: None,
                    tool_wear: None,
                    tool_requirements: vec![],
                    paused_work_rates: vec![],
                    fatigue: None,
                    activity: vec![],
                    agriculture: None,
                    health: Some(health),
                    social: None,
                    knowledge: None,
                    claims: None,
                    food_search: None,
                    opinions: None,
                    goals: None,
                    agreements: None,
                    sites: None,
                    schedule: None,
                };
                let mut identified = snapshot.clone();
                let held = identified
                    .core
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|row| row.place == Place::Person(2))
                    .unwrap();
                *held = Container::new(
                    held.place,
                    10,
                    vec![Lot {
                        item: Some(10),
                        good: 2,
                        quantity: 1,
                        expires_at: None,
                    }],
                )
                .unwrap();
                let mut failed = CoreHost::begin(identified, &[(1, 1)]).unwrap();
                let before = failed.snapshot();
                assert!(failed.treat_autonomously(&[]).is_err());
                assert_eq!(failed.snapshot(), before);

                let mut scheduled = CoreHost::begin(snapshot.clone(), &[(1, 1)]).unwrap();
                scheduled.enable_schedule(1000, 1).unwrap();
                assert_eq!(scheduled.advance_scheduled(0, &[]).unwrap().failure, None);
                assert_eq!(scheduled.snapshot().core.people[1].health, 90);
                assert!(
                    scheduled
                        .snapshot()
                        .health
                        .unwrap()
                        .owner
                        .cases
                        .iter()
                        .all(|case| case.person != 2)
                );
                assert!(
                    scheduled
                        .snapshot()
                        .health
                        .unwrap()
                        .owner
                        .events
                        .iter()
                        .any(|event| event.person == 2
                            && event.kind == game_disease::EventKind::Treated)
                );
                let mut manual_only = CoreHost::begin(snapshot.clone(), &[(1, 1)]).unwrap();
                assert_eq!(manual_only.treat_controlled(3).unwrap().actors, vec![3]);
                assert_eq!(
                    manual_only
                        .snapshot()
                        .core
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Person(2))
                        .unwrap()
                        .used,
                    1
                );
                let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
                let before = host.snapshot();
                assert!(host.treat_controlled(2).is_err());
                assert_eq!(host.snapshot(), before);
                let result = host.treat_autonomously(&[]).unwrap();
                assert_eq!(result.actors, vec![2]);
                assert!(result.outcomes[0].accepted);
                let state = host.snapshot();
                assert_eq!(state.health.as_ref().unwrap().owner.cases.len(), 1);
                let saved = serde_json::to_string(&state).unwrap();
                let mut host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                assert!(host.treat_controlled(3).unwrap().outcomes[0].accepted);
                let result = host.snapshot();
                assert_eq!(result.health.as_ref().unwrap().owner.cases[0].strain, 2);
                assert_eq!(
                    result
                        .core
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Person(3))
                        .unwrap()
                        .used,
                    0
                );
                result
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn boundary_cultivation_uses_real_seeds_grows_and_harvests_after_reload() {
    use game_economy_agriculture::{Agriculture, Crop, Field, Phase, Policy};
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.people[1].region = Some(1);
                let mut third = initial.people[1].clone();
                third.id = 3;
                initial.people.push(third);
                initial.travel = None;
                initial.nutrition = None;
                initial.pending_food.clear();
                initial.rules.starvation_damage = 0;
                let stock = initial
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|row| row.place == Place::Stock { region: 1, good: 1 })
                    .unwrap();
                *stock = Container::new(
                    stock.place,
                    10,
                    vec![Lot {
                        item: None,
                        good: 1,
                        quantity: 1,
                        expires_at: None,
                    }],
                )
                .unwrap();
                let agriculture = Agriculture {
                    inherit_births: false,
                    requests: None,
                    soil: None,
                    crops: vec![Crop {
                        id: 1,
                        seed: 1,
                        seed_quantity: 1,
                        output: 2,
                        output_quantity: 1,
                        growth_ticks: 1,
                    }],
                    fields: (1..=2)
                        .map(|id| Field {
                            id,
                            region: 1,
                            crop: 1,
                            phase: Phase::Fallow,
                            remaining: 0,
                        })
                        .collect(),
                    policies: (2..=3)
                        .map(|person| Policy {
                            person,
                            willing: true,
                            pressured_by: None,
                            pressure_threshold: None,
                        })
                        .collect(),
                    max_hunger: 10,
                };
                let snapshot = CoreHostSnapshot {
                    core: initial,
                    autowork: None,
                    tool_wear: None,
                    tool_requirements: vec![],
                    paused_work_rates: vec![],
                    fatigue: None,
                    activity: vec![],
                    agriculture: Some(agriculture),
                    health: None,
                    social: None,
                    knowledge: None,
                    claims: None,
                    food_search: None,
                    opinions: None,
                    goals: None,
                    agreements: None,
                    sites: None,
                    schedule: None,
                };
                let mut identified = snapshot.clone();
                let seed = identified
                    .core
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|row| row.place == Place::Stock { region: 1, good: 1 })
                    .unwrap();
                *seed = Container::new(
                    seed.place,
                    10,
                    vec![Lot {
                        item: Some(20),
                        good: 1,
                        quantity: 1,
                        expires_at: None,
                    }],
                )
                .unwrap();
                let mut failed = CoreHost::begin(identified, &[(1, 1)]).unwrap();
                failed.progress(1000).unwrap();
                let before = failed.snapshot();
                assert!(failed.settle().is_err());
                assert_eq!(failed.snapshot(), before);
                let mut claimed = snapshot.clone();
                claimed.agriculture.as_mut().unwrap().fields.truncate(1);
                claimed.claims = Some(game_economy_claims::Claims {
                    field_claims: vec![game_economy_claims::FieldClaim {
                        person: 1,
                        field: 1,
                    }],
                    preferences: vec![game_economy_claims::Preference {
                        person: 3,
                        disregard_others: true,
                        assert_outputs: false,
                        assert_built_sites: false,
                        assert_cultivated_fields: true,
                        desperate_hunger: None,
                    }],
                    ..Default::default()
                });
                let mut claimed = CoreHost::begin(claimed, &[(1, 1)]).unwrap();
                claimed.progress(1000).unwrap();
                claimed.settle().unwrap();
                let claims = claimed.snapshot().claims.unwrap();
                assert_eq!(claims.field_events.len(), 1);
                assert_eq!(
                    (
                        claims.field_events[0].claimant,
                        claims.field_events[0].person
                    ),
                    (1, 3)
                );
                assert!(
                    claims
                        .field_claims
                        .iter()
                        .any(|claim| claim.person == 3 && claim.field == 1)
                );
                let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
                host.enable_schedule(5000, 1).unwrap();
                assert_eq!(host.advance_continuous(1000, &[]).unwrap().failure, None);
                let state = host.snapshot();
                assert_eq!(
                    state
                        .agriculture
                        .as_ref()
                        .unwrap()
                        .fields
                        .iter()
                        .filter(|field| field.phase == Phase::Growing)
                        .count(),
                    1
                );
                assert_eq!(
                    state
                        .agriculture
                        .as_ref()
                        .unwrap()
                        .fields
                        .iter()
                        .filter(|field| field.phase == Phase::Fallow)
                        .count(),
                    1
                );
                let saved = serde_json::to_string(&state).unwrap();
                let mut restored = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                assert_eq!(host.advance_continuous(1000, &[]).unwrap().failure, None);
                assert_eq!(
                    restored.advance_continuous(1000, &[]).unwrap().failure,
                    None
                );
                assert!(
                    host.snapshot()
                        .agriculture
                        .unwrap()
                        .fields
                        .iter()
                        .any(|field| field.phase == Phase::Ripe)
                );
                assert_eq!(host.advance_continuous(1000, &[]).unwrap().failure, None);
                assert_eq!(
                    restored.advance_continuous(1000, &[]).unwrap().failure,
                    None
                );
                let result = host.snapshot();
                assert_eq!(result, restored.snapshot());
                assert!(
                    result
                        .agriculture
                        .as_ref()
                        .unwrap()
                        .fields
                        .iter()
                        .all(|field| field.phase == Phase::Fallow)
                );
                assert_eq!(
                    result
                        .core
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Stock { region: 1, good: 2 })
                        .unwrap()
                        .used,
                    2
                );
                result
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
#[ignore = "manual concentrated cultivation scaling benchmark"]
fn core_host_cultivation_benchmark() {
    use game_economy_agriculture::{Agriculture, Crop, Field, Phase, Policy};
    use std::hash::{Hash, Hasher};
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(50_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    assert!(count > 1);
    let mut initial = fixture();
    let template = initial.people[0].clone();
    initial.people = (1..=count)
        .map(|id| {
            let mut person = template.clone();
            person.id = id;
            person
        })
        .collect();
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        count,
        vec![Lot {
            item: None,
            good: 1,
            quantity: count - 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let agriculture = Agriculture {
        inherit_births: false,
        requests: None,
        soil: None,
        crops: vec![Crop {
            id: 1,
            seed: 1,
            seed_quantity: 1,
            output: 2,
            output_quantity: 1,
            growth_ticks: 1,
        }],
        fields: (1..count)
            .map(|id| Field {
                id,
                region: 1,
                crop: 1,
                phase: Phase::Fallow,
                remaining: 0,
            })
            .collect(),
        policies: (2..=count)
            .map(|person| Policy {
                person,
                willing: true,
                pressured_by: None,
                pressure_threshold: None,
            })
            .collect(),
        max_hunger: 10,
    };
    let snapshot = CoreHostSnapshot {
        core: initial,
        autowork: None,
        tool_wear: None,
        tool_requirements: vec![],
        paused_work_rates: vec![],
        fatigue: None,
        activity: vec![],
        agriculture: Some(agriculture),
        health: None,
        social: None,
        knowledge: None,
        claims: None,
        food_search: None,
        opinions: None,
        goals: None,
        agreements: None,
        sites: None,
        schedule: None,
    };
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
            host.progress(1000).unwrap();
            let start = std::time::Instant::now();
            let events = host.settle().unwrap();
            let elapsed = start.elapsed();
            assert_eq!(
                events
                    .physical_actions
                    .iter()
                    .filter(|row| row.accepted)
                    .count(),
                (count - 1) as usize
            );
            let state = host.snapshot();
            assert_eq!(
                state
                    .agriculture
                    .as_ref()
                    .unwrap()
                    .fields
                    .iter()
                    .filter(|field| field.phase == Phase::Growing)
                    .count(),
                (count - 1) as usize
            );
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            serde_json::to_vec(&state).unwrap().hash(&mut hash);
            println!(
                "workers={workers} plantings={} elapsed={:.6} fingerprint={:016x}",
                count - 1,
                elapsed.as_secs_f64(),
                hash.finish()
            );
        });
}

#[test]
#[ignore = "manual personal meetings and reporting scaling benchmark"]
fn core_host_social_benchmark() {
    use std::hash::{Hash, Hasher};
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(50_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    assert!(count > 2);
    let treatment = std::env::var_os("LEGACY_TREATMENT").is_some();
    let disease = std::env::var_os("LEGACY_DISEASE").is_some() || treatment;
    let mut initial = fixture();
    initial.physical.custody.tick = 1;
    let template = initial.people[0].clone();
    initial.people = (1..=count)
        .map(|id| {
            let mut person = template.clone();
            person.id = id;
            person
        })
        .collect();
    initial.travel.as_mut().unwrap().journeys.clear();
    initial.nutrition = None;
    initial.pending_food.clear();
    if treatment {
        initial
            .physical
            .custody
            .containers
            .extend((2..=count).map(|person| {
                Container::new(
                    Place::Person(person),
                    10,
                    vec![Lot {
                        item: None,
                        good: 2,
                        quantity: 1,
                        expires_at: None,
                    }],
                )
                .unwrap()
            }));
        initial
            .physical
            .custody
            .containers
            .sort_by_key(|row| row.place);
    }
    let social = game_economy::core_interval::CoreSocial {
        owner: game_social::Social {
            rules: game_social::Rules {
                interval: 1,
                min_health: 1,
                max_hunger: 10,
                familiarity_gain: 1,
                familiarity_max: 10,
                relationship_retention: None,
            },
            participants: (2..=count)
                .map(|person| game_social::Participant {
                    person,
                    willing: true,
                })
                .collect(),
            pressures: vec![],
            relationships: vec![],
            events: vec![],
            needs: None,
        },
        last_tick: None,
    };
    let knowledge = game_economy_knowledge::Knowledge {
        max_records: 2,
        retention: vec![],
        minds: (2..=count)
            .map(|person| game_economy_knowledge::Mind {
                person,
                remember_people: false,
                people_seen: vec![],
                people_reporting: None,
                people_reports: vec![],
                reporting: Some(game_economy_knowledge::Reporting {
                    willing: true,
                    pressure_threshold: None,
                }),
                misreport: None,
                records: vec![game_economy_knowledge::Record {
                    region: 2,
                    food: 10,
                    observed_at: 0,
                    witness: person,
                    learned_from: None,
                }],
            })
            .collect(),
    };
    let snapshot = CoreHostSnapshot {
        core: initial,
        autowork: None,
        tool_wear: None,
        tool_requirements: vec![],
        paused_work_rates: vec![],
        fatigue: None,
        activity: vec![],
        agriculture: None,
        health: disease.then(|| game_economy::core_interval::CoreHealth {
            treatment: if treatment {
                (2..=count)
                    .map(|person| game_economy_treatment::Policy {
                        person,
                        medicine: 2,
                        quantity: 1,
                        willing: true,
                        health_at_most: Some(100),
                    })
                    .collect()
            } else {
                vec![]
            },
            owner: game_disease::Disease {
                strains: vec![game_disease::Strain {
                    id: 1,
                    name: "fever".into(),
                    incubation: 0,
                    infectious_ticks: 5,
                    immunity_ticks: 0,
                    damage_per_tick: 1,
                    transmission_per_million: 1_000_000,
                    combat_transmission_per_million: 0,
                    birth_transmission_per_million: 0,
                }],
                treatments: if treatment {
                    vec![game_disease::Treatment {
                        strain: 1,
                        medicine: 2,
                    }]
                } else {
                    vec![]
                },
                cases: (2..=count)
                    .step_by(if treatment { 1 } else { 2 })
                    .map(|person| game_disease::Case {
                        person,
                        strain: 1,
                        infected_at: 0,
                        source: None,
                    })
                    .collect(),
                events: vec![],
            },
            last_tick: None,
        }),
        claims: None,
        social: Some(social),
        food_search: None,
        knowledge: Some(knowledge),
        opinions: None,
        goals: None,
        agreements: None,
        sites: None,
        schedule: None,
    };
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
            let start = std::time::Instant::now();
            if treatment {
                let result = host.treat_autonomously(&[]).unwrap();
                let elapsed = start.elapsed();
                assert_eq!(result.actors.len(), (count - 1) as usize);
                assert!(result.outcomes.iter().all(|row| row.accepted));
                assert!(
                    host.snapshot()
                        .health
                        .as_ref()
                        .unwrap()
                        .owner
                        .cases
                        .is_empty()
                );
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                serde_json::to_vec(&host.snapshot())
                    .unwrap()
                    .hash(&mut hash);
                println!(
                    "workers={workers} treatments={} elapsed={:.6} fingerprint={:016x}",
                    result.actors.len(),
                    elapsed.as_secs_f64(),
                    hash.finish()
                );
                return;
            }
            let meetings = host.meet_autonomously(&[]).unwrap();
            let elapsed = start.elapsed();
            assert_eq!(meetings.len(), ((count - 1) / 2) as usize);
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            serde_json::to_vec(&host.snapshot())
                .unwrap()
                .hash(&mut hash);
            println!(
                "workers={workers} meetings={} elapsed={:.6} fingerprint={:016x}",
                meetings.len(),
                elapsed.as_secs_f64(),
                hash.finish()
            );
        });
}

#[test]
fn core_host_clears_old_request_events_at_the_boundary() {
    use game_autowork::requests::{Outcome, Preference, Request, Requests};
    let mut initial = fixture();
    initial.people[1].region = Some(1);
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    let requests = Requests {
        next_id: 2,
        inherit_births: false,
        memory_limit: 4,
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
            expires: 5,
        }],
        events: vec![],
    };
    let auto = game_autowork::Autowork {
        next_id: 2,
        policies: vec![],
        requests: Some(requests),
    };
    let mut host = CoreHost::begin(
        CoreHostSnapshot {
            core: initial,
            autowork: Some(auto),
            tool_wear: None,
            opinions: None,
            goals: None,
            agreements: None,
            sites: None,
            tool_requirements: vec![],
            paused_work_rates: vec![],
            fatigue: None,
            activity: vec![],
            agriculture: None,
            health: None,
            knowledge: None,
            social: None,
            claims: None,
            food_search: None,
            schedule: None,
        },
        &[(1, 1)],
    )
    .unwrap();
    assert_eq!(
        host.respond_to_work_requests(&[], 1).unwrap().events[0].outcome,
        Outcome::Refused
    );
    assert_eq!(
        host.snapshot()
            .autowork
            .as_ref()
            .unwrap()
            .requests
            .as_ref()
            .unwrap()
            .events
            .len(),
        1
    );
    host.progress(1000).unwrap();
    host.settle().unwrap();
    let snapshot = host.snapshot();
    let owner = snapshot.autowork.unwrap().requests.unwrap();
    assert!(owner.events.is_empty());
    assert_eq!(owner.memories.len(), 1);
}

#[test]
fn scheduled_personal_goals_create_requests_and_count_only_witnessed_help() {
    use game_autowork::requests::{Asker, Preference, Requests};
    use game_economy::core_interval::CoreRunEvent;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
            let mut initial = fixture();
            initial.people[1].region = Some(1);
            initial.travel = None;
            initial.nutrition = None;
            initial.pending_food.clear();
            let stock = initial.physical.custody.containers.iter_mut().find(|row| row.place == Place::Stock { region: 1, good: 1 }).unwrap();
            *stock = Container::new(stock.place, 10, vec![Lot { item: None, good: 1, quantity: 2, expires_at: None }]).unwrap();
            let requests = Requests {
                next_id: 1, inherit_births: false, memory_limit: 4,
                work_links: vec![], memories: vec![],
                preferences: vec![Preference { person: 1, effort: 0, help_value: 1, affinity_weight: 0, trust_weight: 0, offer_value: None, disregard_claims: false }],
                askers: vec![Asker { person: 2, interval: 1, lifetime: 3, retry_after_offer: false }],
                pending: vec![], events: vec![],
            };
            let goals = game_goals::Goals { retry_interval: 3, plan_inputs: false, people: vec![game_goals::Goal { person: 2, recipe: 1, priority: 1, target: 1, progress: 0, next_review: 0, enabled: true, recognize_help: true }], events: vec![] };
            let snapshot = CoreHostSnapshot { core: initial, autowork: Some(game_autowork::Autowork { next_id: 2, policies: vec![], requests: Some(requests) }), tool_wear: None, opinions: None, goals: Some(goals), agreements: None, sites: None, tool_requirements: vec![], paused_work_rates: vec![], fatigue: None, activity: vec![], agriculture: None, health: None, knowledge: None, social: None, claims: None, food_search: None, schedule: None };
            let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
            host.enable_schedule(1000, 1).unwrap();
            let run = host.advance_continuous(1000, &[]).unwrap();
            assert_eq!(run.failure, None);
            assert!(run.events.iter().any(|event| matches!(event, CoreRunEvent::Asks(actors) if actors == &[2])));
            assert!(run.events.iter().any(|event| matches!(event, CoreRunEvent::Requests(events) if events.iter().any(|event| matches!(event.outcome, game_autowork::requests::Outcome::Started { .. })))));
            let saved = serde_json::to_string(&host.snapshot()).unwrap();
            let saved: CoreHostSnapshot = serde_json::from_str(&saved).unwrap();
            let mut absent = saved.clone();
            absent.core.people[1].region = Some(2);
            let mut absent = CoreHost::resume(absent).unwrap();
            assert_eq!(absent.advance_continuous(1000, &[]).unwrap().failure, None);
            assert_eq!(absent.snapshot().goals.unwrap().people[0].progress, 0);
            let mut present = CoreHost::resume(saved).unwrap();
            assert_eq!(present.advance_continuous(1000, &[]).unwrap().failure, None);
            let snapshot = present.snapshot();
            assert_eq!(snapshot.goals.as_ref().unwrap().people[0].progress, 1);
            snapshot
        })
    };
    assert_eq!(run(1), run(6));
}

#[test]
#[ignore = "manual goal-driven request creation scaling benchmark"]
fn core_host_goal_asking_benchmark() {
    use game_autowork::requests::{Asker, Requests};
    use std::hash::{Hash, Hasher};
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(50_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    assert!(count >= 4 && count.is_multiple_of(2));
    let mut initial = fixture();
    initial.physical.custody.tick = 1;
    let template = initial.people[0].clone();
    initial.people = (1..=count)
        .map(|id| {
            let mut person = template.clone();
            person.id = id;
            person
        })
        .collect();
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    let askers: Vec<_> = (2..count)
        .step_by(2)
        .map(|person| Asker {
            person,
            interval: 1,
            lifetime: 3,
            retry_after_offer: false,
        })
        .collect();
    let goals = game_goals::Goals {
        retry_interval: 3,
        plan_inputs: false,
        people: askers
            .iter()
            .map(|asker| game_goals::Goal {
                person: asker.person,
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
    };
    let requests = Requests {
        next_id: 1,
        inherit_births: false,
        memory_limit: 0,
        work_links: vec![],
        memories: vec![],
        preferences: vec![],
        askers,
        pending: vec![],
        events: vec![],
    };
    let snapshot = CoreHostSnapshot {
        core: initial,
        autowork: Some(game_autowork::Autowork {
            next_id: 2,
            policies: vec![],
            requests: Some(requests),
        }),
        tool_wear: None,
        opinions: None,
        goals: Some(goals),
        agreements: None,
        sites: None,
        tool_requirements: vec![],
        paused_work_rates: vec![],
        fatigue: None,
        activity: vec![],
        agriculture: None,
        health: None,
        knowledge: None,
        social: None,
        claims: None,
        food_search: None,
        schedule: None,
    };
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap();
    pool.install(|| {
        let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
        let start = std::time::Instant::now();
        let actors = host.ask_for_work(&[]).unwrap();
        let elapsed = start.elapsed();
        assert_eq!(actors.len(), count as usize / 2 - 1);
        let state = serde_json::to_vec(&host.snapshot()).unwrap();
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        state.hash(&mut hash);
        println!("fingerprint={:016x}", hash.finish());
        println!(
            "workers={workers} asks={} elapsed={:.6}",
            actors.len(),
            elapsed.as_secs_f64()
        );
    });
}

#[test]
fn personal_goal_selects_work_and_advances_only_after_completion() {
    let mut initial = fixture();
    initial.people[1].region = Some(1);
    initial.people[1].willing_to_work = true;
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        10,
        vec![Lot {
            item: None,
            good: 1,
            quantity: 2,
            expires_at: None,
        }],
    )
    .unwrap();
    let goals = game_goals::Goals {
        retry_interval: 3,
        plan_inputs: false,
        people: vec![game_goals::Goal {
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
    };
    let snapshot = CoreHostSnapshot {
        core: initial,
        autowork: Some(game_autowork::Autowork {
            next_id: 2,
            policies: vec![],
            requests: None,
        }),
        tool_wear: None,
        opinions: None,
        goals: Some(goals),
        agreements: None,
        sites: None,
        tool_requirements: vec![],
        paused_work_rates: vec![],
        fatigue: None,
        activity: vec![],
        agriculture: None,
        health: None,
        knowledge: None,
        social: None,
        claims: None,
        food_search: None,
        schedule: None,
    };
    let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
    host.enable_schedule(1000, 1).unwrap();
    assert_eq!(host.advance_continuous(0, &[]).unwrap().failure, None);
    assert_eq!(
        host.snapshot().goals.as_ref().unwrap().people[0].progress,
        0
    );
    assert_eq!(host.advance_continuous(1000, &[]).unwrap().failure, None);
    assert_eq!(host.snapshot().goals.unwrap().people[0].progress, 1);
}

#[test]
fn an_unfunded_offer_can_motivate_work_without_forcing_delivery() {
    use game_autowork::requests::{OfferValue, Outcome, Preference, Request, Requests};
    let mut initial = fixture();
    initial.people[1].region = Some(1);
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        10,
        vec![Lot {
            item: None,
            good: 1,
            quantity: 2,
            expires_at: None,
        }],
    )
    .unwrap();
    let requests = Requests {
        next_id: 2,
        inherit_births: false,
        memory_limit: 4,
        work_links: vec![],
        memories: vec![],
        preferences: vec![Preference {
            person: 2,
            effort: 5,
            help_value: 0,
            affinity_weight: 0,
            trust_weight: 0,
            offer_value: Some(OfferValue {
                good: 2,
                value_per_unit: 1,
                max_quantity: 10,
            }),
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
    };
    let agreements = game_agreements::Agreements {
        next_id: 1,
        policies: vec![],
        offering: vec![],
        work_offering: vec![],
        bulk_debtors: vec![],
        contracts: vec![],
        events: vec![],
    };
    let snapshot = CoreHostSnapshot {
        core: initial,
        autowork: Some(game_autowork::Autowork {
            next_id: 2,
            policies: vec![],
            requests: Some(requests),
        }),
        tool_wear: None,
        opinions: None,
        goals: None,
        agreements: Some(agreements),
        sites: None,
        tool_requirements: vec![],
        paused_work_rates: vec![],
        fatigue: None,
        activity: vec![],
        agriculture: None,
        health: None,
        knowledge: None,
        social: None,
        claims: None,
        food_search: None,
        schedule: None,
    };
    let mut without_offer = CoreHost::begin(snapshot.clone(), &[(1, 1)]).unwrap();
    assert_eq!(
        without_offer
            .respond_to_work_requests(&[], 1)
            .unwrap()
            .events[0]
            .outcome,
        Outcome::Refused
    );
    let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
    let physical_before = host.snapshot().core;
    let ids = host
        .propose_agreements(&[game_agreements::Proposal {
            debtor: 1,
            creditor: 2,
            good: 2,
            quantity: 99,
            due: 1,
        }])
        .unwrap();
    assert_eq!(ids, vec![1]);
    assert_eq!(host.snapshot().core, physical_before);
    let saved = serde_json::to_string(&host.snapshot()).unwrap();
    let mut host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
    assert_eq!(
        host.respond_to_work_requests(&[], 1).unwrap().events[0].outcome,
        Outcome::Started { job: 2 }
    );
    host.decide_agreements(&[game_agreements::Decision {
        contract: 1,
        actor: 2,
        accept: true,
    }])
    .unwrap();
    host.enable_schedule(1000, 1).unwrap();
    assert_eq!(host.advance_continuous(2000, &[]).unwrap().failure, None);
    let snapshot = host.snapshot();
    let contract = &snapshot.agreements.as_ref().unwrap().contracts[0];
    assert_eq!(contract.status, game_agreements::Status::Defaulted);
    assert_eq!(contract.delivered, 0);
}

#[test]
fn voluntary_deliveries_record_only_successful_physical_transfers() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.people[1].region = Some(1);
                let mut third = initial.people[1].clone();
                third.id = 3;
                initial.people.push(third);
                initial.travel = None;
                initial.nutrition = None;
                initial.pending_food.clear();
                initial.physical.custody.containers.extend([
                    Container::new(
                        Place::Person(1),
                        10,
                        vec![Lot {
                            item: None,
                            good: 1,
                            quantity: 1,
                            expires_at: None,
                        }],
                    )
                    .unwrap(),
                    Container::new(Place::Person(2), 10, vec![]).unwrap(),
                    Container::new(Place::Person(3), 10, vec![]).unwrap(),
                ]);
                initial
                    .physical
                    .custody
                    .containers
                    .sort_by_key(|row| row.place);
                let agreements = game_agreements::Agreements {
                    next_id: 1,
                    policies: vec![],
                    offering: vec![],
                    work_offering: vec![],
                    bulk_debtors: vec![],
                    contracts: vec![],
                    events: vec![],
                };
                let mut host = CoreHost::begin(
                    CoreHostSnapshot {
                        core: initial,
                        autowork: None,
                        tool_wear: None,
                        opinions: None,
                        goals: None,
                        agreements: Some(agreements),
                        sites: None,
                        tool_requirements: vec![],
                        paused_work_rates: vec![],
                        fatigue: None,
                        activity: vec![],
                        agriculture: None,
                        health: None,
                        knowledge: None,
                        social: None,
                        claims: None,
                        food_search: None,
                        schedule: None,
                    },
                    &[(1, 1)],
                )
                .unwrap();
                let proposals: Vec<_> = (2..=3)
                    .map(|creditor| game_agreements::Proposal {
                        debtor: 1,
                        creditor,
                        good: 1,
                        quantity: 1,
                        due: 2,
                    })
                    .collect();
                assert_eq!(host.propose_agreements(&proposals).unwrap(), vec![1, 2]);
                let decisions: Vec<_> = (2..=3)
                    .map(|actor| game_agreements::Decision {
                        contract: actor - 1,
                        actor,
                        accept: true,
                    })
                    .collect();
                host.decide_agreements(&decisions).unwrap();
                host.progress(1).unwrap();
                let before = host.snapshot();
                assert!(
                    host.deliver_agreements(&[game_agreements::Delivery {
                        contract: 1,
                        actor: 2,
                        quantity: 1
                    }])
                    .is_err()
                );
                assert_eq!(host.snapshot(), before);
                let saved = serde_json::to_string(&before).unwrap();
                let mut host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                let deliveries: Vec<_> = (1..=2)
                    .map(|contract| game_agreements::Delivery {
                        contract,
                        actor: 1,
                        quantity: 1,
                    })
                    .collect();
                let outcomes = host.deliver_agreements(&deliveries).unwrap();
                assert_eq!(
                    outcomes.iter().filter(|outcome| outcome.accepted).count(),
                    1
                );
                let rejected = outcomes
                    .iter()
                    .find(|outcome| !outcome.accepted)
                    .unwrap()
                    .id;
                let state = host.snapshot();
                for outcome in outcomes {
                    let contract = state
                        .agreements
                        .as_ref()
                        .unwrap()
                        .contracts
                        .iter()
                        .find(|contract| contract.id == outcome.id)
                        .unwrap();
                    assert_eq!(contract.delivered, u64::from(outcome.accepted));
                    assert_eq!(
                        contract.status == game_agreements::Status::Fulfilled,
                        outcome.accepted
                    );
                }
                assert_eq!(
                    state
                        .core
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Person(1))
                        .unwrap()
                        .used,
                    0
                );
                let unavailable = host
                    .deliver_agreements(&[game_agreements::Delivery {
                        contract: rejected,
                        actor: 1,
                        quantity: 1,
                    }])
                    .unwrap();
                assert!(!unavailable[0].accepted);
                assert_eq!(host.snapshot(), state);
                state
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn scheduled_delivery_is_a_personal_choice_that_fear_can_motivate() {
    use game_economy::core_interval::CoreRunEvent;
    let run = |workers, fear| {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
        let mut initial = fixture();
        initial.people[1].region = Some(1);
        initial.people[1].willing_to_work = false;
        initial.travel = None;
        initial.nutrition = None;
        initial.pending_food.clear();
        initial.physical.custody.containers.extend([
            Container::new(Place::Person(1), 10, vec![]).unwrap(),
            Container::new(Place::Person(2), 10, vec![Lot { item: None, good: 1, quantity: 1, expires_at: None }]).unwrap(),
        ]);
        initial.physical.custody.containers.sort_by_key(|row| row.place);
        let policy: game_agreements::Policy = serde_json::from_value(serde_json::json!({"person":2,"honor":false,"fear_honor_threshold":5})).unwrap();
        let agreements = game_agreements::Agreements { next_id: 2, policies: vec![policy], offering: vec![], work_offering: vec![], bulk_debtors: vec![], contracts: vec![game_agreements::Contract { id: 1, debtor: 2, creditor: 1, good: 1, quantity: 1, delivered: 0, due: 5, status: game_agreements::Status::Active }], events: vec![] };
        let opinions: game_opinions::Opinions = serde_json::from_value(serde_json::json!({"rules":{"minimum":-10,"maximum":10,"initial":0,"fulfilled_gain":1,"default_loss":1},"records":[{"observer":2,"target":1,"trust":0,"fear":fear}]})).unwrap();
        let snapshot = CoreHostSnapshot { core: initial, autowork: None, tool_wear: None, opinions: Some(opinions), goals: None, agreements: Some(agreements), sites: None, tool_requirements: vec![], paused_work_rates: vec![], fatigue: None, activity: vec![], agriculture: None, health: None, knowledge: None, social: None, claims: None, food_search: None, schedule: None };
        let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
        host.enable_schedule(1000, 1).unwrap();
        let saved = serde_json::to_string(&host.snapshot()).unwrap();
        let mut host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
        let run = host.advance_scheduled(0, &[]).unwrap();
        assert_eq!(run.failure, None);
        assert_eq!(run.elapsed_millis, 0);
        let state = host.snapshot();
        assert_eq!(state.agreements.as_ref().unwrap().contracts[0].delivered, u64::from(fear >= 5));
        assert_eq!(run.events.iter().any(|event| matches!(event, CoreRunEvent::Deliveries(outcomes) if outcomes.iter().any(|row| row.accepted))), fear >= 5);
        state
    })
    };
    assert_eq!(run(1, 10), run(6, 10));
    assert_eq!(run(1, 0), run(6, 0));
}

#[test]
fn people_can_propose_and_accept_unfunded_promises_without_enforcement() {
    use game_economy::core_interval::CoreRunEvent;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
        let mut initial = fixture();
        initial.people[1].region = Some(1);
        let mut third = initial.people[1].clone();
        third.id = 3;
        initial.people.push(third);
        initial.travel = None;
        initial.nutrition = None;
        initial.pending_food.clear();
        let offer: game_agreements::OfferPolicy = serde_json::from_value(serde_json::json!({"person":2,"recipient":3,"good":2,"quantity":99,"due_after":2,"retry_ticks":10,"willing":true})).unwrap();
        let accept: game_agreements::Policy = serde_json::from_value(serde_json::json!({"person":3,"accept":true})).unwrap();
        let agreements = game_agreements::Agreements { next_id: 1, policies: vec![accept], offering: vec![offer], work_offering: vec![], bulk_debtors: vec![], contracts: vec![], events: vec![] };
        let opinions: game_opinions::Opinions = serde_json::from_value(serde_json::json!({"rules":{"minimum":-10,"maximum":10,"initial":0,"fulfilled_gain":1,"default_loss":1},"records":[{"observer":2,"target":3,"trust":0,"obligation":0}]})).unwrap();
        let snapshot = CoreHostSnapshot { core: initial, autowork: None, tool_wear: None, opinions: Some(opinions.clone()), goals: None, agreements: Some(agreements), sites: None, tool_requirements: vec![], paused_work_rates: vec![], fatigue: None, activity: vec![], agriculture: None, health: None, knowledge: None, social: None, claims: None, food_search: None, schedule: None };
        let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
        host.enable_schedule(1000, 1).unwrap();
        let run = host.advance_scheduled(0, &[]).unwrap();
        assert_eq!(run.failure, None);
        assert!(run.events.iter().any(|event| matches!(event, CoreRunEvent::Offers(ids) if ids == &[1])));
        assert!(run.events.iter().any(|event| matches!(event, CoreRunEvent::Acceptances(ids) if ids == &[1])));
        let started = host.snapshot();
        assert_eq!(started.agreements.as_ref().unwrap().contracts[0].status, game_agreements::Status::Active);
        assert_eq!(started.opinions, Some(opinions));
        let saved = serde_json::to_string(&started).unwrap();
        let mut host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
        assert_eq!(host.advance_continuous(3000, &[]).unwrap().failure, None);
        let state = host.snapshot();
        let contract = &state.agreements.as_ref().unwrap().contracts[0];
        assert_eq!((contract.status, contract.delivered), (game_agreements::Status::Defaulted, 0));
        state
    })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn manually_making_a_promise_uses_the_makers_own_obligation_disposition() {
    let mut initial = fixture();
    initial.people[1].region = Some(1);
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    let opinions: game_opinions::Opinions = serde_json::from_value(serde_json::json!({
        "rules":{"minimum":-10,"maximum":10,"initial":0,"fulfilled_gain":1,"default_loss":1},
        "sensitivities":[{"person":2,"promise_obligation_gain":3}],
        "records":[{"observer":2,"target":1,"trust":0,"obligation":0}]
    }))
    .unwrap();
    let agreements = game_agreements::Agreements {
        next_id: 1,
        policies: vec![],
        offering: vec![],
        work_offering: vec![],
        bulk_debtors: vec![],
        contracts: vec![],
        events: vec![],
    };
    let snapshot = CoreHostSnapshot {
        core: initial,
        autowork: None,
        tool_wear: None,
        opinions: Some(opinions),
        goals: None,
        agreements: Some(agreements),
        sites: None,
        tool_requirements: vec![],
        paused_work_rates: vec![],
        fatigue: None,
        activity: vec![],
        agriculture: None,
        health: None,
        knowledge: None,
        social: None,
        claims: None,
        food_search: None,
        schedule: None,
    };
    let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
    host.propose_agreements(&[game_agreements::Proposal {
        debtor: 2,
        creditor: 1,
        good: 2,
        quantity: 99,
        due: 2,
    }])
    .unwrap();
    assert_eq!(host.snapshot().opinions.unwrap().records[0].obligation, 3);
}

#[test]
fn completed_work_wears_a_site_and_real_repairs_reopen_it() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                let mut other_recipe = initial.physical.recipes[0].clone();
                other_recipe.id = 2;
                initial.physical.recipes.insert(0, other_recipe);
                initial.people[1].region = Some(1);
                initial.travel = None;
                initial.nutrition = None;
                initial.pending_food.clear();
                let stock = initial
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|row| row.place == Place::Stock { region: 1, good: 1 })
                    .unwrap();
                *stock = Container::new(
                    stock.place,
                    10,
                    vec![Lot {
                        item: None,
                        good: 1,
                        quantity: 2,
                        expires_at: None,
                    }],
                )
                .unwrap();
                initial.physical.custody.containers.push(
                    Container::new(
                        Place::Person(2),
                        10,
                        vec![Lot {
                            item: None,
                            good: 1,
                            quantity: 1,
                            expires_at: None,
                        }],
                    )
                    .unwrap(),
                );
                initial
                    .physical
                    .custody
                    .containers
                    .sort_by_key(|row| row.place);
                let facilities = game_economy_work::sites::Facilities {
                    next_id: 2,
                    sites: vec![game_economy_work::sites::Site {
                        id: 1,
                        region: 1,
                        kind: "mill".into(),
                        capacity: 1,
                    }],
                    requirements: vec![game_economy_work::sites::Requirement {
                        recipe: 1,
                        kind: "mill".into(),
                        slots: 1,
                    }],
                    bindings: vec![game_economy_work::sites::Binding {
                        job: 1,
                        site: 1,
                        slots: 1,
                    }],
                };
                let maintenance = game_economy_building::maintenance::Maintenance {
                    rules: vec![game_economy_building::maintenance::Rule {
                        kind: "mill".into(),
                        wear_limit: 1,
                        wear_per_slot: 1,
                        repair_good: 1,
                        repair_quantity: 1,
                    }],
                    conditions: vec![],
                    policies: vec![game_economy_building::maintenance::Policy {
                        person: 2,
                        site: 1,
                        willing: true,
                        pressured_by: None,
                        pressure_threshold: None,
                        disregard_claims: false,
                    }],
                    events: vec![],
                };
                let requests = game_autowork::requests::Requests {
                    next_id: 2, inherit_births: false, memory_limit: 2, work_links: vec![], memories: vec![],
                    preferences: vec![game_autowork::requests::Preference { person: 2, effort: 0, help_value: 1, affinity_weight: 0, trust_weight: 0, offer_value: None, disregard_claims: false }], askers: vec![],
                    pending: vec![game_autowork::requests::Request { id: 1, requester: 1, worker: 2, region: 1, recipe: 1, created_at: 0, expires: 5 }], events: vec![],
                };
                let snapshot = CoreHostSnapshot {
                    core: initial,
                    autowork: Some(game_autowork::Autowork { next_id: 2, policies: vec![], requests: Some(requests) }),
                    tool_wear: None,
                    opinions: None,
                    goals: None,
                    agreements: None,
                    sites: Some(game_economy::core_interval::CoreSites {
                        facilities,
                        maintenance: Some(maintenance),
                    }),
                    tool_requirements: vec![], paused_work_rates: vec![], fatigue: None, activity: vec![], agriculture: None, health: None, knowledge: None, social: None, claims: None, food_search: None, schedule: None,
                };
                let mut host = CoreHost::begin(snapshot, &[(1, 2)]).unwrap();
                host.progress(500).unwrap();
                assert_eq!(
                    host.publish_early_work().unwrap().unwrap().finished,
                    vec![1]
                );
                let worn = host.snapshot();
                let sites = worn.sites.as_ref().unwrap();
                assert_eq!(sites.maintenance.as_ref().unwrap().conditions[0].wear, 1);
                assert!(sites.facilities.bindings.is_empty());
                let mut empty = worn.clone();
                let bag = empty.core.physical.custody.containers.iter_mut().find(|row| row.place == Place::Person(2)).unwrap();
                *bag = Container::new(bag.place, 10, vec![]).unwrap();
                let mut empty = CoreHost::resume(empty).unwrap();
                let failed = empty.repair_sites_autonomously(&[]).unwrap();
                assert!(!failed.events[0].repaired);
                assert_eq!(empty.snapshot().sites.unwrap().maintenance.unwrap().conditions[0].wear, 1);
                let saved = serde_json::to_string(&worn).unwrap();
                let mut host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                let start = (
                    Start {
                        id: 2,
                        worker: 2,
                        region: 1,
                        recipe: 1,
                        units: 1,
                    },
                    1,
                );
                assert_eq!(host.start_chosen_work(&[start]).unwrap(), vec![(2, false)]);
                let response = host.respond_to_work_requests(&[], 1).unwrap();
                assert_eq!(response.actors, vec![2]);
                assert!(response.events.is_empty());
                assert_eq!(host.snapshot().autowork.as_ref().unwrap().requests.as_ref().unwrap().pending.len(), 1);
                host.enable_schedule(500, 1).unwrap();
                let run = host.advance_scheduled(0, &[]).unwrap();
                assert_eq!(run.failure, None);
                assert!(run.events.iter().any(|event| matches!(event, game_economy::core_interval::CoreRunEvent::Repairs(repairs) if repairs.iter().any(|repair| repair.person == 2 && repair.repaired))));
                let repaired = host.snapshot();
                assert_eq!(
                    repaired
                        .sites
                        .as_ref()
                        .unwrap()
                        .maintenance
                        .as_ref()
                        .unwrap()
                        .conditions[0]
                        .wear,
                    0
                );
                assert_eq!(
                    repaired
                        .core
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Person(2))
                        .unwrap()
                        .used,
                    0
                );
                assert_eq!(host.advance_scheduled(1, &[]).unwrap().failure, None);
                assert_eq!(host.start_chosen_work(&[start]).unwrap(), vec![(2, true)]);
                let final_state = host.snapshot();
                assert_eq!(
                    final_state.sites.as_ref().unwrap().facilities.bindings[0].job,
                    2
                );
                assert_eq!(host.advance_continuous(499, &[]).unwrap().failure, None);
                let after_boundary = host.snapshot();
                assert!(after_boundary.sites.as_ref().unwrap().maintenance.as_ref().unwrap().events.is_empty());
                assert_eq!(after_boundary.sites.as_ref().unwrap().facilities.bindings[0].job, 2);
                after_boundary
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn scheduled_tool_repair_consumes_material_and_preserves_identity() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
        let mut initial = fixture();
        initial.people[1].region = Some(1);
        initial.travel = None;
        initial.nutrition = None;
        initial.pending_food.clear();
        initial.physical.custody.containers.push(Container::new(Place::Person(2), 10, vec![Lot { item: Some(10), good: 1, quantity: 1, expires_at: None }, Lot { item: None, good: 2, quantity: 1, expires_at: None }]).unwrap());
        initial.physical.custody.containers.sort_by_key(|row| row.place);
        let tool_wear = game_economy_work::tool_wear::ToolWear { next_item: 11, rules: vec![game_economy_work::tool_wear::Rule { good: 1, durability: 10, wear: 1, repair_good: 2, repair_quantity: 1 }], conditions: vec![game_economy_work::tool_wear::Condition { item: 10, good: 1, damage: 4 }], repairs: vec![game_economy_work::tool_wear::RepairChoice { person: 2, item: 10, willing: true }], events: vec![] };
        let snapshot = CoreHostSnapshot { core: initial, autowork: None, tool_wear: Some(tool_wear), opinions: None, goals: None, agreements: None, sites: None, tool_requirements: vec![], paused_work_rates: vec![], fatigue: None, activity: vec![], agriculture: None, health: None, knowledge: None, social: None, claims: None, food_search: None, schedule: None };
        let mut no_material = snapshot.clone();
        let bag = no_material.core.physical.custody.containers.iter_mut().find(|row| row.place == Place::Person(2)).unwrap();
        *bag = Container::new(bag.place, 10, vec![Lot { item: Some(10), good: 1, quantity: 1, expires_at: None }]).unwrap();
        let mut no_material = CoreHost::begin(no_material, &[(1, 1)]).unwrap();
        assert!(!no_material.repair_tools_autonomously(&[]).unwrap().events[0].accepted);
        assert_eq!(no_material.snapshot().tool_wear.unwrap().conditions[0].damage, 4);
        let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
        host.enable_schedule(1000, 1).unwrap();
        let run = host.advance_scheduled(0, &[]).unwrap();
        assert_eq!(run.failure, None);
        assert!(run.events.iter().any(|event| matches!(event, game_economy::core_interval::CoreRunEvent::ToolRepairs(events) if events.iter().any(|event| event.item == 10 && event.accepted))));
        let repaired = host.snapshot();
        assert_eq!(repaired.tool_wear.as_ref().unwrap().conditions[0].damage, 0);
        let bag = repaired.core.physical.custody.containers.iter().find(|row| row.place == Place::Person(2)).unwrap();
        assert_eq!(bag.used, 1);
        assert_eq!(bag.lots[0].item, Some(10));
        let saved = serde_json::to_string(&repaired).unwrap();
        let mut host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
        assert_eq!(host.advance_continuous(1000, &[]).unwrap().failure, None);
        let state = host.snapshot();
        assert!(state.tool_wear.as_ref().unwrap().events.is_empty());
        state
    })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn work_requires_carried_tools_and_wears_only_physically_started_jobs() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.people[1].region = Some(1);
                initial.travel = None;
                initial.nutrition = None;
                initial.pending_food.clear();
                initial.physical.custody.containers.push(
                    Container::new(
                        Place::Person(2),
                        10,
                        vec![
                            Lot {
                                item: None,
                                good: 1,
                                quantity: 1,
                                expires_at: None,
                            },
                            Lot {
                                item: Some(10),
                                good: 2,
                                quantity: 1,
                                expires_at: None,
                            },
                        ],
                    )
                    .unwrap(),
                );
                initial
                    .physical
                    .custody
                    .containers
                    .sort_by_key(|row| row.place);
                let tool_wear = game_economy_work::tool_wear::ToolWear {
                    next_item: 11,
                    rules: vec![game_economy_work::tool_wear::Rule {
                        good: 2,
                        durability: 3,
                        wear: 3,
                        repair_good: 1,
                        repair_quantity: 1,
                    }],
                    conditions: vec![game_economy_work::tool_wear::Condition {
                        item: 10,
                        good: 2,
                        damage: 0,
                    }],
                    repairs: vec![game_economy_work::tool_wear::RepairChoice {
                        person: 2,
                        item: 10,
                        willing: true,
                    }],
                    events: vec![],
                };
                let requirements = vec![game_economy_work::tools::Requirement {
                    recipe: 1,
                    good: 2,
                    quantity: 1,
                }];
                let mut snapshot = CoreHostSnapshot {
                    core: initial,
                    autowork: None,
                    tool_wear: Some(tool_wear),
                    tool_requirements: requirements,
                    opinions: None,
                    goals: None,
                    agreements: None,
                    sites: None,
                    paused_work_rates: vec![],
                    fatigue: None,
                    activity: vec![],
                    agriculture: None,
                    health: None,
                    knowledge: None,
                    social: None,
                    claims: None,
                    food_search: None,
                    schedule: None,
                };
                let start = (
                    Start {
                        id: 2,
                        worker: 2,
                        region: 1,
                        recipe: 1,
                        units: 1,
                    },
                    2,
                );
                let mut without_stock = CoreHost::begin(snapshot.clone(), &[(1, 2)]).unwrap();
                assert_eq!(
                    without_stock.start_chosen_work(&[start]).unwrap(),
                    vec![(2, false)]
                );
                assert_eq!(
                    without_stock.snapshot().tool_wear.unwrap().conditions[0].damage,
                    0
                );
                let stock = snapshot
                    .core
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|row| row.place == Place::Stock { region: 1, good: 1 })
                    .unwrap();
                *stock = Container::new(
                    stock.place,
                    10,
                    vec![Lot {
                        item: None,
                        good: 1,
                        quantity: 4,
                        expires_at: None,
                    }],
                )
                .unwrap();
                let mut missing_tool = snapshot.clone();
                let bag = missing_tool
                    .core
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|row| row.place == Place::Person(2))
                    .unwrap();
                *bag = Container::new(
                    bag.place,
                    10,
                    vec![Lot {
                        item: None,
                        good: 1,
                        quantity: 1,
                        expires_at: None,
                    }],
                )
                .unwrap();
                missing_tool.core.physical.custody.containers.push(
                    Container::new(
                        Place::Person(1),
                        10,
                        vec![Lot {
                            item: Some(10),
                            good: 2,
                            quantity: 1,
                            expires_at: None,
                        }],
                    )
                    .unwrap(),
                );
                missing_tool
                    .core
                    .physical
                    .custody
                    .containers
                    .sort_by_key(|row| row.place);
                let mut missing_tool = CoreHost::begin(missing_tool, &[(1, 2)]).unwrap();
                assert_eq!(
                    missing_tool.start_chosen_work(&[start]).unwrap(),
                    vec![(2, false)]
                );
                assert_eq!(
                    missing_tool.snapshot().tool_wear.unwrap().conditions[0].damage,
                    0
                );
                let mut host = CoreHost::begin(snapshot, &[(1, 2)]).unwrap();
                assert_eq!(host.start_chosen_work(&[start]).unwrap(), vec![(2, true)]);
                assert_eq!(
                    host.snapshot().tool_wear.as_ref().unwrap().conditions[0].damage,
                    3
                );
                let saved = serde_json::to_string(&host.snapshot()).unwrap();
                let mut host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                host.progress(500).unwrap();
                assert_eq!(
                    host.publish_early_work().unwrap().unwrap().finished,
                    vec![2]
                );
                let next = (Start { id: 3, ..start.0 }, 1);
                assert_eq!(host.start_chosen_work(&[next]).unwrap(), vec![(3, false)]);
                assert!(host.repair_tools_autonomously(&[]).unwrap().events[0].accepted);
                assert_eq!(
                    host.snapshot().tool_wear.as_ref().unwrap().conditions[0].damage,
                    0
                );
                assert_eq!(host.start_chosen_work(&[next]).unwrap(), vec![(3, true)]);
                let state = host.snapshot();
                assert_eq!(state.tool_wear.as_ref().unwrap().conditions[0].damage, 3);
                state
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn work_pauses_when_a_required_tool_moves_and_resumes_after_its_return() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.people[1].region = Some(1);
                initial.travel = None;
                initial.nutrition = None;
                initial.pending_food.clear();
                let mut recipe = initial.physical.recipes[0].clone();
                recipe.id = 2;
                initial.physical.recipes.push(recipe);
                let stock = initial
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|row| row.place == Place::Stock { region: 1, good: 1 })
                    .unwrap();
                *stock = Container::new(
                    stock.place,
                    10,
                    vec![Lot {
                        item: None,
                        good: 1,
                        quantity: 2,
                        expires_at: None,
                    }],
                )
                .unwrap();
                initial.physical.custody.containers.extend([
                    Container::new(Place::Person(1), 10, vec![]).unwrap(),
                    Container::new(
                        Place::Person(2),
                        10,
                        vec![Lot {
                            item: Some(10),
                            good: 2,
                            quantity: 1,
                            expires_at: None,
                        }],
                    )
                    .unwrap(),
                ]);
                initial
                    .physical
                    .custody
                    .containers
                    .sort_by_key(|row| row.place);
                let tool_wear = game_economy_work::tool_wear::ToolWear {
                    next_item: 11,
                    rules: vec![game_economy_work::tool_wear::Rule {
                        good: 2,
                        durability: 1,
                        wear: 1,
                        repair_good: 1,
                        repair_quantity: 1,
                    }],
                    conditions: vec![game_economy_work::tool_wear::Condition {
                        item: 10,
                        good: 2,
                        damage: 0,
                    }],
                    repairs: vec![],
                    events: vec![],
                };
                let snapshot = CoreHostSnapshot {
                    core: initial,
                    autowork: None,
                    tool_wear: Some(tool_wear),
                    tool_requirements: vec![game_economy_work::tools::Requirement {
                        recipe: 2,
                        good: 2,
                        quantity: 1,
                    }],
                    paused_work_rates: vec![],
                    opinions: None,
                    goals: None,
                    agreements: None,
                    sites: None,
                    fatigue: None,
                    activity: vec![],
                    agriculture: None,
                    health: None,
                    knowledge: None,
                    social: None,
                    claims: None,
                    food_search: None,
                    schedule: None,
                };
                let scheduled_snapshot = snapshot.clone();
                let mut rested_snapshot = snapshot.clone();
                rested_snapshot.core.people[1].controlled = true;
                let mut owner = timed_fatigue();
                owner
                    .fatigue
                    .people
                    .push(game_economy_fatigue::FatiguePerson {
                        id: 2,
                        fatigue: 2,
                        resting: true,
                    });
                rested_snapshot.fatigue = Some(owner);
                let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
                assert_eq!(
                    host.start_chosen_work(&[(
                        Start {
                            id: 2,
                            worker: 2,
                            region: 1,
                            recipe: 2,
                            units: 1
                        },
                        1
                    )])
                    .unwrap(),
                    vec![(2, true)]
                );
                host.progress(200).unwrap();
                let transfer = |from, to| game_lots::Transaction {
                    id: 1,
                    movements: vec![game_lots::Movement {
                        id: 1,
                        from: Place::Person(from),
                        to: Some(Place::Person(to)),
                        good: 2,
                        quantity: 1,
                        item: Some(10),
                    }],
                    creations: vec![],
                };
                let mut rested = CoreHost::begin(rested_snapshot, &[(1, 1)]).unwrap();
                assert_eq!(
                    rested
                        .start_chosen_work(&[(
                            Start {
                                id: 2,
                                worker: 2,
                                region: 1,
                                recipe: 2,
                                units: 1,
                            },
                            1
                        )])
                        .unwrap(),
                    vec![(2, true)]
                );
                rested.progress(200).unwrap();
                assert_eq!(rested.snapshot().paused_work_rates, vec![(2, 1)]);
                assert!(
                    rested
                        .transfer_goods(&[transfer(2, 1)])
                        .unwrap()
                        .transactions[0]
                        .accepted
                );
                rested.set_controlled_rest(2, false).unwrap();
                rested.progress(100).unwrap();
                assert_eq!(
                    rested.snapshot().core.physical.work[1].progress_remainder,
                    0
                );
                assert!(
                    rested
                        .transfer_goods(&[transfer(1, 2)])
                        .unwrap()
                        .transactions[0]
                        .accepted
                );
                rested.progress(100).unwrap();
                assert_eq!(
                    rested.snapshot().core.physical.work[1].progress_remainder,
                    100
                );
                assert!(host.transfer_goods(&[transfer(2, 1)]).unwrap().transactions[0].accepted);
                host.progress(300).unwrap();
                let paused = host.snapshot();
                assert_eq!(paused.paused_work_rates, vec![(2, 1)]);
                assert_eq!(paused.core.physical.work[1].progress_remainder, 200);
                let saved = serde_json::to_string(&paused).unwrap();
                let mut host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                assert!(host.transfer_goods(&[transfer(1, 2)]).unwrap().transactions[0].accepted);
                host.progress(300).unwrap();
                let state = host.snapshot();
                assert!(state.paused_work_rates.is_empty());
                assert_eq!(state.core.physical.work[1].progress_remainder, 500);
                assert_eq!(state.tool_wear.as_ref().unwrap().conditions[0].damage, 1);
                let mut scheduled = CoreHost::begin(scheduled_snapshot, &[(1, 1)]).unwrap();
                scheduled
                    .start_chosen_work(&[(
                        Start {
                            id: 2,
                            worker: 2,
                            region: 1,
                            recipe: 2,
                            units: 1,
                        },
                        1,
                    )])
                    .unwrap();
                scheduled.enable_schedule(1000, 1).unwrap();
                assert_eq!(
                    scheduled.advance_continuous(200, &[]).unwrap().failure,
                    None
                );
                assert!(
                    scheduled
                        .transfer_goods(&[transfer(2, 1)])
                        .unwrap()
                        .transactions[0]
                        .accepted
                );
                assert_eq!(
                    scheduled.advance_continuous(800, &[]).unwrap().failure,
                    None
                );
                let at_boundary = scheduled.snapshot();
                assert_eq!(at_boundary.paused_work_rates, vec![(2, 1)]);
                let saved = serde_json::to_string(&at_boundary).unwrap();
                let mut scheduled =
                    CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                assert_eq!(scheduled.advance_continuous(1, &[]).unwrap().failure, None);
                assert!(
                    scheduled
                        .transfer_goods(&[transfer(1, 2)])
                        .unwrap()
                        .transactions[0]
                        .accepted
                );
                assert_eq!(
                    scheduled.advance_continuous(300, &[]).unwrap().failure,
                    None
                );
                let resumed = scheduled.snapshot();
                assert!(resumed.paused_work_rates.is_empty());
                let work = resumed
                    .core
                    .physical
                    .work
                    .iter()
                    .find(|row| row.job.id == 2)
                    .unwrap();
                assert_eq!(work.progress_remainder, 500);
                (state, resumed)
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
#[ignore = "manual identified tool repair scaling benchmark"]
fn core_host_tool_repair_benchmark() {
    use std::hash::{Hash, Hasher};
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(50_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    assert!(count > 1);
    let mut initial = fixture();
    let template = initial.people[0].clone();
    initial.people = (1..=count)
        .map(|id| {
            let mut person = template.clone();
            person.id = id;
            person
        })
        .collect();
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    initial
        .physical
        .custody
        .containers
        .extend((2..=count).map(|person| {
            Container::new(
                Place::Person(person),
                10,
                vec![
                    Lot {
                        item: Some(person),
                        good: 1,
                        quantity: 1,
                        expires_at: None,
                    },
                    Lot {
                        item: None,
                        good: 2,
                        quantity: 1,
                        expires_at: None,
                    },
                ],
            )
            .unwrap()
        }));
    initial
        .physical
        .custody
        .containers
        .sort_by_key(|row| row.place);
    let tool_wear = game_economy_work::tool_wear::ToolWear {
        next_item: count + 1,
        rules: vec![game_economy_work::tool_wear::Rule {
            good: 1,
            durability: 10,
            wear: 1,
            repair_good: 2,
            repair_quantity: 1,
        }],
        conditions: (2..=count)
            .map(|item| game_economy_work::tool_wear::Condition {
                item,
                good: 1,
                damage: 4,
            })
            .collect(),
        repairs: (2..=count)
            .map(|person| game_economy_work::tool_wear::RepairChoice {
                person,
                item: person,
                willing: true,
            })
            .collect(),
        events: vec![],
    };
    let snapshot = CoreHostSnapshot {
        core: initial,
        autowork: None,
        tool_wear: Some(tool_wear),
        opinions: None,
        goals: None,
        agreements: None,
        sites: None,
        tool_requirements: vec![],
        paused_work_rates: vec![],
        fatigue: None,
        activity: vec![],
        agriculture: None,
        health: None,
        knowledge: None,
        social: None,
        claims: None,
        food_search: None,
        schedule: None,
    };
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
            let start = std::time::Instant::now();
            let repairs = host.repair_tools_autonomously(&[]).unwrap();
            let elapsed = start.elapsed();
            assert_eq!(repairs.events.len(), (count - 1) as usize);
            assert!(repairs.events.iter().all(|event| event.accepted));
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            serde_json::to_vec(&host.snapshot())
                .unwrap()
                .hash(&mut hash);
            println!("fingerprint={:016x}", hash.finish());
            println!(
                "workers={workers} tool_repairs={} elapsed={:.6}",
                repairs.events.len(),
                elapsed.as_secs_f64()
            );
        });
}

#[test]
#[ignore = "manual site admission scaling benchmark"]
fn core_host_site_admission_benchmark() {
    use std::hash::{Hash, Hasher};
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(50_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let scarce = std::env::var_os("LEGACY_SITE_SCARCE").is_some();
    let work_claims = std::env::var_os("LEGACY_WORK_CLAIMS").is_some();
    let fatigue = std::env::var_os("LEGACY_FATIGUE").is_some();
    let steps = std::env::var("LEGACY_FATIGUE_STEPS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(1);
    assert!(steps > 0 && 1000 % steps == 0);
    let settle = std::env::var_os("LEGACY_SITE_SETTLE").is_some() || fatigue;
    let tools = std::env::var_os("LEGACY_TOOLS").is_some();
    let pause_tools = std::env::var_os("LEGACY_TOOL_PAUSE").is_some();
    assert!(!scarce || !settle);
    assert!(!pause_tools || (tools && !settle && !scarce));
    assert!(count > 1);
    let mut initial = fixture();
    let template = initial.people[0].clone();
    initial.people = (1..=count)
        .map(|id| {
            let mut person = template.clone();
            person.id = id;
            person.willing_to_work = true;
            person
        })
        .collect();
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    if tools {
        initial
            .physical
            .custody
            .containers
            .extend((2..=count).map(|person| {
                Container::new(
                    Place::Person(person),
                    10,
                    vec![Lot {
                        item: Some(person),
                        good: 2,
                        quantity: 1,
                        expires_at: None,
                    }],
                )
                .unwrap()
            }));
        initial
            .physical
            .custody
            .containers
            .sort_by_key(|row| row.place);
    }
    if pause_tools {
        initial
            .physical
            .custody
            .containers
            .push(Container::new(Place::Person(1), count, vec![]).unwrap());
        initial
            .physical
            .custody
            .containers
            .sort_by_key(|row| row.place);
    }
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        count * 2,
        vec![Lot {
            item: None,
            good: 1,
            quantity: (count - 1) * 2,
            expires_at: None,
        }],
    )
    .unwrap();
    let facilities = game_economy_work::sites::Facilities {
        next_id: if scarce { 2 } else { count + 1 },
        sites: (1..=if scarce { 1 } else { count })
            .map(|id| game_economy_work::sites::Site {
                id,
                region: 1,
                kind: "mill".into(),
                capacity: if scarce { count / 2 } else { 1 },
            })
            .collect(),
        requirements: vec![game_economy_work::sites::Requirement {
            recipe: 1,
            kind: "mill".into(),
            slots: 1,
        }],
        bindings: vec![game_economy_work::sites::Binding {
            job: 1,
            site: 1,
            slots: 1,
        }],
    };
    let maintenance = game_economy_building::maintenance::Maintenance {
        rules: vec![game_economy_building::maintenance::Rule {
            kind: "mill".into(),
            wear_limit: 10,
            wear_per_slot: 1,
            repair_good: 1,
            repair_quantity: 1,
        }],
        conditions: vec![],
        policies: vec![],
        events: vec![],
    };
    let auto = game_autowork::Autowork {
        next_id: 2,
        policies: (2..=count)
            .map(|person| game_autowork::Policy {
                person,
                recipe: 1,
                enabled: true,
                disregard_claims: work_claims,
            })
            .collect(),
        requests: None,
    };
    let snapshot = CoreHostSnapshot {
        core: initial,
        autowork: Some(auto),
        tool_wear: tools.then(|| game_economy_work::tool_wear::ToolWear {
            next_item: count + 1,
            rules: vec![game_economy_work::tool_wear::Rule {
                good: 2,
                durability: 10,
                wear: 1,
                repair_good: 1,
                repair_quantity: 1,
            }],
            conditions: (2..=count)
                .map(|item| game_economy_work::tool_wear::Condition {
                    item,
                    good: 2,
                    damage: 0,
                })
                .collect(),
            repairs: vec![],
            events: vec![],
        }),
        opinions: None,
        goals: None,
        agreements: None,
        sites: Some(game_economy::core_interval::CoreSites {
            facilities,
            maintenance: Some(maintenance),
        }),
        tool_requirements: if tools {
            vec![game_economy_work::tools::Requirement {
                recipe: 1,
                good: 2,
                quantity: 1,
            }]
        } else {
            vec![]
        },
        paused_work_rates: vec![],
        fatigue: fatigue.then(timed_fatigue),
        activity: vec![],
        agriculture: None,
        health: None,
        knowledge: None,
        social: None,
        claims: work_claims.then(|| game_economy_claims::Claims {
            claims: vec![game_economy_claims::Claim {
                person: 1,
                region: 1,
                good: 1,
            }],
            ..Default::default()
        }),
        food_search: None,
        schedule: None,
    };
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
            let start = std::time::Instant::now();
            let outcomes = host.start_autonomous_work(&[], 1).unwrap();
            let mut elapsed = start.elapsed();
            assert_eq!(outcomes.len(), (count - 1) as usize);
            if tools {
                let state = host.snapshot();
                assert_eq!(
                    state
                        .tool_wear
                        .as_ref()
                        .unwrap()
                        .conditions
                        .iter()
                        .filter(|condition| condition.damage == 1)
                        .count(),
                    outcomes.iter().filter(|outcome| outcome.1).count()
                );
            }
            assert_eq!(
                outcomes.iter().filter(|outcome| outcome.1).count(),
                if scarce {
                    (count / 2 - 1) as usize
                } else {
                    (count - 1) as usize
                }
            );
            if settle {
                let start = std::time::Instant::now();
                for _ in 0..steps {
                    host.progress(1000 / steps).unwrap();
                }
                let progress = start.elapsed();
                let start = std::time::Instant::now();
                let events = host.settle().unwrap();
                elapsed = start.elapsed()
                    + if fatigue {
                        progress
                    } else {
                        Default::default()
                    };
                assert_eq!(events.finished.len(), count as usize);
                assert!(
                    host.snapshot()
                        .sites
                        .unwrap()
                        .maintenance
                        .unwrap()
                        .conditions
                        .iter()
                        .all(|condition| condition.wear == 1)
                );
            }
            if pause_tools {
                host.progress(1).unwrap();
                let actions: Vec<_> = (2..=count)
                    .map(|person| game_lots::Transaction {
                        id: person - 1,
                        movements: vec![game_lots::Movement {
                            id: person - 1,
                            from: Place::Person(person),
                            to: Some(Place::Person(1)),
                            good: 2,
                            quantity: 1,
                            item: Some(person),
                        }],
                        creations: vec![],
                    })
                    .collect();
                let start = std::time::Instant::now();
                let transfers = host.transfer_goods(&actions).unwrap();
                assert!(transfers.transactions.iter().all(|row| row.accepted));
                host.progress(1).unwrap();
                elapsed = start.elapsed();
                assert_eq!(
                    host.snapshot().paused_work_rates.len(),
                    (count - 1) as usize
                );
            }
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            serde_json::to_vec(&host.snapshot())
                .unwrap()
                .hash(&mut hash);
            println!("fingerprint={:016x}", hash.finish());
            println!(
                "workers={workers} site_starts={} accepted={} settle={settle} elapsed={:.6}",
                outcomes.len(),
                outcomes.iter().filter(|outcome| outcome.1).count(),
                elapsed.as_secs_f64()
            );
        });
}

#[test]
#[ignore = "manual site repair scaling benchmark"]
fn core_host_site_repair_benchmark() {
    use std::hash::{Hash, Hasher};
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(50_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    assert!(count > 1);
    let mut initial = fixture();
    let template = initial.people[0].clone();
    initial.people = (1..=count)
        .map(|id| {
            let mut person = template.clone();
            person.id = id;
            person
        })
        .collect();
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    initial
        .physical
        .custody
        .containers
        .extend((2..=count).map(|person| {
            Container::new(
                Place::Person(person),
                10,
                vec![Lot {
                    item: None,
                    good: 1,
                    quantity: 1,
                    expires_at: None,
                }],
            )
            .unwrap()
        }));
    initial
        .physical
        .custody
        .containers
        .sort_by_key(|row| row.place);
    let facilities = game_economy_work::sites::Facilities {
        next_id: count + 1,
        sites: (1..=count)
            .map(|id| game_economy_work::sites::Site {
                id,
                region: 1,
                kind: "mill".into(),
                capacity: 1,
            })
            .collect(),
        requirements: vec![game_economy_work::sites::Requirement {
            recipe: 1,
            kind: "mill".into(),
            slots: 1,
        }],
        bindings: vec![game_economy_work::sites::Binding {
            job: 1,
            site: 1,
            slots: 1,
        }],
    };
    let maintenance = game_economy_building::maintenance::Maintenance {
        rules: vec![game_economy_building::maintenance::Rule {
            kind: "mill".into(),
            wear_limit: 1,
            wear_per_slot: 1,
            repair_good: 1,
            repair_quantity: 1,
        }],
        conditions: (1..=count)
            .map(|site| game_economy_building::maintenance::Condition { site, wear: 1 })
            .collect(),
        policies: (2..=count)
            .map(|person| game_economy_building::maintenance::Policy {
                person,
                site: person,
                willing: true,
                pressured_by: None,
                pressure_threshold: None,
                disregard_claims: false,
            })
            .collect(),
        events: vec![],
    };
    let snapshot = CoreHostSnapshot {
        core: initial,
        autowork: None,
        tool_wear: None,
        opinions: None,
        goals: None,
        agreements: None,
        sites: Some(game_economy::core_interval::CoreSites {
            facilities,
            maintenance: Some(maintenance),
        }),
        tool_requirements: vec![],
        paused_work_rates: vec![],
        fatigue: None,
        activity: vec![],
        agriculture: None,
        health: None,
        knowledge: None,
        social: None,
        claims: None,
        food_search: None,
        schedule: None,
    };
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
            let start = std::time::Instant::now();
            let repairs = host.repair_sites_autonomously(&[]).unwrap();
            let elapsed = start.elapsed();
            assert_eq!(repairs.events.len(), (count - 1) as usize);
            assert!(repairs.events.iter().all(|event| event.repaired));
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            serde_json::to_vec(&host.snapshot())
                .unwrap()
                .hash(&mut hash);
            println!("fingerprint={:016x}", hash.finish());
            println!(
                "workers={workers} site_repairs={} elapsed={:.6}",
                repairs.events.len(),
                elapsed.as_secs_f64()
            );
        });
}

#[test]
#[ignore = "manual personal proposal and acceptance scaling benchmark"]
fn core_host_offer_choices_benchmark() {
    use std::hash::{Hash, Hasher};
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(50_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    assert!(count >= 4 && count.is_multiple_of(2));
    let mut initial = fixture();
    let template = initial.people[0].clone();
    initial.people = (1..=count)
        .map(|id| {
            let mut person = template.clone();
            person.id = id;
            person
        })
        .collect();
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    let offering = (1..=count / 2)
        .map(|id| game_agreements::OfferPolicy {
            person: id * 2 - 1,
            recipient: id * 2,
            good: 2,
            quantity: 1,
            due_after: 2,
            retry_ticks: 3,
            willing: true,
            recipient_hunger_at_least: 0,
            minimum_trust: None,
            pressure_threshold: None,
            next_review: 0,
            after_work_recipe: None,
            last_work_offer_at: None,
            after_help: false,
            last_help_offer_at: None,
        })
        .collect();
    let policies = (1..=count / 2)
        .map(|id| game_agreements::Policy {
            person: id * 2,
            honor: false,
            accept: true,
            minimum_trust: None,
            minimum_honor_trust: None,
            fear_honor_threshold: None,
            obligation_honor_threshold: None,
            accept_pressure_threshold: None,
        })
        .collect();
    let agreements = game_agreements::Agreements {
        next_id: 1,
        policies,
        offering,
        work_offering: vec![],
        bulk_debtors: vec![],
        contracts: vec![],
        events: vec![],
    };
    let snapshot = CoreHostSnapshot {
        core: initial,
        autowork: None,
        tool_wear: None,
        opinions: None,
        goals: None,
        agreements: Some(agreements),
        sites: None,
        tool_requirements: vec![],
        paused_work_rates: vec![],
        fatigue: None,
        activity: vec![],
        agriculture: None,
        health: None,
        knowledge: None,
        social: None,
        claims: None,
        food_search: None,
        schedule: None,
    };
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
            let start = std::time::Instant::now();
            let offers = host.propose_agreements_autonomously(&[]).unwrap();
            let accepted = host.accept_agreements_autonomously(&offers.actors).unwrap();
            let elapsed = start.elapsed();
            assert_eq!(offers.agreements.len(), count as usize / 2 - 1);
            assert_eq!(accepted.agreements.len(), offers.agreements.len());
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            serde_json::to_vec(&host.snapshot())
                .unwrap()
                .hash(&mut hash);
            println!("fingerprint={:016x}", hash.finish());
            println!(
                "workers={workers} offers={} elapsed={:.6}",
                offers.agreements.len(),
                elapsed.as_secs_f64()
            );
        });
}

#[test]
#[ignore = "manual voluntary agreement delivery scaling benchmark"]
fn core_host_agreement_delivery_benchmark() {
    use std::hash::{Hash, Hasher};
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(50_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let automatic = std::env::var_os("LEGACY_AUTOMATIC").is_some();
    assert!(count >= 4 && count.is_multiple_of(2));
    let mut initial = fixture();
    let template = initial.people[0].clone();
    initial.people = (1..=count)
        .map(|id| {
            let mut person = template.clone();
            person.id = id;
            person
        })
        .collect();
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    initial
        .physical
        .custody
        .containers
        .extend((1..=count).map(|person| {
            Container::new(
                Place::Person(person),
                10,
                if !person.is_multiple_of(2) {
                    vec![Lot {
                        item: None,
                        good: 1,
                        quantity: 1,
                        expires_at: None,
                    }]
                } else {
                    vec![]
                },
            )
            .unwrap()
        }));
    initial
        .physical
        .custody
        .containers
        .sort_by_key(|row| row.place);
    let agreements = game_agreements::Agreements {
        next_id: count / 2 + 1,
        policies: if automatic {
            (1..=count / 2)
                .map(|id| game_agreements::Policy {
                    person: id * 2 - 1,
                    honor: true,
                    accept: false,
                    minimum_trust: None,
                    minimum_honor_trust: None,
                    fear_honor_threshold: None,
                    obligation_honor_threshold: None,
                    accept_pressure_threshold: None,
                })
                .collect()
        } else {
            vec![]
        },
        offering: vec![],
        work_offering: vec![],
        bulk_debtors: vec![],
        contracts: (1..=count / 2)
            .map(|id| game_agreements::Contract {
                id,
                debtor: id * 2 - 1,
                creditor: id * 2,
                good: 1,
                quantity: 1,
                delivered: 0,
                due: 5,
                status: game_agreements::Status::Active,
            })
            .collect(),
        events: vec![],
    };
    let deliveries: Vec<_> = (1..=count / 2)
        .map(|contract| game_agreements::Delivery {
            contract,
            actor: contract * 2 - 1,
            quantity: 1,
        })
        .collect();
    let snapshot = CoreHostSnapshot {
        core: initial,
        autowork: None,
        tool_wear: None,
        opinions: None,
        goals: None,
        agreements: Some(agreements),
        sites: None,
        tool_requirements: vec![],
        paused_work_rates: vec![],
        fatigue: None,
        activity: vec![],
        agriculture: None,
        health: None,
        knowledge: None,
        social: None,
        claims: None,
        food_search: None,
        schedule: None,
    };
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
            host.progress(1).unwrap();
            let start = std::time::Instant::now();
            let outcomes = if automatic {
                host.deliver_agreements_autonomously(&[]).unwrap().outcomes
            } else {
                host.deliver_agreements(&deliveries).unwrap()
            };
            let elapsed = start.elapsed();
            assert_eq!(outcomes.len(), count as usize / 2 - usize::from(automatic));
            assert!(outcomes.iter().all(|row| row.accepted));
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            serde_json::to_vec(&host.snapshot())
                .unwrap()
                .hash(&mut hash);
            println!("fingerprint={:016x}", hash.finish());
            println!(
                "workers={workers} deliveries={} elapsed={:.6}",
                outcomes.len(),
                elapsed.as_secs_f64()
            );
        });
}

#[test]
fn core_host_physical_taking_preserves_item_identity_without_implied_permission() {
    let mut initial = fixture();
    initial.people[1].region = Some(1);
    initial.people[1].health = 0;
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    initial.physical.custody.containers.extend([
        Container::new(Place::Person(1), 2, vec![]).unwrap(),
        Container::new(
            Place::Person(2),
            2,
            vec![Lot {
                item: Some(10),
                good: 1,
                quantity: 1,
                expires_at: None,
            }],
        )
        .unwrap(),
    ]);
    initial
        .physical
        .custody
        .containers
        .sort_by_key(|row| row.place);
    let owner = game_economy_work::tool_wear::ToolWear {
        next_item: 11,
        rules: vec![game_economy_work::tool_wear::Rule {
            good: 1,
            durability: 10,
            wear: 1,
            repair_good: 2,
            repair_quantity: 1,
        }],
        conditions: vec![game_economy_work::tool_wear::Condition {
            item: 10,
            good: 1,
            damage: 2,
        }],
        repairs: vec![],
        events: vec![],
    };
    let mut host = CoreHost::begin(
        CoreHostSnapshot {
            core: initial,
            autowork: None,
            tool_wear: Some(owner),
            opinions: None,
            goals: None,
            agreements: None,
            sites: None,
            tool_requirements: vec![],
            paused_work_rates: vec![],
            fatigue: None,
            activity: vec![],
            agriculture: None,
            health: None,
            knowledge: None,
            social: None,
            claims: None,
            food_search: None,
            schedule: None,
        },
        &[(1, 1)],
    )
    .unwrap();
    host.progress(100).unwrap();
    let action = game_lots::Transaction {
        id: 1,
        movements: vec![game_lots::Movement {
            id: 1,
            from: Place::Person(2),
            to: Some(Place::Person(1)),
            good: 1,
            quantity: 1,
            item: Some(10),
        }],
        creations: vec![],
    };
    assert!(host.transfer_goods(&[action]).unwrap().transactions[0].accepted);
    let snapshot = host.snapshot();
    let physical = Runtime::compile(snapshot.core.physical.clone()).unwrap();
    assert_eq!(physical.item_place(10), Some(Place::Person(1)));
    let owner = snapshot.tool_wear.unwrap();
    assert_eq!(owner.conditions[0].damage, 2);
    owner
        .validate(
            &physical,
            &[1, 2].into_iter().collect(),
            &[1, 2].into_iter().collect(),
            0,
        )
        .unwrap();
}

#[test]
fn core_host_rejects_unowned_identified_consumption_without_mutation() {
    let mut initial = fixture();
    let input = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::JobInput(1))
        .unwrap();
    *input = Container::new(
        input.place,
        2,
        vec![
            Lot {
                item: Some(10),
                good: 1,
                quantity: 1,
                expires_at: None,
            },
            Lot {
                item: Some(11),
                good: 1,
                quantity: 1,
                expires_at: None,
            },
        ],
    )
    .unwrap();
    let mut host = CoreHost::begin(
        CoreHostSnapshot {
            core: initial,
            autowork: None,
            tool_wear: None,
            opinions: None,
            goals: None,
            agreements: None,
            sites: None,
            tool_requirements: vec![],
            paused_work_rates: vec![],
            fatigue: None,
            activity: vec![],
            agriculture: None,
            health: None,
            knowledge: None,
            social: None,
            claims: None,
            food_search: None,
            schedule: None,
        },
        &[(1, 2)],
    )
    .unwrap();
    host.progress(500).unwrap();
    let before = host.snapshot();
    assert!(host.publish_early_work().is_err());
    assert_eq!(host.snapshot(), before);
}

#[test]
fn scheduled_core_host_is_partition_independent_at_tied_deadlines() {
    let run = |workers, chunks: &[u64], reload| {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
        let mut initial = fixture();
        initial.people[0].willing_to_work = true;
        initial.people[1].willing_to_work = true;
        initial.travel.as_mut().unwrap().journeys[0].progress_millis = 500;
        let stock = initial.physical.custody.containers.iter_mut().find(|row| row.place == Place::Stock { region: 2, good: 1 }).unwrap();
        *stock = Container::new(stock.place, 10, vec![Lot { item: None, good: 1, quantity: 2, expires_at: None }]).unwrap();
        let snapshot = CoreHostSnapshot {
            core: initial,
            autowork: Some(game_autowork::Autowork { next_id: 2, policies: vec![game_autowork::Policy { person: 2, recipe: 1, enabled: true, disregard_claims: false }], requests: None }),
            tool_wear: None, opinions: None, goals: None, agreements: None, sites: None, tool_requirements: vec![], paused_work_rates: vec![], fatigue: None, activity: vec![], agriculture: None, health: None, knowledge: None, social: None, claims: None, food_search: None, schedule: None,
        };
        let mut host = CoreHost::begin(snapshot, &[(1, 2)]).unwrap();
        host.progress(100).unwrap();
        host.enable_schedule(200, 2).unwrap();
        let mut events = Vec::new();
        let mut decisions = 0;
        for &chunk in chunks {
            let result = host.advance_scheduled(chunk, &[]).unwrap();
            assert_eq!(result.elapsed_millis, chunk);
            assert_eq!(result.failure, None);
            decisions += result.decisions;
            events.extend(result.events);
            if reload {
                let saved = serde_json::to_string(&host.snapshot()).unwrap();
                host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
            }
        }
        assert_eq!(decisions, 5);
        assert!(matches!(&events[0], game_economy::core_interval::CoreRunEvent::Arrivals(arrival) if arrival.people == vec![2]));
        assert!(matches!(&events[1], game_economy::core_interval::CoreRunEvent::Work(work) if work.finished == vec![1]));
        assert!(matches!(&events[2], game_economy::core_interval::CoreRunEvent::Starts(starts) if starts == &vec![(2, true)]));
        let boundary = host.snapshot();
        assert_eq!(boundary.core.physical.work_interval.as_ref().unwrap().elapsed_millis, 1000);
        let events_at_boundary = host.settle().unwrap();
        assert_eq!(events_at_boundary.finished, vec![2]);
        assert_eq!(events_at_boundary.delivered, vec![1]);
        (host.snapshot(), events)
    })
    };
    assert_eq!(run(1, &[900], false), run(6, &[200, 200, 500], true));
}

#[test]
fn scheduled_decision_resumes_after_work_failure_without_repeating_meal() {
    let mut initial = fixture();
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    initial.people[1].region = Some(1);
    initial.people[1].seek_meals = true;
    initial.people[1].willing_to_work = true;
    let mut third = initial.people[0].clone();
    third.id = 3;
    third.willing_to_work = true;
    initial.people.push(third);
    initial.physical.goods[1].food = Some(game_production_model::FoodProfile {
        portion: 1,
        hunger_relief: 1,
        nutrients: Default::default(),
        health_effect: 0,
        shelf_days: None,
    });
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        10,
        vec![Lot {
            item: None,
            good: 1,
            quantity: 2,
            expires_at: None,
        }],
    )
    .unwrap();
    initial.physical.custody.containers.push(
        Container::new(
            Place::Person(2),
            2,
            vec![Lot {
                item: None,
                good: 2,
                quantity: 2,
                expires_at: None,
            }],
        )
        .unwrap(),
    );
    initial
        .physical
        .custody
        .containers
        .sort_by_key(|row| row.place);
    let snapshot = CoreHostSnapshot {
        core: initial,
        tool_wear: None,
        opinions: None,
        goals: None,
        agreements: None,
        sites: None,
        tool_requirements: vec![],
        paused_work_rates: vec![],
        fatigue: None,
        activity: vec![],
        agriculture: None,
        health: None,
        knowledge: None,
        social: None,
        claims: None,
        food_search: None,
        schedule: None,
        autowork: Some(game_autowork::Autowork {
            next_id: u64::MAX,
            policies: vec![game_autowork::Policy {
                person: 3,
                recipe: 1,
                enabled: true,
                disregard_claims: false,
            }],
            requests: None,
        }),
    };
    let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
    host.progress(100).unwrap();
    host.enable_schedule(200, 1).unwrap();
    let result = host.advance_scheduled(0, &[]).unwrap();
    assert_eq!(result.elapsed_millis, 0);
    assert!(result.failure.is_some());
    assert!(
        matches!(&result.events[..], [game_economy::core_interval::CoreRunEvent::Meals(meals)] if meals == &vec![(2, 2)])
    );
    let mut saved = host.snapshot();
    assert!(matches!(
        saved.schedule.as_ref().unwrap().phase,
        game_economy::core_interval::DecisionPhase::Work { .. }
    ));
    saved.autowork.as_mut().unwrap().next_id = 2;
    let saved = serde_json::to_string(&saved).unwrap();
    let mut host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
    let result = host.advance_scheduled(0, &[]).unwrap();
    assert_eq!(result.failure, None);
    assert_eq!(result.decisions, 1);
    assert!(
        matches!(&result.events[..], [game_economy::core_interval::CoreRunEvent::Starts(starts)] if starts == &vec![(2, true)])
    );
    let snapshot = host.snapshot();
    assert_eq!(
        snapshot
            .core
            .physical
            .custody
            .containers
            .iter()
            .find(|row| row.place == Place::Person(2))
            .unwrap()
            .used,
        1
    );
    assert_eq!(snapshot.core.people[1].hunger, 1);
    assert_eq!(snapshot.core.physical.work[1].job.worker, 3);
}

#[test]
fn scheduled_runner_defers_perishable_work_and_publishes_other_completion() {
    let mut initial = fixture();
    initial.people[1].region = Some(1);
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    initial.physical.goods.push(Good {
        id: 3,
        name: "perishable".into(),
        food: Some(game_production_model::FoodProfile {
            portion: 1,
            hunger_relief: 1,
            nutrients: Default::default(),
            health_effect: 0,
            shelf_days: Some(1),
        }),
    });
    let mut recipe = initial.physical.recipes[0].clone();
    recipe.id = 2;
    recipe.outputs[0].good = 3;
    initial.physical.recipes.push(recipe);
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        10,
        vec![Lot {
            item: None,
            good: 1,
            quantity: 2,
            expires_at: None,
        }],
    )
    .unwrap();
    initial
        .physical
        .custody
        .containers
        .push(Container::new(Place::Stock { region: 1, good: 3 }, 10, vec![]).unwrap());
    initial
        .physical
        .custody
        .containers
        .sort_by_key(|row| row.place);
    let mut physical = Runtime::compile(initial.physical).unwrap();
    physical
        .start(
            &[Start {
                id: 2,
                worker: 2,
                region: 1,
                recipe: 2,
                units: 1,
            }],
            7,
        )
        .unwrap();
    initial.physical = physical.snapshot();
    let mut host = CoreHost::begin(
        CoreHostSnapshot {
            core: initial,
            autowork: None,
            tool_wear: None,
            opinions: None,
            goals: None,
            agreements: None,
            sites: None,
            tool_requirements: vec![],
            paused_work_rates: vec![],
            fatigue: None,
            activity: vec![],
            agriculture: None,
            health: None,
            knowledge: None,
            social: None,
            claims: None,
            food_search: None,
            schedule: None,
        },
        &[(1, 2), (2, 2)],
    )
    .unwrap();
    host.progress(100).unwrap();
    host.enable_schedule(600, 1).unwrap();
    let result = host.advance_scheduled(900, &[]).unwrap();
    assert_eq!(result.failure, None);
    assert!(result.boundary);
    assert!(
        matches!(&result.events[0], game_economy::core_interval::CoreRunEvent::Deferred(jobs) if jobs == &vec![2])
    );
    assert!(
        matches!(&result.events[1], game_economy::core_interval::CoreRunEvent::Work(events) if events.finished == vec![1])
    );
    assert_eq!(host.settle().unwrap().finished, vec![2]);
}

#[test]
fn continuous_core_host_crosses_boundaries_without_partition_or_reload_drift() {
    let run = |workers, chunks: &[u64], reload| {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
        let mut initial = fixture();
        initial.people[0].willing_to_work = true;
        initial.people[1].region = Some(1);
        initial.people[1].willing_to_work = true;
        initial.travel = None;
        initial.nutrition = None;
        initial.pending_food.clear();
        let stock = initial.physical.custody.containers.iter_mut().find(|row| row.place == Place::Stock { region: 1, good: 1 }).unwrap();
        *stock = Container::new(stock.place, 10, vec![Lot { item: None, good: 1, quantity: 2, expires_at: None }]).unwrap();
        let tool_wear = game_economy_work::tool_wear::ToolWear { next_item: 10,
            rules: vec![game_economy_work::tool_wear::Rule { good: 2, durability: 10, wear: 1, repair_good: 1, repair_quantity: 1 }],
            conditions: vec![], repairs: vec![], events: vec![] };
        let snapshot = CoreHostSnapshot { core: initial, tool_wear: Some(tool_wear), opinions: None, goals: None, agreements: None, sites: None, tool_requirements: vec![], paused_work_rates: vec![], fatigue: None, activity: vec![], agriculture: None, health: None, knowledge: None, social: None, claims: None, food_search: None, schedule: None,
            autowork: Some(game_autowork::Autowork { next_id: 2, policies: vec![game_autowork::Policy { person: 2, recipe: 1, enabled: true, disregard_claims: false }], requests: None }) };
        let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
        host.enable_schedule(500, 1).unwrap();
        let mut events = Vec::new();
        let mut boundaries = 0;
        let mut decisions = 0;
        for &chunk in chunks {
            let result = host.advance_continuous(chunk, &[]).unwrap();
            assert_eq!(result.failure, None);
            assert_eq!(result.elapsed_millis, chunk);
            boundaries += result.boundaries;
            decisions += result.decisions;
            events.extend(result.events);
            if reload {
                let saved = serde_json::to_string(&host.snapshot()).unwrap();
                host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
            }
        }
        assert_eq!(boundaries, 2);
        assert_eq!(decisions, 5);
        let snapshot = host.snapshot();
        assert_eq!(snapshot.core.physical.custody.tick, 2);
        assert_eq!(snapshot.core.physical.work_interval.as_ref().unwrap().elapsed_millis, 400);
        let physical = Runtime::compile(snapshot.core.physical.clone()).unwrap();
        assert_eq!(physical.item_place(10), Some(Place::Stock { region: 1, good: 2 }));
        assert_eq!(physical.item_place(11), Some(Place::Stock { region: 1, good: 2 }));
        snapshot.tool_wear.as_ref().unwrap().validate(&physical, &[1,2].into_iter().collect(), &[1,2].into_iter().collect(), 2).unwrap();
        assert!(matches!(&events[0], game_economy::core_interval::CoreRunEvent::Starts(rows) if rows == &vec![(2, true)]));
        (snapshot, events)
    })
    };
    assert_eq!(
        run(1, &[2400], false),
        run(6, &[400, 600, 0, 300, 700, 400], true)
    );
}

#[test]
fn long_decision_period_survives_save_and_crosses_a_boundary() {
    let snapshot = CoreHostSnapshot {
        core: fixture(),
        autowork: None,
        tool_wear: None,
        opinions: None,
        goals: None,
        agreements: None,
        sites: None,
        tool_requirements: vec![],
        paused_work_rates: vec![],
        fatigue: None,
        activity: vec![],
        agriculture: None,
        health: None,
        knowledge: None,
        social: None,
        claims: None,
        food_search: None,
        schedule: None,
    };
    let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
    host.progress(100).unwrap();
    host.enable_schedule(u64::MAX, 1).unwrap();
    assert_eq!(host.advance_continuous(0, &[]).unwrap().decisions, 1);
    let before = host.snapshot();
    assert!(
        before
            .schedule
            .as_ref()
            .unwrap()
            .next_decision_millis
            .unwrap()
            > u128::from(u64::MAX)
    );
    let saved = serde_json::to_string(&before).unwrap();
    let mut host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
    assert_eq!(host.snapshot(), before);
    let result = host.advance_continuous(1000, &[]).unwrap();
    assert_eq!(result.failure, None);
    assert_eq!(result.boundaries, 1);
    assert_eq!(result.decisions, 0);
}

#[test]
fn continuous_host_carries_long_work_rate_and_fractional_progress() {
    let run = |chunks: &[u64], reload| {
        let mut initial = fixture();
        initial.physical.recipes[0].ticks = 3;
        initial.physical.work[0].job.remaining = 3;
        let mut host = CoreHost::begin(
            CoreHostSnapshot {
                core: initial,
                autowork: None,
                tool_wear: None,
                opinions: None,
                goals: None,
                agreements: None,
                sites: None,
                tool_requirements: vec![],
                paused_work_rates: vec![],
                fatigue: None,
                activity: vec![],
                agriculture: None,
                health: None,
                knowledge: None,
                social: None,
                claims: None,
                food_search: None,
                schedule: None,
            },
            &[(1, 1)],
        )
        .unwrap();
        host.enable_schedule(10_000, 1).unwrap();
        let mut events = Vec::new();
        for &chunk in chunks {
            let run = host.advance_continuous(chunk, &[]).unwrap();
            assert_eq!(run.failure, None);
            events.extend(run.events);
            if reload {
                let saved = serde_json::to_string(&host.snapshot()).unwrap();
                host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
            }
        }
        let snapshot = host.snapshot();
        assert_eq!(snapshot.core.physical.custody.tick, 3);
        assert_eq!(
            snapshot
                .core
                .physical
                .work_interval
                .as_ref()
                .unwrap()
                .elapsed_millis,
            0
        );
        assert!(events.iter().any(|event| matches!(event, game_economy::core_interval::CoreRunEvent::Boundary(events) if events.finished == vec![1])));
        (snapshot, events)
    };
    assert_eq!(run(&[3000], false), run(&[333, 667, 1300, 700], true));
}

#[test]
fn continuous_host_retains_rate_for_work_paused_at_a_boundary() {
    let mut initial = fixture();
    initial.physical.recipes[0].ticks = 3;
    initial.physical.work[0].job.remaining = 3;
    let mut host = CoreHost::begin(
        CoreHostSnapshot {
            core: initial,
            autowork: None,
            tool_wear: None,
            opinions: None,
            goals: None,
            agreements: None,
            sites: None,
            tool_requirements: vec![],
            paused_work_rates: vec![],
            fatigue: None,
            activity: vec![],
            agriculture: None,
            health: None,
            knowledge: None,
            social: None,
            claims: None,
            food_search: None,
            schedule: None,
        },
        &[(1, 1)],
    )
    .unwrap();
    host.enable_schedule(10_000, 1).unwrap();
    assert_eq!(host.advance_continuous(1000, &[1]).unwrap().failure, None);
    let paused = host.snapshot();
    assert_eq!(paused.core.physical.work[0].job.remaining, 2);
    assert!(
        paused
            .core
            .physical
            .work_interval
            .as_ref()
            .unwrap()
            .rates
            .is_empty()
    );
    assert_eq!(
        paused.schedule.as_ref().unwrap().carried_rates,
        vec![(1, 1)]
    );
    assert_eq!(host.advance_continuous(1000, &[]).unwrap().failure, None);
    let resumed = host.snapshot();
    assert_eq!(resumed.core.physical.work[0].job.remaining, 2);
    assert_eq!(
        resumed.core.physical.work_interval.as_ref().unwrap().rates,
        vec![(1, 1)]
    );
    assert_eq!(host.advance_continuous(1000, &[]).unwrap().failure, None);
    assert_eq!(host.snapshot().core.physical.work[0].job.remaining, 1);
}

#[test]
fn continuous_host_keeps_completed_interval_after_boundary_failure() {
    let mut initial = fixture();
    initial.physical.custody.tick = u64::MAX;
    initial.people[1].region = Some(1);
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    let mut host = CoreHost::begin(
        CoreHostSnapshot {
            core: initial,
            autowork: None,
            tool_wear: None,
            opinions: None,
            goals: None,
            agreements: None,
            sites: None,
            tool_requirements: vec![],
            paused_work_rates: vec![],
            fatigue: None,
            activity: vec![],
            agriculture: None,
            health: None,
            knowledge: None,
            social: None,
            claims: None,
            food_search: None,
            schedule: None,
        },
        &[(1, 1)],
    )
    .unwrap();
    host.enable_schedule(10_000, 1).unwrap();
    let run = host.advance_continuous(1000, &[]).unwrap();
    assert_eq!(run.elapsed_millis, 1000);
    assert!(run.boundary);
    assert!(run.failure.is_some());
    assert_eq!(run.boundaries, 0);
    let before = host.snapshot();
    assert_eq!(
        before
            .core
            .physical
            .work_interval
            .as_ref()
            .unwrap()
            .elapsed_millis,
        1000
    );
    let saved = serde_json::to_string(&before).unwrap();
    let mut host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
    assert!(host.advance_continuous(0, &[]).unwrap().failure.is_some());
    assert_eq!(host.snapshot(), before);
}

#[test]
fn unsupported_completed_work_can_be_deferred_without_blocking_other_jobs() {
    let run =
        |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut initial = fixture();
                    initial.people[1].region = Some(1);
                    initial.travel = None;
                    initial.nutrition = None;
                    initial.pending_food.clear();
                    initial.physical.goods.push(Good {
                        id: 3,
                        name: "perishable".into(),
                        food: Some(game_production_model::FoodProfile {
                            portion: 1,
                            hunger_relief: 1,
                            nutrients: Default::default(),
                            health_effect: 0,
                            shelf_days: Some(1),
                        }),
                    });
                    let mut recipe = initial.physical.recipes[0].clone();
                    recipe.id = 2;
                    recipe.outputs[0].good = 3;
                    initial.physical.recipes.push(recipe);
                    let stock = initial
                        .physical
                        .custody
                        .containers
                        .iter_mut()
                        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
                        .unwrap();
                    *stock = Container::new(
                        stock.place,
                        10,
                        vec![Lot {
                            item: None,
                            good: 1,
                            quantity: 2,
                            expires_at: None,
                        }],
                    )
                    .unwrap();
                    initial.physical.custody.containers.push(
                        Container::new(Place::Stock { region: 1, good: 3 }, 10, vec![]).unwrap(),
                    );
                    initial
                        .physical
                        .custody
                        .containers
                        .sort_by_key(|row| row.place);
                    let mut physical = Runtime::compile(initial.physical).unwrap();
                    assert!(
                        physical
                            .start(
                                &[Start {
                                    id: 2,
                                    worker: 2,
                                    region: 1,
                                    recipe: 2,
                                    units: 1
                                }],
                                7
                            )
                            .unwrap()[0]
                            .1
                    );
                    initial.physical = physical.snapshot();
                    let mut core = CoreInterval::begin(initial, &[(1, 2), (2, 2)]).unwrap();
                    core.progress(400).unwrap();
                    let before = core.snapshot();
                    assert!(core.defer_work(0, 400, &[2]).is_err());
                    assert_eq!(core.snapshot(), before);
                    core.progress(100).unwrap();
                    let due = core.snapshot();
                    assert!(core.publish_early_work().is_err());
                    assert!(core.defer_work(0, 500, &[2, 2]).is_err());
                    assert_eq!(core.snapshot(), due);
                    core.defer_work(0, 500, &[2]).unwrap();
                    let mut core = CoreInterval::resume(
                        serde_json::from_str(&serde_json::to_string(&core.snapshot()).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                    assert_eq!(
                        core.publish_early_work().unwrap().unwrap().finished,
                        vec![1]
                    );
                    assert_eq!(core.next_deadline(None).unwrap().after_millis, 500);
                    assert!(
                        core.start_work(
                            0,
                            500,
                            &[(
                                Start {
                                    id: 3,
                                    worker: 2,
                                    region: 1,
                                    recipe: 1,
                                    units: 1
                                },
                                1
                            )]
                        )
                        .is_err()
                    );
                    core.progress(500).unwrap();
                    let events = core.settle().unwrap();
                    assert_eq!(events.finished, vec![2]);
                    let result = core.snapshot();
                    assert_eq!(
                        result
                            .physical
                            .custody
                            .containers
                            .iter()
                            .find(|row| row.place == Place::JobOutput(2))
                            .unwrap()
                            .used,
                        1
                    );
                    result
                })
        };
    assert_eq!(run(1), run(6));
}

#[test]
fn identified_interval_settlement_stages_item_owner_and_person_effects_together() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut core = CoreInterval::begin(fixture(), &[(1, 1)]).unwrap();
                let owner = game_economy_work::tool_wear::ToolWear {
                    next_item: 10,
                    rules: vec![game_economy_work::tool_wear::Rule {
                        good: 2,
                        durability: 10,
                        wear: 1,
                        repair_good: 1,
                        repair_quantity: 1,
                    }],
                    conditions: vec![],
                    repairs: vec![],
                    events: vec![],
                };
                core.progress(1000).unwrap();
                let before = core.snapshot();
                let goods = [2];
                assert!(
                    core.prepare_settlement(Some(game_lot_runtime::IdentifiedProduction {
                        goods: &goods,
                        next_item: 0
                    }))
                    .is_err()
                );
                assert_eq!(core.snapshot(), before);
                let draft = core
                    .prepare_settlement(Some(game_lot_runtime::IdentifiedProduction {
                        goods: &goods,
                        next_item: 10,
                    }))
                    .unwrap();
                assert_eq!(draft.events().finished, vec![1]);
                assert_eq!(
                    draft.produced_items(),
                    &[game_lot_runtime::ProducedItem {
                        job: 1,
                        item: 10,
                        good: 2
                    }]
                );
                assert_eq!(draft.next_produced_item(), Some(11));
                assert_eq!(core.snapshot(), before);
                // A failed item-owner preparation discards the draft and changes no owner.
                assert!(
                    owner
                        .stage_identified_outputs(draft.produced_items(), 0, draft.item_updates())
                        .is_err()
                );
                assert!(owner.conditions.is_empty());
                drop(draft);
                let saved = serde_json::to_string(&(core.snapshot(), owner)).unwrap();
                let (snapshot, mut owner): (CoreSnapshot, game_economy_work::tool_wear::ToolWear) =
                    serde_json::from_str(&saved).unwrap();
                let mut core = CoreInterval::resume(snapshot).unwrap();
                let stale = core
                    .prepare_settlement(Some(game_lot_runtime::IdentifiedProduction {
                        goods: &goods,
                        next_item: 10,
                    }))
                    .unwrap();
                let draft = core
                    .prepare_settlement(Some(game_lot_runtime::IdentifiedProduction {
                        goods: &goods,
                        next_item: 10,
                    }))
                    .unwrap();
                let next_owner = owner
                    .stage_identified_outputs(
                        draft.produced_items(),
                        draft.next_produced_item().unwrap(),
                        draft.item_updates(),
                    )
                    .unwrap();
                core.validate_settlement(&draft).unwrap();
                core.commit_settlement(draft).unwrap();
                owner = next_owner;
                assert_eq!(
                    owner.conditions,
                    vec![game_economy_work::tool_wear::Condition {
                        item: 10,
                        good: 2,
                        damage: 0
                    }]
                );
                let result = core.snapshot();
                assert_eq!(result.people[0].health, 96);
                let output = result
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|row| row.place == Place::JobOutput(1))
                    .unwrap();
                assert_eq!(output.lots[0].item, Some(10));
                let physical = Runtime::compile(result.physical.clone()).unwrap();
                owner
                    .validate(
                        &physical,
                        &[1, 2].into_iter().collect(),
                        &[1, 2].into_iter().collect(),
                        1,
                    )
                    .unwrap();
                let saved = serde_json::to_string(&(result.clone(), owner.clone())).unwrap();
                let (restored, restored_owner): (
                    CoreSnapshot,
                    game_economy_work::tool_wear::ToolWear,
                ) = serde_json::from_str(&saved).unwrap();
                assert_eq!(restored, result);
                assert_eq!(restored_owner, owner);
                assert!(core.commit_settlement(stale).is_err());
                assert_eq!(core.snapshot(), result);
                (result, owner)
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn identified_early_work_stages_tool_owner_and_survives_later_delivery() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut core = CoreInterval::begin(fixture(), &[(1, 2)]).unwrap();
                core.progress(500).unwrap();
                let owner = game_economy_work::tool_wear::ToolWear {
                    next_item: 10,
                    rules: vec![game_economy_work::tool_wear::Rule {
                        good: 2,
                        durability: 10,
                        wear: 1,
                        repair_good: 1,
                        repair_quantity: 1,
                    }],
                    conditions: vec![],
                    repairs: vec![],
                    events: vec![],
                };
                let before = core.snapshot();
                let draft = core
                    .prepare_early_work(Some(game_lot_runtime::IdentifiedProduction {
                        goods: &[2],
                        next_item: owner.next_item,
                    }))
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    draft.produced_items(),
                    &[game_lot_runtime::ProducedItem {
                        job: 1,
                        item: 10,
                        good: 2
                    }]
                );
                assert!(
                    owner
                        .stage_identified_outputs(draft.produced_items(), 0, draft.item_updates())
                        .is_err()
                );
                assert_eq!(core.snapshot(), before);
                drop(draft);
                let saved = serde_json::to_string(&(core.snapshot(), owner)).unwrap();
                let (snapshot, owner): (CoreSnapshot, game_economy_work::tool_wear::ToolWear) =
                    serde_json::from_str(&saved).unwrap();
                let mut core = CoreInterval::resume(snapshot).unwrap();
                let stale = core
                    .prepare_early_work(Some(game_lot_runtime::IdentifiedProduction {
                        goods: &[2],
                        next_item: owner.next_item,
                    }))
                    .unwrap()
                    .unwrap();
                let draft = core
                    .prepare_early_work(Some(game_lot_runtime::IdentifiedProduction {
                        goods: &[2],
                        next_item: owner.next_item,
                    }))
                    .unwrap()
                    .unwrap();
                let owner = owner
                    .stage_identified_outputs(
                        draft.produced_items(),
                        draft.next_produced_item().unwrap(),
                        draft.item_updates(),
                    )
                    .unwrap();
                core.validate_early_work(&draft).unwrap();
                assert_eq!(core.commit_early_work(draft).unwrap().finished, vec![1]);
                assert!(core.commit_early_work(stale).is_err());
                let early = core.snapshot();
                assert_eq!(
                    early
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::JobOutput(1))
                        .unwrap()
                        .lots[0]
                        .item,
                    Some(10)
                );
                core.progress(500).unwrap();
                assert_eq!(core.settle().unwrap().delivered, vec![1]);
                let result = core.snapshot();
                let physical = Runtime::compile(result.physical.clone()).unwrap();
                assert_eq!(
                    physical.item_place(10),
                    Some(Place::Stock { region: 1, good: 2 })
                );
                owner
                    .validate(
                        &physical,
                        &[1, 2].into_iter().collect(),
                        &[1, 2].into_iter().collect(),
                        1,
                    )
                    .unwrap();
                (result, owner)
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn early_work_consuming_identified_inputs_requires_owner_staging() {
    let mut initial = fixture();
    let input = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::JobInput(1))
        .unwrap();
    *input = Container::new(
        input.place,
        2,
        vec![
            Lot {
                item: Some(10),
                good: 1,
                quantity: 1,
                expires_at: None,
            },
            Lot {
                item: Some(11),
                good: 1,
                quantity: 1,
                expires_at: None,
            },
        ],
    )
    .unwrap();
    let owner = game_economy_work::tool_wear::ToolWear {
        next_item: 12,
        rules: vec![game_economy_work::tool_wear::Rule {
            good: 1,
            durability: 10,
            wear: 1,
            repair_good: 2,
            repair_quantity: 1,
        }],
        conditions: vec![
            game_economy_work::tool_wear::Condition {
                item: 10,
                good: 1,
                damage: 0,
            },
            game_economy_work::tool_wear::Condition {
                item: 11,
                good: 1,
                damage: 0,
            },
        ],
        repairs: vec![],
        events: vec![],
    };
    let mut boundary = CoreInterval::begin(initial.clone(), &[(1, 1)]).unwrap();
    boundary.progress(1000).unwrap();
    let before_boundary = boundary.snapshot();
    assert!(boundary.settle().is_err());
    assert_eq!(boundary.snapshot(), before_boundary);
    let draft = boundary.prepare_settlement(None).unwrap();
    let boundary_owner = owner
        .stage_identified_outputs(&[], owner.next_item, draft.item_updates())
        .unwrap();
    boundary.commit_settlement(draft).unwrap();
    assert!(boundary_owner.conditions.is_empty());
    let mut core = CoreInterval::begin(initial, &[(1, 2)]).unwrap();
    core.progress(500).unwrap();
    let before = core.snapshot();
    assert!(core.publish_early_work().is_err());
    assert_eq!(core.snapshot(), before);
    let draft = core.prepare_early_work(None).unwrap().unwrap();
    assert_eq!(draft.item_updates(), &[(10, None), (11, None)]);
    let owner = owner
        .stage_identified_outputs(&[], owner.next_item, draft.item_updates())
        .unwrap();
    core.commit_early_work(draft).unwrap();
    assert!(owner.conditions.is_empty());
    let physical = Runtime::compile(core.snapshot().physical).unwrap();
    assert_eq!(physical.item_place(10), None);
    assert_eq!(physical.item_place(11), None);
}

#[test]
fn composed_core_interval_is_chunk_independent_and_resumable() {
    let initial = fixture();
    let run = |workers, chunks: &[u64], reload| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut core = CoreInterval::begin(initial.clone(), &[(1, 2)]).unwrap();
                let opening = core.snapshot();
                assert_eq!(core.progress(0).unwrap(), 0);
                assert!(core.settle().is_err());
                assert_eq!(core.snapshot(), opening);
                for (index, &chunk) in chunks.iter().enumerate() {
                    assert_eq!(core.progress(chunk).unwrap(), chunk);
                    if index == 0 && reload {
                        let saved = serde_json::to_string(&core.snapshot()).unwrap();
                        core = CoreInterval::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                    }
                }
                let pending = core.snapshot();
                assert_eq!(pending.people[1].region, None);
                assert!(pending.travel.as_ref().unwrap().journeys[0].pending_arrival);
                assert_eq!(pending.physical.work[0].job.remaining, 0);
                assert_eq!(core.progress(1).unwrap(), 0);
                let events = core.settle().unwrap();
                assert!(core.settle().is_err());
                (core.snapshot(), events)
            })
    };
    let (whole, whole_events) = run(1, &[1000], false);
    let (split, split_events) = run(4, &[333, 167, 500], true);
    assert_eq!(whole, split);
    assert_eq!(whole_events, split_events);
    assert_eq!(whole.people[0].health, 96);
    assert_eq!(whole.people[1].region, Some(2));
    assert!(whole.travel.as_ref().unwrap().journeys.is_empty());
    let mut physical = Runtime::compile(initial.physical.clone()).unwrap();
    let expected_events = physical.advance(&[(1, 2)], initial.seed).unwrap();
    let expected_person = game_economy_person::prepare(
        &initial.people,
        &initial.rules,
        initial.nutrition.as_ref(),
        &initial.pending_food,
        Effects {
            hunger_relief: vec![],
            pending: vec![],
        },
        1,
    );
    let mut people = expected_person.people;
    let expected_travel = initial.travel.as_ref().unwrap().advance(&mut people);
    assert_eq!(whole.physical, physical.snapshot());
    assert_eq!(whole.people, people);
    assert_eq!(whole.nutrition, expected_person.nutrition);
    assert_eq!(whole.pending_food, expected_person.pending);
    assert_eq!(whole.travel, Some(expected_travel));
    assert_eq!(whole_events, expected_events);
}

#[test]
fn early_arrival_is_resumable_and_does_not_run_periodic_effects() {
    let run = |workers, chunks: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.travel.as_mut().unwrap().journeys[0].progress_millis = 500;
                let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
                assert!(core.publish_arrivals().is_err());
                for &chunk in chunks {
                    assert_eq!(core.progress_to_deadline(chunk, None).unwrap(), chunk);
                }
                assert!(core.next_deadline(None).unwrap().arrival);
                let before = core.snapshot();
                assert_eq!(before.people[1].region, None);
                let mut core = CoreInterval::resume(
                    serde_json::from_str(&serde_json::to_string(&before).unwrap()).unwrap(),
                )
                .unwrap();
                let arrived = core.publish_arrivals().unwrap().unwrap();
                assert_eq!((arrived.tick, arrived.offset_millis), (0, 500));
                assert_eq!(arrived.people, vec![2]);
                let after = core.snapshot();
                assert_eq!(after.people[1].region, Some(2));
                assert_eq!(after.people[1].health, 100);
                assert_eq!(after.physical.custody.tick, 0);
                assert_eq!(after.pending_food.len(), 1);
                assert!(after.travel.as_ref().unwrap().journeys.is_empty());
                let mut core = CoreInterval::resume(
                    serde_json::from_str(&serde_json::to_string(&after).unwrap()).unwrap(),
                )
                .unwrap();
                assert_eq!(core.publish_arrivals().unwrap(), None);
                assert!(!core.next_deadline(None).unwrap().arrival);
                assert_eq!(core.progress_to_deadline(500, None).unwrap(), 500);
                assert!(core.publish_arrivals().is_err());
                core.settle().unwrap();
                core.snapshot()
            })
    };
    let one = run(1, &[500]);
    let multiple = run(4, &[125, 375]);
    assert_eq!(one, multiple);
    assert_eq!(one.people[1].region, Some(2));
    assert_eq!(one.physical.custody.tick, 1);
}

#[test]
fn arrival_can_start_work_that_completes_before_the_boundary() {
    let run = |workers, chunks: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.travel.as_mut().unwrap().journeys[0].progress_millis = 600;
                let input = initial
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|row| row.place == Place::Stock { region: 2, good: 1 })
                    .unwrap();
                *input = Container::new(
                    input.place,
                    10,
                    vec![Lot {
                        item: None,
                        good: 1,
                        quantity: 2,
                        expires_at: None,
                    }],
                )
                .unwrap();
                let start = Start {
                    id: 2,
                    worker: 2,
                    region: 2,
                    recipe: 1,
                    units: 1,
                };
                let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
                assert_eq!(core.progress_to_deadline(400, None).unwrap(), 400);
                let before = core.snapshot();
                assert!(core.start_work(0, 400, &[(start, 2)]).is_err());
                assert_eq!(core.snapshot(), before);
                assert_eq!(core.publish_arrivals().unwrap().unwrap().people, vec![2]);
                let saved = serde_json::to_string(&core.snapshot()).unwrap();
                let mut core = CoreInterval::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                let before = core.snapshot();
                assert!(core.start_work(0, 399, &[(start, 2)]).is_err());
                assert!(core.start_work(0, 400, &[(start, 0)]).is_err());
                assert_eq!(core.snapshot(), before);
                assert_eq!(
                    core.start_work(0, 400, &[(start, 2)]).unwrap(),
                    vec![(2, true)]
                );
                let after = core.snapshot();
                assert_eq!(after.physical.custody.tick, 0);
                assert_eq!(
                    after
                        .physical
                        .work_interval
                        .as_ref()
                        .unwrap()
                        .elapsed_millis,
                    400
                );
                assert_eq!(
                    after.physical.work_interval.as_ref().unwrap().rates,
                    vec![(1, 1), (2, 2)]
                );
                assert!(core.start_work(0, 400, &[(start, 2)]).is_err());
                let mut core = CoreInterval::resume(
                    serde_json::from_str(&serde_json::to_string(&after).unwrap()).unwrap(),
                )
                .unwrap();
                for &chunk in chunks {
                    assert_eq!(core.progress_to_deadline(chunk, None).unwrap(), chunk);
                }
                assert_eq!(core.elapsed_millis(), Some(900));
                assert_eq!(
                    core.publish_early_work().unwrap().unwrap().finished,
                    vec![2]
                );
                let intermediate = core.snapshot();
                assert_eq!(intermediate.people[1].health, 100);
                assert!(intermediate.physical.custody.containers.iter().any(|row| {
                    row.place == Place::JobOutput(2)
                        && row
                            .lots
                            .iter()
                            .any(|lot| lot.good == 2 && lot.quantity == 1)
                }));
                assert_eq!(core.progress_to_deadline(100, None).unwrap(), 100);
                let events = core.settle().unwrap();
                (core.snapshot(), events)
            })
    };
    assert_eq!(run(1, &[500]), run(4, &[100, 150, 250]));
}

#[test]
fn rejected_interval_work_reservation_is_inert() {
    let mut initial = fixture();
    initial.travel.as_mut().unwrap().journeys[0].progress_millis = 600;
    let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
    core.progress_to_deadline(400, None).unwrap();
    core.publish_arrivals().unwrap();
    let before = core.snapshot();
    let start = Start {
        id: 2,
        worker: 2,
        region: 2,
        recipe: 1,
        units: 1,
    };
    let remote = Start { region: 1, ..start };
    assert!(core.start_work(0, 400, &[(remote, 2)]).is_err());
    let busy = Start {
        worker: 1,
        region: 1,
        ..start
    };
    assert!(core.start_work(0, 400, &[(busy, 2)]).is_err());
    assert_eq!(
        core.start_work(0, 400, &[(start, 2)]).unwrap(),
        vec![(2, false)]
    );
    assert_eq!(core.snapshot(), before);
}

#[test]
fn interval_work_rejects_incapacitated_or_overhungry_people() {
    for (health, hunger) in [(0, 0), (100, 11)] {
        let mut initial = fixture();
        let mut person = initial.people[0].clone();
        person.id = 3;
        person.health = health;
        person.hunger = hunger;
        initial.people.push(person);
        initial.nutrition = None;
        let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
        core.progress(400).unwrap();
        let before = core.snapshot();
        let start = Start {
            id: 2,
            worker: 3,
            region: 1,
            recipe: 1,
            units: 1,
        };
        assert!(core.start_work(0, 400, &[(start, 1)]).is_err());
        assert_eq!(core.snapshot(), before);
    }
}

#[test]
fn mixed_interval_admission_preserves_rejected_stock_and_resumes() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.travel.as_mut().unwrap().journeys[0].progress_millis = 600;
                let mut third = initial.people[0].clone();
                third.id = 3;
                third.region = Some(2);
                initial.people.push(third);
                initial.nutrition = None;
                let input = initial
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|row| row.place == Place::Stock { region: 2, good: 1 })
                    .unwrap();
                *input = Container::new(
                    input.place,
                    10,
                    vec![Lot {
                        item: None,
                        good: 1,
                        quantity: 2,
                        expires_at: None,
                    }],
                )
                .unwrap();
                let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
                core.progress_to_deadline(400, None).unwrap();
                core.publish_arrivals().unwrap();
                let requests = [2, 3].map(|id| {
                    (
                        Start {
                            id,
                            worker: id,
                            region: 2,
                            recipe: 1,
                            units: 1,
                        },
                        2,
                    )
                });
                let results = core.start_work(0, 400, &requests).unwrap();
                assert_eq!(results.iter().filter(|row| row.1).count(), 1);
                let saved = core.snapshot();
                assert_eq!(saved.physical.work.len(), 2);
                assert_eq!(
                    saved.physical.work_interval.as_ref().unwrap().rates.len(),
                    2
                );
                let mut core = CoreInterval::resume(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                core.progress_to_deadline(500, None).unwrap();
                core.publish_early_work().unwrap();
                core.progress_to_deadline(100, None).unwrap();
                core.settle().unwrap();
                (results, core.snapshot())
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn a_worker_can_start_again_after_early_completion() {
    let mut initial = fixture();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        10,
        vec![Lot {
            item: None,
            good: 1,
            quantity: 2,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut core = CoreInterval::begin(initial, &[(1, 2)]).unwrap();
    core.progress_to_deadline(500, None).unwrap();
    let second = Start {
        id: 2,
        worker: 1,
        region: 1,
        recipe: 1,
        units: 1,
    };
    assert!(core.start_work(0, 500, &[(second, 2)]).is_err());
    core.publish_early_work().unwrap();
    assert_eq!(
        core.start_work(0, 500, &[(second, 2)]).unwrap(),
        vec![(2, true)]
    );
    core.progress_to_deadline(500, None).unwrap();
    let events = core.settle().unwrap();
    assert_eq!(events.finished, vec![2]);
}

#[test]
#[ignore = "manual interval work admission scaling benchmark"]
fn interval_work_admission_benchmark() {
    use std::hash::{Hash, Hasher};
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(50_000);
    assert!(count >= 2);
    let stock_units = std::env::var("LEGACY_ADMISSION_STOCK")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(count - 1)
        .min(count - 1);
    let defer = std::env::var_os("LEGACY_DEFER").is_some();
    let identified = std::env::var_os("LEGACY_IDENTIFIED_SETTLE").is_some();
    let identified_early = std::env::var_os("LEGACY_IDENTIFIED_EARLY").is_some();
    let automatic = std::env::var_os("LEGACY_AUTOWORK").is_some();
    assert!(
        [
            defer,
            identified,
            identified_early,
            automatic,
            std::env::var_os("LEGACY_ABANDON").is_some()
        ]
        .iter()
        .filter(|&&value| value)
        .count()
            <= 1
    );
    assert!(!identified_early || stock_units > 0);
    let mut initial = fixture();
    let person = initial.people[0].clone();
    initial.people = (1..=count)
        .map(|id| {
            let mut p = person.clone();
            p.id = id;
            if automatic {
                p.willing_to_work = true;
            }
            p
        })
        .collect();
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        count * 2,
        if stock_units == 0 {
            vec![]
        } else {
            vec![Lot {
                item: None,
                good: 1,
                quantity: stock_units * 2,
                expires_at: None,
            }]
        },
    )
    .unwrap();
    let starts: Vec<_> = (2..=count)
        .map(|id| {
            (
                Start {
                    id,
                    worker: id,
                    region: 1,
                    recipe: 1,
                    units: 1,
                },
                if defer || identified || identified_early {
                    2
                } else {
                    1
                },
            )
        })
        .collect();
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            let mut abandonment_samples = Vec::new();
            let mut deferral_samples = Vec::new();
            let mut settlement_samples = Vec::new();
            let mut early_samples = Vec::new();
            for round in 0..4 {
                let mut settled_owner = None;
                let mut auto = automatic.then(|| game_autowork::Autowork {
                    next_id: 2,
                    policies: (2..=count)
                        .map(|person| game_autowork::Policy {
                            person,
                            recipe: 1,
                            enabled: true,
                            disregard_claims: false,
                        })
                        .collect(),
                    requests: None,
                });
                let mut core = CoreInterval::begin(initial.clone(), &[(1, 1)]).unwrap();
                assert_eq!(core.progress(400).unwrap(), 400);
                let start = std::time::Instant::now();
                let result = if let Some(owner) = &mut auto {
                    core.start_autonomous_work(0, 400, owner, &[], 1).unwrap()
                } else {
                    core.start_work(0, 400, &starts).unwrap()
                };
                let elapsed = start.elapsed().as_secs_f64();
                assert_eq!(
                    result.len(),
                    if automatic {
                        stock_units as usize
                    } else {
                        starts.len()
                    }
                );
                assert_eq!(
                    result.iter().filter(|row| row.1).count() as u64,
                    stock_units
                );
                if std::env::var_os("LEGACY_ABANDON").is_some() {
                    let requests: Vec<_> = result
                        .iter()
                        .filter(|row| row.1)
                        .map(|row| (row.0, row.0))
                        .collect();
                    let start = std::time::Instant::now();
                    core.abandon_work(0, 400, &requests).unwrap();
                    if round > 0 {
                        abandonment_samples.push(start.elapsed().as_secs_f64());
                    }
                }
                if defer {
                    core.progress(500).unwrap();
                    let jobs: Vec<_> = result.iter().filter(|row| row.1).map(|row| row.0).collect();
                    let start = std::time::Instant::now();
                    core.defer_work(0, 900, &jobs).unwrap();
                    if round > 0 {
                        deferral_samples.push(start.elapsed().as_secs_f64());
                    }
                }
                if identified {
                    core.progress(600).unwrap();
                    let owner = game_economy_work::tool_wear::ToolWear {
                        next_item: 1_000_000,
                        rules: vec![game_economy_work::tool_wear::Rule {
                            good: 2,
                            durability: 10,
                            wear: 1,
                            repair_good: 1,
                            repair_quantity: 1,
                        }],
                        conditions: vec![],
                        repairs: vec![],
                        events: vec![],
                    };
                    let start = std::time::Instant::now();
                    let draft = core
                        .prepare_settlement(Some(game_lot_runtime::IdentifiedProduction {
                            goods: &[2],
                            next_item: 1_000_000,
                        }))
                        .unwrap();
                    let next_owner = owner
                        .stage_identified_outputs(
                            draft.produced_items(),
                            draft.next_produced_item().unwrap(),
                            draft.item_updates(),
                        )
                        .unwrap();
                    assert_eq!(next_owner.conditions.len(), stock_units as usize + 1);
                    core.commit_settlement(draft).unwrap();
                    settled_owner = Some(next_owner);
                    if round > 0 {
                        settlement_samples.push(start.elapsed().as_secs_f64());
                    }
                }
                if identified_early {
                    core.progress(500).unwrap();
                    let owner = game_economy_work::tool_wear::ToolWear {
                        next_item: 1_000_000,
                        rules: vec![game_economy_work::tool_wear::Rule {
                            good: 2,
                            durability: 10,
                            wear: 1,
                            repair_good: 1,
                            repair_quantity: 1,
                        }],
                        conditions: vec![],
                        repairs: vec![],
                        events: vec![],
                    };
                    let start = std::time::Instant::now();
                    let draft = core
                        .prepare_early_work(Some(game_lot_runtime::IdentifiedProduction {
                            goods: &[2],
                            next_item: 1_000_000,
                        }))
                        .unwrap()
                        .unwrap();
                    let next_owner = owner
                        .stage_identified_outputs(
                            draft.produced_items(),
                            draft.next_produced_item().unwrap(),
                            draft.item_updates(),
                        )
                        .unwrap();
                    assert_eq!(next_owner.conditions.len(), stock_units as usize);
                    core.commit_early_work(draft).unwrap();
                    settled_owner = Some(next_owner);
                    if round > 0 {
                        early_samples.push(start.elapsed().as_secs_f64());
                    }
                }
                if round == 3 {
                    let mut hash = std::collections::hash_map::DefaultHasher::new();
                    serde_json::to_vec(&core.snapshot())
                        .unwrap()
                        .hash(&mut hash);
                    if let Some(owner) = settled_owner {
                        serde_json::to_vec(&owner).unwrap().hash(&mut hash);
                    }
                    if let Some(owner) = auto {
                        serde_json::to_vec(&owner).unwrap().hash(&mut hash);
                    }
                    println!("fingerprint={:016x}", hash.finish());
                }
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} starts={} stock_units={stock_units} elapsed={:.6}",
                starts.len(),
                samples[1]
            );
            if automatic {
                println!(
                    "workers={workers} autonomous_starts={stock_units} elapsed={:.6}",
                    samples[1]
                );
            }
            if !abandonment_samples.is_empty() {
                abandonment_samples.sort_by(f64::total_cmp);
                println!(
                    "workers={workers} abandoned={stock_units} elapsed={:.6}",
                    abandonment_samples[1]
                );
            }
            if defer {
                deferral_samples.sort_by(f64::total_cmp);
                println!(
                    "workers={workers} deferred={stock_units} elapsed={:.6}",
                    deferral_samples[1]
                );
            }
            if identified {
                settlement_samples.sort_by(f64::total_cmp);
                println!(
                    "workers={workers} identified={} elapsed={:.6}",
                    stock_units + 1,
                    settlement_samples[1]
                );
            }
            if identified_early {
                early_samples.sort_by(f64::total_cmp);
                println!(
                    "workers={workers} early_identified={stock_units} elapsed={:.6}",
                    early_samples[1]
                );
            }
            #[cfg(feature = "profile")]
            if std::env::var_os("LEGACY_PROFILE").is_some() {
                for (name, sample) in game_diagnostics::snapshot() {
                    if name.starts_with("physical.") || name.starts_with("work.start.") {
                        println!(
                            "{name}: {:.3} ms/call",
                            sample.nanoseconds as f64 / sample.calls as f64 / 1_000_000.0
                        );
                    }
                }
            }
        });
}

#[test]
fn core_publishes_early_work_without_advancing_periodic_systems() {
    let run = |workers, chunks: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut core = CoreInterval::begin(fixture(), &[(1, 2)]).unwrap();
                for &chunk in chunks {
                    assert_eq!(core.progress_to_deadline(chunk, Some(3)).unwrap(), chunk);
                }
                assert_eq!(core.progress_to_deadline(1, Some(3)).unwrap(), 0);
                let before = core.snapshot();
                assert_eq!(before.physical.custody.tick, 0);
                assert_eq!(before.people[0].health, 100);
                let mut core = CoreInterval::resume(
                    serde_json::from_str(&serde_json::to_string(&before).unwrap()).unwrap(),
                )
                .unwrap();
                let early = core.publish_early_work().unwrap().unwrap();
                assert_eq!((early.tick, early.offset_millis), (0, 500));
                assert_eq!(early.finished, vec![1]);
                let after = core.snapshot();
                assert_eq!(after.physical.custody.tick, 0);
                assert_eq!(after.people[0].health, 100);
                assert_eq!(
                    after
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::JobOutput(1))
                        .unwrap()
                        .used,
                    1
                );
                let mut core = CoreInterval::resume(
                    serde_json::from_str(&serde_json::to_string(&after).unwrap()).unwrap(),
                )
                .unwrap();
                assert!(core.publish_early_work().unwrap().is_none());
                let next = core.next_deadline(Some(3)).unwrap();
                assert_eq!(next.after_millis, 500);
                assert!(next.quantum && next.arrival && next.hunger);
                assert_eq!(core.progress_to_deadline(1000, Some(3)).unwrap(), 500);
                let boundary = core.settle().unwrap();
                let state = core.snapshot();
                assert_eq!(state.physical.custody.tick, 1);
                assert_eq!(state.people[0].health, 96);
                assert_eq!(state.people[1].region, Some(2));
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
                assert!(boundary.finished.is_empty());
                (state, early, boundary)
            })
    };
    assert_eq!(run(1, &[500]), run(4, &[333, 167]));
}

#[test]
fn core_deadline_reports_earliest_and_tied_causes_without_publishing() {
    let mut core = CoreInterval::begin(fixture(), &[(1, 2)]).unwrap();
    let opening = core.snapshot();
    let deadline = core.next_deadline(Some(3)).unwrap();
    assert_eq!(deadline.after_millis, 500);
    assert!(deadline.work);
    assert!(!deadline.arrival && !deadline.hunger && !deadline.quantum);
    assert_eq!(core.snapshot(), opening);
    core.progress(333).unwrap();
    assert_eq!(core.next_deadline(Some(3)).unwrap().after_millis, 167);
    core.progress(167).unwrap();
    let pending = core.next_deadline(Some(3)).unwrap();
    assert_eq!(pending.after_millis, 0);
    assert!(pending.work);
    assert!(core.settle().is_err());

    let mut tied = CoreInterval::begin(fixture(), &[]).unwrap();
    let deadline = tied.next_deadline(Some(3)).unwrap();
    assert_eq!(deadline.after_millis, 1000);
    assert!(deadline.quantum && deadline.arrival && deadline.hunger);
    assert!(!deadline.work);
    tied.progress(1000).unwrap();
    let deadline = tied.next_deadline(Some(3)).unwrap();
    assert_eq!(deadline.after_millis, 0);
    assert!(deadline.quantum && deadline.arrival && deadline.hunger);
}

#[test]
fn composed_interval_rejects_invalid_reload_without_mutation() {
    let mut dead_worker = fixture();
    dead_worker.people[0].health = 0;
    assert!(CoreInterval::begin(dead_worker.clone(), &[(1, 2)]).is_err());
    assert!(CoreInterval::begin(dead_worker, &[]).is_ok());
    let mut core = CoreInterval::begin(fixture(), &[(1, 2)]).unwrap();
    assert_eq!(core.progress(250).unwrap(), 250);
    let before = core.snapshot();
    let mut invalid = before.clone();
    invalid.people[1].region = Some(1);
    assert!(CoreInterval::resume(invalid).is_err());
    let mut invalid = before.clone();
    invalid.people[0].health = 0;
    assert!(CoreInterval::resume(invalid).is_err());
    assert_eq!(core.snapshot(), before);
    assert_eq!(core.progress(u64::MAX).unwrap(), 750);
}

#[test]
fn interval_rejects_unordered_journeys_and_preserves_failed_settlement() {
    let mut unordered = fixture();
    let mut second = unordered.people[1].clone();
    second.id = 3;
    unordered.people.push(second);
    let travel = unordered.travel.as_mut().unwrap();
    let mut journey = travel.journeys[0].clone();
    journey.person = 3;
    travel.journeys.push(journey);
    assert!(CoreInterval::begin(unordered.clone(), &[(1, 2)]).is_ok());
    unordered.travel.as_mut().unwrap().journeys.reverse();
    assert!(CoreInterval::begin(unordered, &[(1, 2)]).is_err());

    let mut overflow = fixture();
    overflow.physical.custody.tick = u64::MAX;
    overflow.pending_food.clear();
    let mut core = CoreInterval::begin(overflow, &[(1, 2)]).unwrap();
    core.progress(1000).unwrap();
    let before = core.snapshot();
    assert!(core.settle().is_err());
    assert_eq!(core.snapshot(), before);
    let saved = serde_json::to_string(&before).unwrap();
    assert_eq!(
        CoreInterval::resume(serde_json::from_str(&saved).unwrap())
            .unwrap()
            .snapshot(),
        before
    );
}

#[test]
fn incapacitated_immortal_does_not_travel_before_boundary_recovery() {
    let mut initial = fixture();
    initial.people[1].health = 0;
    initial.people[1].hunger = 0;
    initial.people[1].immortal = Some(game_economy_person::Immortal {
        recovery_per_tick: 5,
    });
    initial.pending_food.clear();
    let mut core = CoreInterval::begin(initial, &[(1, 2)]).unwrap();
    core.progress(1000).unwrap();
    let pending = core.snapshot();
    assert_eq!(pending.travel.as_ref().unwrap().journeys[0].remaining, 1);
    assert_eq!(
        pending.travel.as_ref().unwrap().journeys[0].progress_millis,
        0
    );
    core.settle().unwrap();
    let settled = core.snapshot();
    assert_eq!(settled.people[1].health, 5);
    assert_eq!(settled.people[1].region, None);
    assert_eq!(settled.travel.unwrap().journeys[0].remaining, 1);
}

#[test]
#[ignore = "manual composed interval scaling benchmark"]
fn composed_core_progress_benchmark() {
    use std::hash::{Hash, Hasher};
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(100_000u64);
    let deadline = std::env::var_os("LEGACY_CORE_DEADLINE").is_some();
    let arrivals = std::env::var_os("LEGACY_CORE_ARRIVALS").is_some();
    let many_jobs = std::env::var_os("LEGACY_CORE_JOBS").is_some();
    assert!(count >= 2 && count.is_multiple_of(2));
    let mut initial = fixture();
    let resident = initial.people[0].clone();
    let traveler = initial.people[1].clone();
    initial.people = (1..=count)
        .map(|id| {
            let mut p = if id % 2 == 0 {
                traveler.clone()
            } else {
                resident.clone()
            };
            p.id = id;
            p
        })
        .collect();
    initial.travel.as_mut().unwrap().journeys = (1..=count)
        .filter(|id| id % 2 == 0)
        .map(|person| Journey {
            person,
            source: 1,
            destination: 2,
            remaining: 1,
            progress_millis: if arrivals { 500 } else { 0 },
            pending_arrival: false,
        })
        .collect();
    let rates: Vec<_> = if many_jobs {
        let stock = initial
            .physical
            .custody
            .containers
            .iter_mut()
            .find(|row| row.place == Place::Stock { region: 1, good: 1 })
            .unwrap();
        *stock = Container::new(
            stock.place,
            count,
            if count == 2 {
                vec![]
            } else {
                vec![Lot {
                    item: None,
                    good: 1,
                    quantity: count - 2,
                    expires_at: None,
                }]
            },
        )
        .unwrap();
        let mut runtime = Runtime::compile(initial.physical.clone()).unwrap();
        let starts: Vec<_> = (3..=count)
            .step_by(2)
            .map(|id| Start {
                id,
                worker: id,
                region: 1,
                recipe: 1,
                units: 1,
            })
            .collect();
        assert!(
            runtime
                .start(&starts, initial.seed)
                .unwrap()
                .iter()
                .all(|row| row.1)
        );
        initial.physical = runtime.snapshot();
        (1..=count).step_by(2).map(|id| (id, 2)).collect()
    } else {
        vec![(1, 2)]
    };
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut interval = CoreInterval::begin(initial.clone(), &rates).unwrap();
                if arrivals {
                    assert_eq!(interval.progress(500).unwrap(), 500);
                }
                let started = std::time::Instant::now();
                if arrivals {
                    assert_eq!(interval.publish_arrivals().unwrap().unwrap().people.len(), (count / 2) as usize);
                } else if deadline {
                    assert_eq!(interval.next_deadline(Some(3)).unwrap().after_millis, 500);
                } else {
                    assert_eq!(interval.progress(500).unwrap(), 500);
                }
                let elapsed = started.elapsed().as_secs_f64();
                if round == 3 {
                    let state = interval.snapshot();
                    assert!(
                        state
                            .people
                            .iter()
                            .all(|person| person.hunger_remainder == if deadline { 0 } else { 500 })
                    );
                    assert!(
                        state
                            .travel
                            .as_ref()
                            .unwrap()
                            .journeys
                            .iter()
                            .all(
                                |journey| journey.progress_millis == if deadline { 0 } else { 500 }
                            )
                    );
                    let mut hash = std::collections::hash_map::DefaultHasher::new();
                    serde_json::to_vec(&state).unwrap().hash(&mut hash);
                    println!("fingerprint={:016x}", hash.finish());
                }
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} travelers={} jobs={} deadline={deadline} arrivals={arrivals} elapsed={:.6}",
                count / 2,
                rates.len(),
                samples[1]
            );
        });
}
