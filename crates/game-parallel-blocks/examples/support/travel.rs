//! Concentrated departure and arrival. Arguments: workers, people, reference|blocks.
use game_parallel::{Move, Person, World};
use game_parallel_blocks::Engine;
use std::time::Instant;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let workers: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let count: usize = args.get(1).map_or(Ok(100_000), |v| v.parse())?;
    let reference = args.get(2).is_some_and(|v| v == "reference");
    if workers == 0 || count == 0 {
        return Err("invalid arguments".into());
    }
    let mut world = World::from_toml(include_str!("../../../../content/parallel/example.toml"))?;
    world.regions[0].people = (0..count)
        .map(|id| Person {
            id: id as u64 + 100,
            health: 100,
            hunger: 0,
        })
        .collect();
    let moves: Vec<_> = (0..count)
        .map(|id| Move {
            person: id as u64 + 100,
            destination: 2,
        })
        .collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    let mut engine = if reference {
        None
    } else {
        Some(Engine::compile(world.clone())?)
    };
    let mut phases = [std::time::Duration::ZERO; 3];
    let start = Instant::now();
    pool.install(|| -> Result<(), String> {
        if let Some(engine) = &mut engine {
            let at = Instant::now();
            engine.move_people(&moves)?;
            phases[0] = at.elapsed();
            let at = Instant::now();
            engine.step()?;
            phases[1] = at.elapsed();
            let at = Instant::now();
            engine.step()?;
            phases[2] = at.elapsed();
        } else {
            let at = Instant::now();
            world.move_people(&moves)?;
            phases[0] = at.elapsed();
            let at = Instant::now();
            world.step()?;
            phases[1] = at.elapsed();
            let at = Instant::now();
            world.step()?;
            phases[2] = at.elapsed();
        }
        Ok(())
    })?;
    let elapsed = start.elapsed();
    if let Some(engine) = engine {
        world = engine.export();
    }
    if !world.travelers.is_empty() || world.regions[1].people.len() != count + 1 {
        return Err("travelers lost".into());
    }
    println!(
        "workers={workers} people={count} reference={reference} elapsed_seconds={:.6} depart={:.6} travel={:.6} arrival={:.6}",
        elapsed.as_secs_f64(),
        phases[0].as_secs_f64(),
        phases[1].as_secs_f64(),
        phases[2].as_secs_f64()
    );
    Ok(())
}
