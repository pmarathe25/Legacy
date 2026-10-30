//! Measure independent and concentrated personal food help.
use super::*;
use game_parallel::{FoodAidPolicy, FoodSharingPolicy, Holding, Opinion};

pub fn run(
    workers: usize,
    count: usize,
    repeats: usize,
    hot: bool,
    learned: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if workers == 0
        || count < 4
        || !count.is_multiple_of(2)
        || count > (u32::MAX / 2) as usize
        || repeats == 0
    {
        return Err("invalid benchmark arguments".into());
    }
    let half = count / 2;
    let mut world = World::from_toml(include_str!("../../../../content/parallel/food_aid.toml"))?;
    world.rules.food_capacity = (count * 2) as u32;
    world.regions[0].food = 0;
    world.regions[0].people = (1..=count as u64)
        .map(|id| Person {
            id,
            health: 100,
            hunger: if id > half as u64 { 3 } else { 0 },
        })
        .collect();
    world.regions[1].people.clear();
    if learned {
        let opinions = world.opinions.as_mut().unwrap();
        opinions.rules.fear_decay_per_tick = 0;
        opinions.rules.gratitude_gain = 0;
        opinions.records = (1..=half as u64)
            .map(|giver| Opinion {
                observer: giver,
                target: if hot {
                    half as u64 + 1
                } else {
                    half as u64 + giver
                },
                trust: 0,
                fear: 1,
                obligation: 0,
                affinity: 0,
            })
            .collect();
    } else {
        world.opinions = None;
    }
    let cargo = world.cargo.as_mut().unwrap();
    cargo.acquire_when_hungry.clear();
    cargo.holdings = (1..=half as u64)
        .map(|person| Holding { person, food: 2 })
        .collect();
    cargo.food_sharing = if learned {
        (1..=half as u64)
            .map(|person| FoodSharingPolicy {
                person,
                amount: 2,
                retain: 0,
                threshold: 1,
                interval: 1,
                enabled: true,
            })
            .collect()
    } else {
        Vec::new()
    };
    cargo.food_aid = if learned {
        Vec::new()
    } else {
        (1..=half as u64)
            .map(|giver| FoodAidPolicy {
                giver,
                recipient: if hot {
                    half as u64 + 1
                } else {
                    half as u64 + giver
                },
                amount: 2,
                retain: 0,
                help_value: 1,
                threshold: 1,
                interval: 1,
                requires_request: false,
                enabled: true,
            })
            .collect()
    };
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    pool.install(|| {
        let mut samples = Vec::new();
        for round in 0..=repeats {
            let mut kernel = Kernel::compile(world.clone()).unwrap();
            let start = Instant::now();
            kernel.step().unwrap();
            let elapsed = start.elapsed().as_secs_f64();
            if round > 0 { samples.push(elapsed); }
            let result = kernel.export();
            let holdings = result.cargo.unwrap().holdings;
            assert_eq!(holdings.iter().map(|h| u64::from(h.food)).sum::<u64>(), (half * 2) as u64);
            assert_eq!(holdings.iter().filter(|h| h.person > half as u64).count(), if hot { 1 } else { half });
        }
        samples.sort_by(f64::total_cmp);
        println!("workers={workers} people={count} proposals={half} hot={hot} learned={learned} rounds={repeats} tick_median={:.6}", samples[samples.len()/2]);
    });
    Ok(())
}
