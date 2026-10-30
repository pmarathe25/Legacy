//! Time one boundary with many movements to or from a single custody.
use game_parallel_lots::{Container, Creation, Lot, Movement, Place, State, Store, Transaction};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let workers: usize = args.first().map_or(Ok(1), |value| value.parse())?;
    let count: u64 = args.get(1).map_or(Ok(100_000), |value| value.parse())?;
    let mode = args.get(2).map_or("source", String::as_str);
    let hot_source = mode == "source";
    let repeats: usize = args.get(3).map_or(Ok(5), |value| value.parse())?;
    if workers == 0 || count < 2 || repeats == 0 {
        return Err("invalid benchmark arguments".into());
    }
    if mode == "recipes" {
        return recipes(workers, count, repeats);
    }
    if !matches!(mode, "source" | "destination") {
        return Err("invalid contention mode".into());
    }
    let inventory = Place::Inventory(1);
    let item = || Lot {
        good: 1,
        quantity: 1,
        expires_at: Some(5),
    };
    let mut containers = vec![Container::new(
        inventory,
        count,
        if hot_source {
            vec![Lot {
                quantity: count / 2,
                ..item()
            }]
        } else {
            vec![]
        },
    )?];
    containers.extend((1..=count).map(|id| {
        Container::new(
            Place::Person(id),
            1,
            if hot_source { vec![] } else { vec![item()] },
        )
        .unwrap()
    }));
    if !hot_source {
        containers[0].capacity = count / 2;
    }
    let state = State {
        tick: 0,
        revision: 0,
        containers,
    };
    let moves: Vec<_> = (1..=count)
        .map(|id| Movement {
            id,
            from: if hot_source {
                inventory
            } else {
                Place::Person(id)
            },
            to: Some(if hot_source {
                Place::Person(id)
            } else {
                inventory
            }),
            good: 1,
            quantity: 1,
        })
        .collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    pool.install(|| {
        let mut samples = Vec::new();
        for round in 0..=repeats {
            let mut store = Store::compile(state.clone()).unwrap();
            let start = Instant::now();
            let draft = store.prepare(17, &moves).unwrap();
            let (outcomes, spoiled) = store.commit(draft).unwrap();
            let elapsed = start.elapsed().as_secs_f64();
            if round > 0 {
                samples.push(elapsed);
            }
            assert!(spoiled.is_empty());
            assert_eq!(
                outcomes.iter().filter(|row| row.accepted).count(),
                count as usize / 2
            );
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "workers={workers} moves={count} contention={} rounds={repeats} median={:.6}",
            if hot_source { "source" } else { "destination" },
            samples[repeats / 2]
        );
    });
    Ok(())
}

fn recipes(workers: usize, count: u64, repeats: usize) -> Result<(), Box<dyn std::error::Error>> {
    let source = Place::Stock { region: 1, good: 1 };
    let destination = Place::Stock { region: 1, good: 2 };
    let state = State {
        tick: 0,
        revision: 0,
        containers: vec![
            Container::new(
                source,
                count,
                vec![Lot {
                    good: 1,
                    quantity: count / 2,
                    expires_at: Some(5),
                }],
            )?,
            Container::new(destination, count / 2, vec![])?,
        ],
    };
    let actions: Vec<_> = (1..=count)
        .map(|id| Transaction {
            id,
            movements: vec![Movement {
                id,
                from: source,
                to: None,
                good: 1,
                quantity: 1,
            }],
            creations: vec![Creation {
                id: count + id,
                to: destination,
                lot: Lot {
                    good: 2,
                    quantity: 1,
                    expires_at: Some(5),
                },
            }],
        })
        .collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    pool.install(|| {
        let mut samples = Vec::new();
        let mut last_granted = 0;
        for round in 0..=repeats {
            let mut store = Store::compile(state.clone()).unwrap();
            let start = Instant::now();
            let draft = store.prepare_transactions(17, &actions).unwrap();
            let granted = draft.transactions().iter().filter(|row| row.accepted).count() as u64;
            let (moves, spoiled) = store.commit(draft).unwrap();
            let elapsed = start.elapsed().as_secs_f64();
            if round > 0 { samples.push(elapsed); }
            assert!(spoiled.is_empty() && granted > 0 && granted <= count / 2);
            assert_eq!(moves.iter().filter(|row| row.accepted).count() as u64, granted);
            let final_state = store.export();
            assert_eq!(final_state.containers[0].used, count / 2 - granted);
            assert_eq!(final_state.containers[1].used, granted);
            last_granted = granted;
        }
        samples.sort_by(f64::total_cmp);
        println!("workers={workers} recipes={count} granted={last_granted} rounds={repeats} median={:.6}", samples[repeats / 2]);
    });
    Ok(())
}
