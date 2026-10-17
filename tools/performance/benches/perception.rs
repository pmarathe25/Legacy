//! Growing personal history and distant ecology must stay outside local decisions.
use game_core::*;
use game_knowledge::{Fact, Facts, Knowledge, ObserveSurroundings};
use game_sim::*;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Instant};
#[derive(Clone, Serialize, Deserialize)]
struct Seed {
    people: Vec<PersonId>,
    memories: usize,
    roads: usize,
}
impl Request for Seed {
    const KEY: &'static str = "fixture.seed_memories";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Options(PersonId);
impl Request for Options {
    const KEY: &'static str = "fixture.options";
    type Response = Vec<game_provisioning::Plan>;
}
// Read preparation on one committed view. Receiver effects remain serial.
#[derive(Clone, Serialize, Deserialize)]
struct BatchOptions {
    people: Vec<PersonId>,
    parallel: bool,
}
impl Request for BatchOptions {
    const KEY: &'static str = "fixture.batch_options";
    type Response = Vec<Vec<game_provisioning::Plan>>;
}
fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_skills::plugin(&mut b).unwrap();
    game_ecology::plugin(&mut b).unwrap();
    game_knowledge::plugin(&mut b).unwrap();
    // Fixture construction registers with the knowledge owner; no gameplay
    // request or application can inject these artificial historical memories.
    b.request::<Seed>(|tx, r| {
        let value: Arc<game_knowledge::FactValue> =
            Arc::new(serde_json::json!({ "story": "shared historical account" }).into());
        let mut memories: Facts = (0..r.memories)
            .map(|i| {
                (
                    format!("memory:{i:08}").into(),
                    Fact {
                        value: value.clone(),
                        source: r.people[0],
                        observed: SimTime(0),
                        revision: 0,
                        confidence: 90,
                    },
                )
            })
            .collect();
        let road: Arc<game_knowledge::FactValue> =
            Arc::new(serde_json::json!({ "name": "obsolete map", "links": {} }).into());
        for id in 0..r.roads {
            memories.insert(
                format!("roads:{}", 1_000_000 + id).into(),
                Fact {
                    value: road.clone(),
                    source: r.people[0],
                    observed: SimTime(0),
                    revision: 0,
                    confidence: 90,
                },
            );
        }
        for person in r.people {
            tx.edit::<Knowledge>()?.0.insert(person, memories.clone());
        }
        Ok(())
    })
    .unwrap();
    b.request::<Options>(|tx, r| {
        Ok(game_provisioning::options(tx, r.0, 1)?
            .into_iter()
            .map(|o| o.plan)
            .collect())
    })
    .unwrap();
    b.request::<BatchOptions>(|tx, r| {
        let prepare = |person: &PersonId| {
            Ok(game_provisioning::options(tx, *person, 1)?
                .into_iter()
                .map(|o| o.plan)
                .collect())
        };
        if r.parallel {
            tx.compute()
                .install(|| r.people.par_iter().map(prepare).collect())
        } else {
            r.people.iter().map(prepare).collect()
        }
    })
    .unwrap();
    b
}
fn main() {
    use game_performance::{metrics, setting};
    let count = setting("LEGACY_POPULATION", 1000usize);
    let workers = setting("LEGACY_WORKERS", 1usize);
    let memories = setting("LEGACY_MEMORIES", 1000usize);
    let roads = setting("LEGACY_ROADS", 1000usize);
    let patches = setting("LEGACY_PATCHES", 10_000usize);
    let rounds = setting("LEGACY_ROUNDS", 10usize);
    let local_patches = setting("LEGACY_LOCAL_PATCHES", 1usize);
    assert!(
        count > 0 && patches > 0 && rounds > 0 && local_patches > 0 && local_patches <= patches
    );
    metrics::metadata();
    let mut host = builder().build(1, workers).unwrap();
    host.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let home = host
        .dispatch(game_land::CreateArea {
            name: "home".into(),
        })
        .unwrap();
    let away = host
        .dispatch(game_land::CreateArea {
            name: "away".into(),
        })
        .unwrap();
    host.dispatch(game_land::Link {
        from: home,
        to: away,
        duration: MINUTE,
    })
    .unwrap();
    let mut people = vec![];
    for _ in 0..count {
        let person = host
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        host.dispatch(game_transport::Place { person, area: home })
            .unwrap();
        people.push(person);
    }
    let store = host
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(home),
            capacity: 100_000,
        })
        .unwrap();
    host.dispatch(game_cargo::Deposit {
        inventory: store,
        good: game_cargo::FOOD,
        quantity: 1000,
    })
    .unwrap();
    host.dispatch(game_property::ClaimInventory {
        claimant: people[0],
        inventory: store,
        recognized_by: Default::default(),
    })
    .unwrap();
    for person in &people[1..] {
        host.dispatch(game_property::Permit {
            issuer: people[0],
            inventory: store,
            person: *person,
            allow: true,
        })
        .unwrap();
    }
    for i in 0..patches {
        host.dispatch(game_ecology::Create(game_ecology::Patch {
            area: if i < local_patches { home } else { away },
            good: game_cargo::FOOD,
            stock: 100,
            capacity: 100,
            renew: 0,
        }))
        .unwrap();
    }
    host.dispatch(Seed {
        people: people.clone(),
        memories,
        roads,
    })
    .unwrap();
    for person in &people {
        host.dispatch(ObserveSurroundings { person: *person })
            .unwrap();
    }
    let sample = metrics::Sample::new();
    let mut perception = 0.;
    let mut choices = 0.;
    let mut routing = 0.;
    for _ in 0..rounds {
        let start = Instant::now();
        for person in &people {
            host.dispatch(ObserveSurroundings { person: *person })
                .unwrap();
        }
        perception += start.elapsed().as_secs_f64() * 1000.;
        let start = Instant::now();
        for person in &people {
            let options = host.query(Options(*person)).unwrap();
            assert_eq!(options.len(), 1 + local_patches);
            assert!(options.contains(&game_provisioning::Plan::Meal {
                inventory: store,
                good: Some(game_cargo::FOOD)
            }));
        }
        choices += start.elapsed().as_secs_f64() * 1000.;
        let start = Instant::now();
        for person in &people {
            let facts = &host.get::<Knowledge>().unwrap().0[person];
            assert_eq!(
                game_knowledge::route(facts, home, away).unwrap().duration,
                MINUTE
            );
        }
        routing += start.elapsed().as_secs_f64() * 1000.;
    }
    sample.report("perception_choices");
    metrics::rss();
    assert!(
        host.get::<Knowledge>()
            .unwrap()
            .0
            .values()
            .all(|facts| facts.len() == memories + roads + 3 + local_patches)
    );
    assert_eq!(
        host.get::<game_cargo::Cargo>().unwrap().0[&store].amount(game_cargo::FOOD),
        1000
    );
    // Fixed logical batch sizes; both paths read the same immutable phase and
    // retain stable-person order. These timings do not include arbitration.
    let before_preparation = host.save().unwrap();
    for size in [128, 512] {
        let mut serial = 0.;
        let mut parallel = 0.;
        for round in 0..rounds {
            for chunk in people.chunks(size) {
                let first_parallel = round % 2 == 1;
                let mut previous = None;
                for is_parallel in [first_parallel, !first_parallel] {
                    let request = BatchOptions {
                        people: chunk.to_vec(),
                        parallel: is_parallel,
                    };
                    let start = Instant::now();
                    let actual = host.query(request).unwrap();
                    let elapsed = start.elapsed().as_secs_f64() * 1000.;
                    if is_parallel {
                        parallel += elapsed;
                    } else {
                        serial += elapsed;
                    }
                    if let Some(expected) = &previous {
                        assert_eq!(&actual, expected);
                    } else {
                        previous = Some(actual);
                    }
                }
            }
        }
        println!(
            "preparation_batch={size} workers={workers} people={count} rounds={rounds} serial_ms={serial:.3} parallel_ms={parallel:.3}"
        );
    }
    let save = host.save().unwrap();
    assert_eq!(
        save, before_preparation,
        "preparation cannot alter its view"
    );
    if let Ok(path) = std::env::var("LEGACY_SNAPSHOT") {
        std::fs::write(path, &save).unwrap();
    }
    println!(
        "population={count} workers={workers} memories={memories} roads={roads} patches={patches} local_patches={local_patches} rounds={rounds} perception_ms={perception:.3} choices_ms={choices:.3} routing_ms={routing:.3} save_bytes={} hash={:016x}",
        save.len(),
        game_performance::hash(save.as_bytes())
    );
}
