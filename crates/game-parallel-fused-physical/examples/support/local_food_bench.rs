//! Concentrated carried meals alongside scarce regional food.
use super::*;
use game_parallel::Holding;

pub fn run(workers: usize, count: usize, repeats: usize) -> Result<(), Box<dyn std::error::Error>> {
    if workers == 0
        || count < 4
        || !count.is_multiple_of(4)
        || count > (u32::MAX / 2) as usize
        || repeats == 0
    {
        return Err("invalid benchmark arguments".into());
    }
    let mut world = World::from_toml(include_str!("../../../../content/parallel/local_food.toml"))?;
    world.rules.food_capacity = (count * 2) as u32;
    world.regions[0].food = (count / 2) as u32;
    world.regions[0].people = (1..=count as u64)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 8,
        })
        .collect();
    world.regions[1].people.clear();
    world.cargo.as_mut().unwrap().holdings = (1..=count as u64)
        .filter(|id| id.is_multiple_of(2))
        .map(|person| Holding { person, food: 2 })
        .collect();
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
            assert_eq!(result.regions[0].food, 0);
            assert!(result.cargo.unwrap().holdings.is_empty());
            assert_eq!(result.regions[0].people.iter().filter(|person| person.hunger == 5).count(), count * 3 / 4);
            assert_eq!(result.regions[0].people.iter().filter(|person| person.hunger == 9).count(), count / 4);
        }
        samples.sort_by(f64::total_cmp);
        println!("workers={workers} people={count} carried={} regional={} rounds={repeats} tick_median={:.6}", count/2, count/4, samples[samples.len()/2]);
    });
    Ok(())
}
