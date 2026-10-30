//! Compare resident physics and active production. Args: workers, people, ticks, [finish].
use game_parallel::{Amount, Job, JobPhase, Person, World};
use game_parallel_blocks::Engine;
use game_parallel_fused_physical::Kernel;
use std::time::Instant;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let workers: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let count: usize = args.get(1).map_or(Ok(1_000_000), |v| v.parse())?;
    let ticks: usize = args.get(2).map_or(Ok(2), |v| v.parse())?;
    let finish = args.get(3).is_some_and(|v| v == "finish");
    if workers == 0 || count == 0 || ticks == 0 {
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
    production.inventories[0].stocks[1].capacity = count as u64 * 2;
    production.jobs = (0..count)
        .map(|id| Job {
            id: id as u64,
            worker: id as u64,
            region: 1,
            recipe: 1,
            remaining: (ticks + usize::from(!finish)) as u64,
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
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    let mut reference = Engine::compile(world.clone())?;
    let mut fused = Kernel::compile(world)?;
    let start = Instant::now();
    pool.install(|| -> Result<(), &'static str> {
        for _ in 0..ticks {
            reference.step()?;
        }
        Ok(())
    })?;
    let normal = start.elapsed();
    let start = Instant::now();
    pool.install(|| -> Result<(), &'static str> {
        for _ in 0..ticks {
            fused.step()?;
        }
        Ok(())
    })?;
    let combined = start.elapsed();
    if reference.export() != fused.export() {
        return Err("different outcomes".into());
    }
    println!(
        "workers={workers} people={count} ticks={ticks} finish={finish} blocks={:.6} fused={:.6}",
        normal.as_secs_f64(),
        combined.as_secs_f64()
    );
    Ok(())
}
