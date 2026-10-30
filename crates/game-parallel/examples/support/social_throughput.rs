//! Full-world local encounter benchmark; workers regions people ticks.
use game_parallel::{Knowledge, KnowledgeRecord, Mind, Person, SocialParticipant, World};
use std::time::Instant;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let workers: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let regions: usize = args.get(1).map_or(Ok(100), |v| v.parse())?;
    let people: usize = args.get(2).map_or(Ok(10000), |v| v.parse())?;
    let ticks: usize = args.get(3).map_or(Ok(10), |v| v.parse())?;
    let knowledge = args.get(4).is_some_and(|v| v == "knowledge");
    let needs = args.get(5).is_some_and(|v| v == "needs");
    if workers == 0 || regions == 0 || people > u32::MAX as usize / 4 {
        return Err("invalid arguments".into());
    }
    let mut w = World::from_toml(include_str!("../../../../content/parallel/social.toml"))?;
    if needs {
        w.social.as_mut().unwrap().needs = World::from_toml(include_str!(
            "../../../../content/parallel/social_needs.toml"
        ))?
        .social
        .unwrap()
        .needs;
    }
    w.rules.food_capacity = (people as u32 * 4).max(100);
    w.rules.food_growth = w.rules.food_capacity;
    w.rules.hunger_per_tick = 0;
    w.routes.clear();
    w.regions = (0..regions)
        .map(|id| game_parallel::Region {
            id: id as u64,
            food: 0,
            people: Vec::new(),
        })
        .collect();
    for id in 0..people {
        w.regions[id % regions].people.push(Person {
            id: id as u64,
            hunger: 0,
            health: 100,
        });
    }
    w.social.as_mut().unwrap().participants = (0..people)
        .map(|id| SocialParticipant {
            person: id as u64,
            willing: id % 3 != 0,
        })
        .collect();
    if knowledge {
        w.knowledge = Some(Knowledge {
            max_records: 4,
            minds: (0..people)
                .map(|id| Mind {
                    person: id as u64,
                    records: if regions > 1 {
                        vec![KnowledgeRecord {
                            region: ((id + 1) % regions) as u64,
                            food: 10,
                            observed_at: 0,
                            witness: id as u64,
                            learned_from: None,
                        }]
                    } else {
                        Vec::new()
                    },
                })
                .collect(),
        });
    }
    w.validate()?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    let start = Instant::now();
    pool.install(|| -> Result<(), &'static str> {
        for _ in 0..ticks {
            w.step()?;
        }
        Ok(())
    })?;
    println!(
        "knowledge={knowledge} needs={needs} workers={workers} regions={regions} people={people} ticks={ticks} elapsed_seconds={:.6} relationships={}",
        start.elapsed().as_secs_f64(),
        w.social.as_ref().unwrap().relationships.len()
    );
    std::hint::black_box(w);
    Ok(())
}
