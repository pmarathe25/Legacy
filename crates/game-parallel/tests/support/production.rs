use game_parallel::{CropPhase, JobPhase, Move, ProductionEventKind, StartWork, World};

fn world() -> World {
    World::from_toml(include_str!("../../../../content/parallel/workshop.toml")).unwrap()
}
fn work(id: u64, worker: u64) -> StartWork {
    StartWork {
        id,
        worker,
        region: 1,
        recipe: 1,
        units: 1,
    }
}
fn stock(w: &World, good: u32) -> u64 {
    w.production.as_ref().unwrap().inventories[0]
        .stocks
        .iter()
        .find(|s| s.good == good)
        .unwrap()
        .quantity
}

#[test]
fn reservations_are_atomic_and_work_survives_reload() {
    let mut w = world();
    let before = w.clone();
    assert!(w.start_work(&[work(1, 1), work(2, 1)]).is_err());
    assert_eq!(w, before);
    let mut too_many = work(2, 2);
    too_many.units = 3;
    assert!(w.start_work(&[work(1, 1), too_many]).is_err());
    assert_eq!(w, before);
    w.start_work(&[work(2, 2), work(1, 1)]).unwrap();
    assert_eq!(stock(&w, 1), 3);
    let reserved = w
        .production
        .as_ref()
        .unwrap()
        .jobs
        .iter()
        .map(|j| j.reserved[0].quantity)
        .sum::<u64>();
    assert_eq!(stock(&w, 1) + reserved, 9);
    w.step().unwrap();
    w = World::from_json(&serde_json::to_string(&w).unwrap()).unwrap();
    w.step().unwrap();
    assert_eq!(stock(&w, 2), 4);
    assert!(w.production.as_ref().unwrap().jobs.is_empty());
    assert_eq!(
        w.production
            .as_ref()
            .unwrap()
            .events
            .iter()
            .filter(|e| e.kind == ProductionEventKind::Delivered)
            .count(),
        2
    );
    w.validate().unwrap();
}
#[test]
fn finished_outputs_wait_for_capacity_and_are_delivered_once() {
    let mut w = world();
    w.production.as_mut().unwrap().inventories[0].stocks[1].quantity = 3;
    w.start_work(&[work(1, 1)]).unwrap();
    w.step().unwrap();
    w.step().unwrap();
    let job = &w.production.as_ref().unwrap().jobs[0];
    assert_eq!(job.phase, JobPhase::Waiting);
    assert!(job.reserved.is_empty());
    assert_eq!(job.outputs[0].quantity, 2);
    assert_eq!(stock(&w, 2), 3);
    w.move_people(&[Move {
        person: 1,
        destination: 2,
    }])
    .unwrap();
    w.production.as_mut().unwrap().inventories[0].stocks[1].quantity = 2;
    w.step().unwrap();
    assert_eq!(stock(&w, 2), 4);
    assert!(w.production.as_ref().unwrap().jobs.is_empty());
    w.step().unwrap();
    assert_eq!(stock(&w, 2), 4);
}
#[test]
fn sick_or_dead_workers_leave_inputs_held_and_a_replacement_can_resume() {
    let mut w = world();
    w.start_work(&[work(1, 1)]).unwrap();
    let before = w.clone();
    assert!(
        w.move_people(&[Move {
            person: 1,
            destination: 2
        }])
        .is_err()
    );
    assert_eq!(w, before);
    w.regions[0].people[0].health = 0;
    w.step().unwrap();
    assert_eq!(w.production.as_ref().unwrap().jobs[0].remaining, 2);
    assert_eq!(
        w.production.as_ref().unwrap().jobs[0].reserved[0].quantity,
        3
    );
    w.reassign_work(1, 2).unwrap();
    w.step().unwrap();
    w.step().unwrap();
    assert_eq!(stock(&w, 2), 2);
}
#[test]
fn cancellation_returns_inputs_or_fails_without_changes() {
    let mut w = world();
    w.start_work(&[work(1, 1)]).unwrap();
    w.production.as_mut().unwrap().inventories[0].stocks[0].quantity = 19;
    let before = w.clone();
    assert!(w.cancel_work(1).is_err());
    assert_eq!(w, before);
    w.production.as_mut().unwrap().inventories[0].stocks[0].quantity = 6;
    w.cancel_work(1).unwrap();
    assert_eq!(stock(&w, 1), 9);
    assert!(w.production.as_ref().unwrap().jobs.is_empty());
}
#[test]
fn working_person_does_not_also_farm_or_socialize() {
    let mut w = world();
    w.agriculture = World::from_toml(include_str!("../../../../content/parallel/farming.toml"))
        .unwrap()
        .agriculture;
    w.social = World::from_toml(include_str!("../../../../content/parallel/social.toml"))
        .unwrap()
        .social;
    w.regions[0].people[1].health = 0;
    w.start_work(&[work(1, 1)]).unwrap();
    w.step().unwrap();
    assert!(
        w.agriculture.as_ref().unwrap().farms[0]
            .fields
            .iter()
            .all(|f| f.phase == CropPhase::Fallow)
    );
    assert!(w.social.as_ref().unwrap().events.is_empty());
    assert_eq!(w.production.as_ref().unwrap().jobs[0].remaining, 1);
}
#[test]
fn failed_family_tick_does_not_publish_work() {
    let mut w =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    w.production = world().production;
    w.start_work(&[work(1, 1)]).unwrap();
    w.families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|m| m.id == 2)
        .unwrap()
        .age = u64::MAX;
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
}

