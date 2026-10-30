//! Batch work admission against one shared input. Arguments: workers, jobs.
use game_parallel::{Person, StartWork, World};
use std::time::Instant;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let workers: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let count: usize = args.get(1).map_or(Ok(100_000), |v| v.parse())?;
    if workers == 0 || count == 0 {
        return Err("workers and jobs must be positive".into());
    }
    let mut world = World::from_toml(include_str!("../../../../content/parallel/workshop.toml"))?;
    world.regions.truncate(1);
    world.routes.clear();
    world.regions[0].people = (0..count)
        .map(|id| Person {
            id: id as u64,
            hunger: 0,
            health: 100,
        })
        .collect();
    world.production.as_mut().unwrap().inventories.truncate(1);
    let stock = &mut world.production.as_mut().unwrap().inventories[0].stocks[0];
    stock.quantity = count as u64 * 3;
    stock.capacity = stock.quantity;
    let requests: Vec<_> = (0..count)
        .map(|id| StartWork {
            id: id as u64,
            worker: id as u64,
            region: 1,
            recipe: 1,
            units: 1,
        })
        .collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    let start = Instant::now();
    pool.install(|| world.start_work(&requests))?;
    let elapsed = start.elapsed();
    let production = world.production.as_ref().unwrap();
    if production.jobs.len() != count || production.inventories[0].stocks[0].quantity != 0 {
        return Err("reservation conservation failed".into());
    }
    println!(
        "workers={workers} jobs={count} elapsed_seconds={:.6}",
        elapsed.as_secs_f64()
    );
    Ok(())
}
