//! Full-world arrival benchmark; arguments: workers travelers repetitions.
use game_parallel::{Cargo, Holding, Person, Traveler, World};
use std::time::Duration;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let threads: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let count: usize = args.get(1).map_or(Ok(100_000), |v| v.parse())?;
    let repetitions: usize = args.get(2).map_or(Ok(20), |v| v.parse())?;
    let meals = args.get(3).is_some_and(|v| v == "meals");
    if threads == 0 || repetitions == 0 {
        return Err("workers and repetitions must be positive".into());
    }
    let mut base = World::from_toml(include_str!("../../../../content/parallel/example.toml"))?;
    base.rules.food_growth = 0;
    for region in &mut base.regions {
        region.people.clear();
    }
    base.travelers = (0..count)
        .rev()
        .map(|id| Traveler {
            person: Person {
                id: id as u64,
                hunger: if meals { 3 } else { 0 },
                health: 100,
            },
            source: 1,
            destination: 2,
            ticks_remaining: 1,
        })
        .collect();
    if meals {
        base.cargo = Some(Cargo {
            capacity: 2,
            auto_eat_travel: true,
            auto_eat_local: false,
            personal_food_access: false,
            acquire_when_hungry: vec![],
            food_aid: vec![],
            food_stores: vec![],
            food_store_claims: vec![],
            food_store_choices: vec![],
            food_depositors: vec![],
            food_store_blueprints: vec![],
            food_store_builders: vec![],
            inherit_store_building: false,
            food_sharing: vec![],
            inherit_food_sharing: false,
            food_requests: vec![],
            next_food_request: 0,
            food_askers: vec![],
            auto_seek_age: None,
            food_enrollments: vec![],
            auto_pack_travel: false,
            holdings: (0..count)
                .map(|id| Holding {
                    person: id as u64,
                    food: 2,
                })
                .collect(),
        });
    }
    base.validate()?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()?;
    let mut samples = Vec::new();
    for _ in 0..repetitions {
        let mut world = base.clone();
        let start = std::time::Instant::now();
        pool.install(|| world.step())?;
        samples.push(start.elapsed());
        assert_eq!(world.regions[1].people.len(), count);
        if meals {
            assert!(world.cargo.as_ref().unwrap().holdings.is_empty());
            assert!(
                world.regions[1]
                    .people
                    .iter()
                    .all(|person| person.hunger == 0)
            );
        }
        std::hint::black_box(world);
    }
    samples.sort_unstable();
    let median = samples[repetitions / 2];
    let total: Duration = samples.iter().sum();
    println!(
        "meals={meals} workers={threads} travelers={count} repetitions={repetitions} median_seconds={:.6} mean_seconds={:.6}",
        median.as_secs_f64(),
        total.as_secs_f64() / repetitions as f64
    );
    Ok(())
}
