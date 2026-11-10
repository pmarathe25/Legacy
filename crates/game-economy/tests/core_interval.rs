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
    let mut initial = fixture();
    let person = initial.people[0].clone();
    initial.people = (1..=count)
        .map(|id| {
            let mut p = person.clone();
            p.id = id;
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
                let mut core = CoreInterval::begin(initial.clone(), &[(1, 1)]).unwrap();
                assert_eq!(core.progress(400).unwrap(), 400);
                let start = std::time::Instant::now();
                let result = core.start_work(0, 400, &starts).unwrap();
                let elapsed = start.elapsed().as_secs_f64();
                assert_eq!(result.len(), starts.len());
                assert_eq!(
                    result.iter().filter(|row| row.1).count() as u64,
                    stock_units
                );
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
                "workers={workers} starts={} stock_units={stock_units} elapsed={:.6}",
                starts.len(),
                samples[1]
            );
            #[cfg(feature = "profile")]
            if std::env::var_os("LEGACY_PROFILE").is_some() {
                for (name, sample) in game_diagnostics::snapshot() {
                    if name.starts_with("physical.") {
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
