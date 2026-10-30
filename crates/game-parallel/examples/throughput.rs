use game_parallel::{Person, Region, World};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let workers: usize = args.get(1).map_or(Ok(1), |v| v.parse())?;
    let regions: usize = args.get(2).map_or(Ok(1000), |v| v.parse())?;
    let population: usize = args.get(3).map_or(Ok(100), |v| v.parse())?;
    let ticks: usize = args.get(4).map_or(Ok(100), |v| v.parse())?;
    let scarce = args.get(5).is_some_and(|v| v == "scarce");
    if workers == 0 {
        return Err("workers must be positive".into());
    }
    let mut world = World::from_toml(include_str!("../../../content/parallel/example.toml"))?;
    world.rules.food_capacity = u32::try_from(population.saturating_mul(4))
        .map_err(|_| "population too large")?
        .max(100);
    world.rules.food_growth = if scarce {
        u32::try_from(population / 4)?
    } else {
        world.rules.food_capacity
    };
    if scarce {
        world.rules.starvation_threshold = u32::MAX;
    }
    world.regions = (0..regions)
        .map(|r| Region {
            id: r as u64,
            food: if scarce { 0 } else { world.rules.food_capacity },
            people: (0..population)
                .map(|p| Person {
                    id: (r as u64)
                        .checked_mul(population as u64)
                        .and_then(|v| v.checked_add(p as u64))
                        .expect("population exceeds id space"),
                    hunger: 0,
                    health: world.rules.max_health,
                })
                .collect(),
        })
        .collect();
    world.routes.clear();
    world.validate()?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    let start = Instant::now();
    pool.install(|| -> Result<(), &'static str> {
        for _ in 0..ticks {
            world.step()?;
        }
        Ok(())
    })?;
    println!(
        "scarce={scarce} workers={workers} regions={regions} people_per_region={population} ticks={ticks} elapsed_seconds={:.6}",
        start.elapsed().as_secs_f64()
    );
    std::hint::black_box(world);
    Ok(())
}
