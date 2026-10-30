//! Concentrated workshop arbitration with independent workers. Args: threads jobs ticks.
use game_parallel::{Amount, Job, JobPhase, Person, World};
use std::time::Instant;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let threads: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let count: usize = args.get(1).map_or(Ok(100_000), |v| v.parse())?;
    let ticks: usize = args.get(2).map_or(Ok(10), |v| v.parse())?;
    let scarce = args.get(3).is_some_and(|value| value == "scarce");
    if threads == 0 || count == 0 {
        return Err("threads and jobs must be positive".into());
    }
    let mut world = World::from_toml(include_str!("../../../../content/parallel/workshop.toml"))?;
    world.regions.truncate(1);
    world.routes.clear();
    world.rules.food_growth = 0;
    world.rules.hunger_per_tick = 0;
    world.regions[0].people = (0..count)
        .map(|id| Person {
            id: id as u64,
            hunger: 0,
            health: 100,
        })
        .collect();
    let production = world.production.as_mut().unwrap();
    production.inventories.truncate(1);
    let capacity = if scarce {
        count as u64
    } else {
        count as u64 * 2
    };
    production.inventories[0].stocks[1].capacity = capacity;
    production.jobs = (0..count)
        .map(|id| Job {
            id: id as u64,
            worker: id as u64,
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
    world.validate()?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()?;
    let phase_only = args.get(4).is_some_and(|value| value == "phase");
    let mut prepare_time = std::time::Duration::ZERO;
    let mut commit_time = std::time::Duration::ZERO;
    let mut worlds: Vec<_> = (0..ticks).map(|_| world.clone()).collect();
    let start = Instant::now();
    pool.install(|| -> Result<(), &'static str> {
        for world in &mut worlds {
            if phase_only {
                let mark = Instant::now();
                let draft = world
                    .production
                    .as_ref()
                    .unwrap()
                    .prepare(&world.regions, &world.travelers, world.tick, world.tick + 1)
                    .map_err(|_| "preparation failed")?;
                prepare_time += mark.elapsed();
                let mark = Instant::now();
                draft.commit(world.production.as_mut().unwrap());
                commit_time += mark.elapsed();
            } else {
                world.step()?;
            }
            let p = world.production.as_ref().unwrap();
            let delivered = (capacity / 2).min(count as u64);
            if p.jobs.len() != count - delivered as usize
                || p.inventories[0].stocks[1].quantity != delivered * 2
            {
                return Err("output conservation failed");
            }
        }
        Ok(())
    })?;
    println!(
        "workers={threads} jobs={count} ticks={ticks} scarce={scarce} phase_only={phase_only} elapsed_seconds={:.6} prepare_seconds={:.6} commit_seconds={:.6}",
        start.elapsed().as_secs_f64(),
        prepare_time.as_secs_f64(),
        commit_time.as_secs_f64()
    );
    Ok(())
}
