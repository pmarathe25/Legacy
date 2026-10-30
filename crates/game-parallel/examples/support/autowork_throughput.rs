//! Autonomous work with one concentrated contested input. Arguments: workers, people.
use game_parallel::{Person, WorkPolicy, World};
use std::time::Instant;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let workers: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let count: usize = args.get(1).map_or(Ok(100_000), |v| v.parse())?;
    if workers == 0 || count == 0 {
        return Err("workers and people must be positive".into());
    }
    let mut w = World::from_toml(include_str!("../../../../content/parallel/autowork.toml"))?;
    w.regions[0].people = (0..count)
        .map(|id| Person {
            id: id as u64 + 100,
            health: 100,
            hunger: 0,
        })
        .collect();
    w.rules.hunger_per_tick = 0;
    w.rules.food_growth = 0;
    w.autowork.as_mut().unwrap().policies = (0..count)
        .map(|id| WorkPolicy {
            person: id as u64 + 100,
            recipe: 1,
            enabled: true,
            disregard_claims: false,
        })
        .collect();
    let stock = &mut w.production.as_mut().unwrap().inventories[0].stocks[0];
    stock.quantity = (count / 2) as u64 * 3;
    stock.capacity = stock.quantity.max(3);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    let start = Instant::now();
    pool.install(|| w.step())?;
    let elapsed = start.elapsed();
    if w.production.as_ref().unwrap().jobs.len() != count / 2
        || w.production.as_ref().unwrap().inventories[0].stocks[0].quantity != 0
    {
        return Err("work reservations not conserved".into());
    }
    println!(
        "workers={workers} people={count} granted={} elapsed_seconds={:.6}",
        count / 2,
        elapsed.as_secs_f64()
    );
    Ok(())
}
