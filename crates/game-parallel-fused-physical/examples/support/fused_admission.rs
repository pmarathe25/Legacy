//! Compare bulk admission and two physical/work intervals. Args: workers, people, [scarce].
use game_parallel::{Person, StartWork, World};
use game_parallel_blocks::Engine;
use game_parallel_fused_physical::Kernel;
use std::time::Instant;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let workers: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let count: usize = args.get(1).map_or(Ok(1_000_000), |v| v.parse())?;
    let scarce = args.get(2).is_some_and(|arg| arg == "scarce");
    if workers == 0 || count == 0 {
        return Err("invalid arguments".into());
    }
    let mut world = World::from_toml(include_str!("../../../../content/parallel/workshop.toml"))?;
    world.market = None;
    world.goods_cargo = None;
    world.rules.hunger_per_tick = 0;
    world.rules.food_growth = 0;
    world.regions[1].people.clear();
    world.regions[0].people = (0..count)
        .map(|id| Person {
            id: id as u64,
            health: 100,
            hunger: 0,
        })
        .collect();
    let production = world.production.as_mut().unwrap();
    production.inventories[0].stocks[0].quantity = count as u64 * 3;
    production.inventories[0].stocks[0].capacity = count as u64 * 3;
    production.inventories[0].stocks[1].capacity = if scarce {
        count as u64
    } else {
        count as u64 * 2
    };
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
    let mut reference = Engine::compile(world.clone())?;
    let mut fused = Kernel::compile(world)?;
    let mut a = [std::time::Duration::ZERO; 3];
    let mut b = [std::time::Duration::ZERO; 3];
    let start = Instant::now();
    pool.install(|| -> Result<(), String> {
        let at = Instant::now();
        reference.start_work(&requests)?;
        a[0] = at.elapsed();
        let at = Instant::now();
        reference.step()?;
        a[1] = at.elapsed();
        let at = Instant::now();
        reference.step()?;
        a[2] = at.elapsed();
        Ok(())
    })?;
    let normal = start.elapsed();
    let start = Instant::now();
    pool.install(|| -> Result<(), String> {
        let at = Instant::now();
        fused.start_work(&requests)?;
        b[0] = at.elapsed();
        let at = Instant::now();
        fused.step()?;
        b[1] = at.elapsed();
        let at = Instant::now();
        fused.step()?;
        b[2] = at.elapsed();
        Ok(())
    })?;
    let combined = start.elapsed();
    if reference.export() != fused.export() {
        return Err("different outcomes".into());
    }
    println!(
        "workers={workers} people={count} scarce={scarce} blocks={:.6} fused={:.6} block_start={:.6} fused_start={:.6} block_ticks={:.6} fused_ticks={:.6}",
        normal.as_secs_f64(),
        combined.as_secs_f64(),
        a[0].as_secs_f64(),
        b[0].as_secs_f64(),
        (a[1] + a[2]).as_secs_f64(),
        (b[1] + b[2]).as_secs_f64()
    );
    Ok(())
}
