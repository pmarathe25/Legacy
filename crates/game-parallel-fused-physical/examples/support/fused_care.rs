//! Concentrated medical care. Args: workers, caregivers, [hot].
use game_parallel::{Care, CareStart, Person, World};
use game_parallel_fused_physical::Kernel;
use std::time::Instant;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let workers: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let count: usize = args.get(1).map_or(Ok(100000), |v| v.parse())?;
    let hot = args.get(2).is_some_and(|v| v == "hot");
    if workers == 0 || count == 0 {
        return Err("invalid arguments".into());
    }
    let mut world = World::from_toml(include_str!("../../../../content/parallel/workshop.toml"))?;
    world.market = None;
    world.goods_cargo = None;
    world.rules.hunger_per_tick = 0;
    world.rules.recovery = 0;
    world.regions[1].people.clear();
    world.regions[0].people = (0..count * 2)
        .map(|id| Person {
            id: id as u64 + 1,
            health: if id >= count { 1 } else { 100 },
            hunger: 0,
        })
        .collect();
    let stock = &mut world.production.as_mut().unwrap().inventories[0].stocks[0];
    stock.quantity = count as u64;
    stock.capacity = count as u64;
    world.care = Some(Care {
        medicine: 1,
        dose: 1,
        duration: 1,
        healing: 1,
        next_id: 1,
        jobs: vec![],
        events: vec![],
    });
    let requests: Vec<_> = (0..count)
        .map(|i| CareStart {
            id: i as u64 + 1,
            caregiver: i as u64 + 1,
            patient: (if hot { count } else { count + i }) as u64 + 1,
        })
        .collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    let mut kernel = Kernel::compile(world)?;
    let start = Instant::now();
    pool.install(|| kernel.start_care(&requests))?;
    let admission = start.elapsed();
    let start = Instant::now();
    pool.install(|| kernel.step())?;
    let tick = start.elapsed();
    let state = kernel.export();
    assert_eq!(state.care.unwrap().events.len(), count);
    println!(
        "workers={workers} caregivers={count} hot={hot} admission={:.6} tick={:.6}",
        admission.as_secs_f64(),
        tick.as_secs_f64()
    );
    Ok(())
}
