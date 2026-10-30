//! Full-world farming throughput. Arguments: workers, people, fields, ticks.
use game_parallel::{CropPhase, Field, Person, World};
use std::time::Instant;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let threads: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let people: usize = args.get(1).map_or(Ok(100_000), |v| v.parse())?;
    let fields: usize = args.get(2).map_or(Ok(100_000), |v| v.parse())?;
    let ticks: usize = args.get(3).map_or(Ok(100), |v| v.parse())?;
    if threads == 0 || fields > u32::MAX as usize / 8 {
        return Err("invalid arguments".into());
    }
    let mut w = World::from_toml(include_str!("../../../../content/parallel/farming.toml"))?;
    w.regions.truncate(1);
    w.routes.clear();
    w.rules.food_capacity = (fields as u32 * 8).max(100);
    w.rules.food_growth = 0;
    w.rules.hunger_per_tick = 0;
    w.regions[0].food = fields as u32 * 4;
    w.regions[0].people = (0..people)
        .map(|id| Person {
            id: id as u64,
            hunger: 0,
            health: 100,
        })
        .collect();
    w.agriculture.as_mut().unwrap().farms[0].fields = (0..fields)
        .map(|id| Field {
            id: id as u64,
            phase: match id % 3 {
                0 => CropPhase::Fallow,
                1 => CropPhase::Growing,
                _ => CropPhase::Ripe,
            },
            work: 0,
        })
        .collect();
    w.validate()?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()?;
    let start = Instant::now();
    pool.install(|| -> Result<(), &'static str> {
        for _ in 0..ticks {
            w.step()?;
        }
        Ok(())
    })?;
    println!(
        "workers={threads} people={people} fields={fields} ticks={ticks} elapsed_seconds={:.6}",
        start.elapsed().as_secs_f64()
    );
    std::hint::black_box(w);
    Ok(())
}
