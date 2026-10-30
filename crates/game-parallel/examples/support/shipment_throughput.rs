//! Concentrated shipment benchmark, including world stepping and cargo updates.
use game_parallel::{Shipment, World};
use std::time::Instant;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let workers: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let count: usize = args.get(1).map_or(Ok(100_000), |v| v.parse())?;
    let ticks: usize = args.get(2).map_or(Ok(100), |v| v.parse())?;
    if workers == 0 || count > u32::MAX as usize / 4 {
        return Err("invalid arguments".into());
    }
    let mut w = World::from_toml(include_str!("../../../../content/parallel/example.toml"))?;
    w.rules.food_capacity = (count as u32 * 4).max(100);
    w.rules.food_growth = 0;
    for region in &mut w.regions {
        region.people.clear();
        region.food = 0;
    }
    w.regions[1].food = w.rules.food_capacity;
    w.shipments = (0..count)
        .map(|id| Shipment {
            id: id as u64,
            source: 1,
            destination: 2,
            food: 1,
            ticks_remaining: 1,
        })
        .collect();
    w.validate()?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    let initial_food = u64::from(w.regions[1].food) + count as u64;
    let mut consumed = 0u64;
    let start = Instant::now();
    pool.install(|| -> Result<(), &'static str> {
        for _ in 0..ticks {
            // Model an external sink clearing a fixed amount of storage each tick.
            let taken = w.regions[1].food.min((count as u32 / 200).max(1));
            w.regions[1].food -= taken;
            consumed += u64::from(taken);
            w.step()?;
        }
        Ok(())
    })?;
    let final_food =
        u64::from(w.regions[1].food) + w.shipments.iter().map(|s| u64::from(s.food)).sum::<u64>();
    assert_eq!(initial_food, final_food + consumed);
    println!(
        "workers={workers} shipments={count} ticks={ticks} elapsed_seconds={:.6} remaining={}",
        start.elapsed().as_secs_f64(),
        w.shipments.len()
    );
    std::hint::black_box(w);
    Ok(())
}