#[test]
fn contested_multi_output_grants_are_atomic_and_worker_independent() {
    use game_parallel::{Amount, Good, Recipe, Stock};
    let mut initial = world();
    let production = initial.production.as_mut().unwrap();
    production.goods.push(Good {
        id: 3,
        name: "chips".into(),
        food: None,
    });
    production.inventories[0].stocks.push(Stock {
        good: 3,
        quantity: 0,
        capacity: 1,
    });
    production.recipes.push(Recipe {
        id: 2,
        name: "split".into(),
        ticks: 1,
        inputs: vec![Amount {
            good: 1,
            quantity: 3,
        }],
        outputs: vec![
            Amount {
                good: 2,
                quantity: 2,
            },
            Amount {
                good: 3,
                quantity: 1,
            },
        ],
    });
    let mut first = work(1, 1);
    first.recipe = 2;
    let mut second = work(2, 2);
    second.recipe = 2;
    initial.start_work(&[first, second]).unwrap();
    let mut results = Vec::new();
    for threads in [1, 4, 8] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        let mut w = initial.clone();
        if threads > 1 {
            w.production.as_mut().unwrap().jobs.reverse();
            w.regions[0].people.reverse();
        }
        pool.install(|| w.step().unwrap());
        w.regions[0].people.sort_unstable_by_key(|p| p.id);
        assert_eq!(stock(&w, 2), 2);
        assert_eq!(stock(&w, 3), 1);
        assert_eq!(w.production.as_ref().unwrap().jobs.len(), 1);
        assert_eq!(w.production.as_ref().unwrap().jobs[0].id, 2);
        results.push(w);
    }
    assert!(results.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn concentrated_capacity_prefix_crosses_chunks_and_preserves_every_output() {
    use game_parallel::{Amount, Job, Person};
    let mut initial = world();
    let count = 5_000;
    initial.regions[0].people = (0..count)
        .map(|id| Person {
            id: id + 100,
            health: 100,
            hunger: 0,
        })
        .collect();
    let p = initial.production.as_mut().unwrap();
    p.inventories[0].stocks[1].capacity = count;
    p.jobs = (0..count)
        .map(|id| Job {
            id,
            worker: id + 100,
            region: 1,
            recipe: 1,
            remaining: 1,
            phase: JobPhase::Working,
            reserved: vec![Amount {
                good: 1,
                quantity: 3,
            }]
            .into(),
            outputs: vec![Amount {
                good: 2,
                quantity: 2,
            }]
            .into(),
        })
        .collect();
    let mut results = Vec::new();
    for threads in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        let mut w = initial.clone();
        if threads == 4 {
            w.production.as_mut().unwrap().jobs.reverse();
            w.regions[0].people.reverse();
        }
        pool.install(|| w.step().unwrap());
        w.regions[0].people.sort_unstable_by_key(|p| p.id);
        assert_eq!(stock(&w, 2), count);
        let p = w.production.as_ref().unwrap();
        assert_eq!(p.jobs.len(), count as usize / 2);
        assert!(
            p.jobs
                .iter()
                .all(|j| j.id >= count / 2 && j.phase == JobPhase::Waiting)
        );
        w.validate().unwrap();
        results.push(w);
    }
    assert_eq!(results[0], results[1]);
}

#[test]
fn large_atomic_start_batch_reserves_shared_inputs_independent_of_worker_count() {
    use game_parallel::Person;
    let mut initial = world();
    let count = 5_000u64;
    initial.regions[0].people = (100..100 + count)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    let input = &mut initial.production.as_mut().unwrap().inventories[0].stocks[0];
    input.quantity = count * 3;
    input.capacity = count * 3;
    let requests: Vec<_> = (0..count).map(|id| work(id, id + 100)).collect();
    let mut results = Vec::new();
    for threads in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        let mut w = initial.clone();
        let mut batch = requests.clone();
        if threads == 4 {
            batch.reverse();
        }
        pool.install(|| w.start_work(&batch).unwrap());
        assert_eq!(stock(&w, 1), 0);
        assert_eq!(w.production.as_ref().unwrap().jobs.len(), count as usize);
        assert_eq!(
            w.production
                .as_ref()
                .unwrap()
                .jobs
                .iter()
                .map(|j| j.reserved[0].quantity)
                .sum::<u64>(),
            count * 3
        );
        results.push(w);
    }
    assert_eq!(results[0], results[1]);
    let mut insufficient = initial;
    insufficient.production.as_mut().unwrap().inventories[0].stocks[0].quantity -= 1;
    let before = insufficient.clone();
    assert!(insufficient.start_work(&requests).is_err());
    assert_eq!(insufficient, before);
}
