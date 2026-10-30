//! Measure bounded inbox admission and a tick with one addressed holder.
use super::*;
use game_parallel::{FoodAidPolicy, FoodAsker, FoodRequest, Holding};
pub fn run(workers: usize, count: usize, repeats: usize) -> Result<(), Box<dyn std::error::Error>> {
    if workers == 0 || count < 2 || count > (u32::MAX / 2) as usize || repeats == 0 {
        return Err("invalid benchmark arguments".into());
    }
    let mut world = World::from_toml(include_str!("../../../../content/parallel/food_aid.toml"))?;
    world.rules.food_capacity = (count * 2) as u32;
    world.regions[0].food = 0;
    world.regions[0].people = (1..=count as u64)
        .map(|id| Person {
            id,
            health: 100,
            hunger: if id == 1 { 0 } else { 3 },
        })
        .collect();
    world.regions[1].people.clear();
    world.opinions = None;
    let cargo = world.cargo.as_mut().unwrap();
    cargo.acquire_when_hungry.clear();
    cargo.holdings = vec![Holding { person: 1, food: 2 }];
    cargo.food_aid[0].requires_request = true;
    cargo.food_aid[0].threshold = 0;
    let requests: Vec<_> = (2..=count as u64)
        .map(|requester| FoodRequest {
            id: requester,
            requester,
            giver: 1,
            amount: 2,
            expires_at: 5,
        })
        .collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    pool.install(|| {
        let mut admission = Vec::new();
        let mut ticks = Vec::new();
        for round in 0..=repeats {
            let mut kernel = Kernel::compile(world.clone()).unwrap();
            let start = Instant::now();
            kernel.request_food_batch(&requests).unwrap();
            let admitted = start.elapsed().as_secs_f64();
            let start = Instant::now();
            kernel.step().unwrap();
            let tick = start.elapsed().as_secs_f64();
            if round > 0 { admission.push(admitted); ticks.push(tick); }
            let result = kernel.export();
            let cargo = result.cargo.unwrap();
            assert_eq!(cargo.food_requests.len(), count - 2);
            assert_eq!(cargo.holdings.iter().map(|h| u64::from(h.food)).sum::<u64>(), 2);
        }
        admission.sort_by(f64::total_cmp); ticks.sort_by(f64::total_cmp);
        println!("workers={workers} people={count} requests={} rounds={repeats} admission_median={:.6} tick_median={:.6}", count-1, admission[admission.len()/2], ticks[ticks.len()/2]);
    });
    Ok(())
}

pub fn run_auto(
    workers: usize,
    count: usize,
    repeats: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    if workers == 0 || count < 2 || count > (u32::MAX / 2) as usize || repeats == 0 {
        return Err("invalid benchmark arguments".into());
    }
    let mut world = World::from_toml(include_str!("../../../../content/parallel/food_aid.toml"))?;
    world.rules.food_capacity = (count * 2) as u32;
    world.regions[0].food = 0;
    world.regions[0].people = (1..=count as u64)
        .map(|id| Person {
            id,
            health: 100,
            hunger: if id == 1 { 0 } else { 3 },
        })
        .collect();
    world.regions[1].people.clear();
    world.opinions = None;
    let cargo = world.cargo.as_mut().unwrap();
    cargo.acquire_when_hungry.clear();
    cargo.holdings = vec![Holding { person: 1, food: 2 }];
    cargo.food_askers = (2..=count as u64)
        .map(|person| FoodAsker {
            person,
            giver: 1,
            choose_known: false,
            amount: 2,
            interval: 1,
            lifetime: 5,
            enabled: true,
        })
        .collect();
    cargo.food_aid = (2..=count as u64)
        .map(|recipient| FoodAidPolicy {
            giver: 1,
            recipient,
            amount: 2,
            retain: 0,
            help_value: 1,
            threshold: 1,
            interval: 1,
            requires_request: true,
            enabled: true,
        })
        .collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    pool.install(|| {
        let mut creation = Vec::new(); let mut response = Vec::new();
        for round in 0..=repeats {
            let mut kernel = Kernel::compile(world.clone()).unwrap();
            let start = Instant::now(); kernel.step().unwrap(); let first = start.elapsed().as_secs_f64();
            let start = Instant::now(); kernel.step().unwrap(); let second = start.elapsed().as_secs_f64();
            if round > 0 { creation.push(first); response.push(second); }
            let result = kernel.export(); let cargo = result.cargo.unwrap();
            assert_eq!(cargo.food_requests.len(), count - 2);
            assert_eq!(cargo.holdings, vec![Holding { person: 2, food: 2 }]);
        }
        creation.sort_by(f64::total_cmp); response.sort_by(f64::total_cmp);
        println!("workers={workers} people={count} requests={} rounds={repeats} create_tick_median={:.6} response_tick_median={:.6}", count-1, creation[creation.len()/2], response[response.len()/2]);
    });
    Ok(())
}
