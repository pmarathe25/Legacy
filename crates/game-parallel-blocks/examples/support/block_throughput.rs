//! Sustained one-region food contention. Arguments: workers, people, ticks, reference|blocks.
use game_parallel::{Person, World};
use game_parallel_blocks::Engine;
use std::time::Instant;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let workers: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let count: usize = args.get(1).map_or(Ok(1_000_000), |v| v.parse())?;
    let ticks: usize = args.get(2).map_or(Ok(20), |v| v.parse())?;
    let reference = args.get(3).is_some_and(|mode| mode == "reference");
    if workers == 0 || count == 0 || count > u32::MAX as usize {
        return Err("invalid arguments".into());
    }
    let mut world = World::from_toml(include_str!("../../../../content/parallel/example.toml"))?;
    world.regions.truncate(1);
    world.routes.clear();
    world.regions[0].people = (0..count)
        .map(|id| Person {
            id: id as u64,
            health: 100,
            hunger: 0,
        })
        .collect();
    world.regions[0].food = (count / 2) as u32;
    world.rules.food_growth = (count / 2) as u32;
    world.rules.food_capacity = count as u32;
    world.rules.food_per_meal = 1;
    world.rules.hunger_satisfied = 1;
    world.rules.hunger_per_tick = 1;
    world.rules.starvation_threshold = u32::MAX;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    let mut blocks = if reference {
        None
    } else {
        Some(Engine::compile(world.clone())?)
    };
    let start = Instant::now();
    pool.install(|| -> Result<(), &'static str> {
        for _ in 0..ticks {
            if let Some(engine) = &mut blocks {
                engine.step()?;
            } else {
                world.step()?;
            }
        }
        Ok(())
    })?;
    let elapsed = start.elapsed();
    if let Some(engine) = blocks {
        world = engine.export();
    }
    let hunger: u64 = world.regions[0]
        .people
        .iter()
        .map(|p| u64::from(p.hunger))
        .sum();
    println!(
        "workers={workers} people={count} ticks={ticks} reference={reference} elapsed_seconds={:.6} hunger={hunger}",
        elapsed.as_secs_f64()
    );
    Ok(())
}
