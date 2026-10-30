//! Contended personal goods transfer. Arguments: workers, people.
use game_parallel::{GoodsDirection, GoodsTransfer, Person, World};
use std::time::Instant;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let workers: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let count: usize = args.get(1).map_or(Ok(100_000), |v| v.parse())?;
    if workers == 0 || count == 0 {
        return Err("workers and people must be positive".into());
    }
    let mut world = World::from_toml(include_str!("../../../../content/parallel/workshop.toml"))?;
    world.regions.truncate(1);
    world.routes.clear();
    world.regions[0].people = (0..count)
        .map(|id| Person {
            id: id as u64,
            health: 100,
            hunger: 0,
        })
        .collect();
    let production = world.production.as_mut().unwrap();
    production.inventories.truncate(1);
    production.inventories[0].stocks[0].capacity = count as u64;
    production.inventories[0].stocks[0].quantity = count as u64;
    let transfers: Vec<_> = (0..count)
        .map(|id| GoodsTransfer {
            person: id as u64,
            good: 1,
            quantity: 1,
            direction: GoodsDirection::Pickup,
        })
        .collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    let start = Instant::now();
    pool.install(|| world.transfer_goods(&transfers))?;
    let elapsed = start.elapsed();
    if world.production.as_ref().unwrap().inventories[0].stocks[0].quantity != 0
        || world.goods_cargo.as_ref().unwrap().holdings.len() != count
    {
        return Err("goods were not conserved".into());
    }
    println!(
        "workers={workers} people={count} elapsed_seconds={:.6}",
        elapsed.as_secs_f64()
    );
    Ok(())
}
