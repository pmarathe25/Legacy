//! End-to-end contested packing and departure benchmark.
use game_parallel::{Move, Person, World};
use std::time::Duration;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let workers: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let count: usize = args.get(1).map_or(Ok(100_000), |v| v.parse())?;
    let repetitions: usize = args.get(2).map_or(Ok(10), |v| v.parse())?;
    if workers == 0 || repetitions == 0 || count > u32::MAX as usize / 4 {
        return Err("invalid arguments".into());
    }
    let mut base = World::from_toml(include_str!("../../../../content/parallel/cargo.toml"))?;
    base.rules.food_capacity = (count as u32 * 4).max(100);
    base.regions[0].food = count as u32 * 4;
    base.regions[0].people = (0..count)
        .map(|id| Person {
            id: id as u64,
            hunger: 0,
            health: 100,
        })
        .collect();
    base.regions[1].people.clear();
    base.routes
        .iter_mut()
        .find(|r| r.source == 1 && r.destination == 2)
        .unwrap()
        .ticks = 8;
    let cargo = base.cargo.as_mut().unwrap();
    cargo.capacity = 4;
    cargo.auto_pack_travel = true;
    let requests: Vec<_> = (0..count)
        .rev()
        .map(|id| Move {
            person: id as u64,
            destination: 2,
        })
        .collect();
    base.validate()?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    let mut samples = Vec::new();
    for _ in 0..repetitions {
        let mut w = base.clone();
        let start = std::time::Instant::now();
        pool.install(|| w.move_people(&requests))?;
        samples.push(start.elapsed());
        assert_eq!(w.travelers.len(), count);
        assert_eq!(w.cargo.as_ref().unwrap().holdings.len(), count);
        assert_eq!(w.regions[0].food, 0);
        std::hint::black_box(w);
    }
    samples.sort_unstable();
    let total: Duration = samples.iter().sum();
    println!(
        "workers={workers} departures={count} repetitions={repetitions} median_seconds={:.6} mean_seconds={:.6}",
        samples[repetitions / 2].as_secs_f64(),
        total.as_secs_f64() / repetitions as f64
    );
    Ok(())
}
