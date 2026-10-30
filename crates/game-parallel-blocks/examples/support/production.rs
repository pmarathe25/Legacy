//! Concentrated production lifecycle. Arguments: workers, jobs, reference|blocks.
use game_parallel::{Person, StartWork, World};
use game_parallel_blocks::Engine;
use std::time::Instant;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let workers: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let count: usize = args.get(1).map_or(Ok(100_000), |v| v.parse())?;
    let reference = args.get(2).is_some_and(|v| v == "reference");
    let scarce = args.iter().any(|arg| arg == "scarce");
    if workers == 0 || count == 0 {
        return Err("invalid arguments".into());
    }
    let mut w = World::from_toml(include_str!("../../../../content/parallel/workshop.toml"))?;
    w.goods_cargo = None;
    w.market = None;
    w.regions[1].people.clear();
    w.regions[0].people = (0..count)
        .map(|id| Person {
            id: id as u64,
            health: 100,
            hunger: 0,
        })
        .collect();
    w.rules.hunger_per_tick = 0;
    w.rules.food_growth = 0;
    let inv = &mut w.production.as_mut().unwrap().inventories[0];
    inv.stocks[0].quantity = count as u64 * 3;
    inv.stocks[0].capacity = count as u64 * 3;
    inv.stocks[1].capacity = if scarce {
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
    let mut engine = if reference {
        None
    } else {
        Some(Engine::compile(w.clone())?)
    };
    let mut phases = [std::time::Duration::ZERO; 3];
    let start = Instant::now();
    pool.install(|| -> Result<(), String> {
        if let Some(engine) = &mut engine {
            let at = Instant::now();
            engine.start_work(&requests)?;
            phases[0] = at.elapsed();
            let at = Instant::now();
            engine.step()?;
            phases[1] = at.elapsed();
            let at = Instant::now();
            engine.step()?;
            phases[2] = at.elapsed();
        } else {
            let at = Instant::now();
            w.start_work(&requests)?;
            phases[0] = at.elapsed();
            let at = Instant::now();
            w.step()?;
            phases[1] = at.elapsed();
            let at = Instant::now();
            w.step()?;
            phases[2] = at.elapsed();
        }
        Ok(())
    })?;
    let elapsed = start.elapsed();
    if let Some(engine) = engine {
        w = engine.export();
    }
    let expected = if scarce {
        (count as u64 / 2) * 2
    } else {
        count as u64 * 2
    };
    if w.production.as_ref().unwrap().inventories[0].stocks[1].quantity != expected {
        return Err("production conservation failed".into());
    }
    println!(
        "workers={workers} jobs={count} reference={reference} scarce={scarce} elapsed_seconds={:.6} start={:.6} first={:.6} second={:.6}",
        elapsed.as_secs_f64(),
        phases[0].as_secs_f64(),
        phases[1].as_secs_f64(),
        phases[2].as_secs_f64()
    );
    Ok(())
}
