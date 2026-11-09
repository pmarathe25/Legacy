use game_economy::core_interval::{CoreInterval, CoreSnapshot};
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
            progress_millis: 0,
            pending_arrival: false,
        })
        .collect();
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut interval = CoreInterval::begin(initial.clone(), &[(1, 2)]).unwrap();
                let started = std::time::Instant::now();
                assert_eq!(interval.progress(500).unwrap(), 500);
                let elapsed = started.elapsed().as_secs_f64();
                if round == 3 {
                    let state = interval.snapshot();
                    assert!(
                        state
                            .people
                            .iter()
                            .all(|person| person.hunger_remainder == 500)
                    );
                    assert!(
                        state
                            .travel
                            .as_ref()
                            .unwrap()
                            .journeys
                            .iter()
                            .all(|journey| journey.progress_millis == 500)
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
                "workers={workers} people={count} travelers={} elapsed={:.6}",
                count / 2,
                samples[1]
            );
        });
}
