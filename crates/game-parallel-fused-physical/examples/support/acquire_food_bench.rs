//! Measure competing individual attempts at one physical source.
use super::*;

pub fn run(
    workers: usize,
    count: usize,
    repeats: usize,
    stored: bool,
    observed: bool,
    single_observer: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if workers == 0
        || count < 2
        || !count.is_multiple_of(2)
        || count > (u32::MAX / 2) as usize
        || repeats == 0
    {
        return Err("invalid benchmark arguments".into());
    }
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/personal_food.toml"
    ))?;
    world.rules.food_capacity = (count * 2) as u32;
    world.regions[0].food = count as u32;
    world.regions[0].people = (1..=count as u64)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 3,
        })
        .collect();
    world.regions[1].people.clear();
    if observed {
        world.opinions =
            World::from_toml(include_str!("../../../../content/parallel/food_aid.toml"))?.opinions;
        let opinions = world.opinions.as_mut().unwrap();
        opinions.records.clear();
        opinions.rules.theft_loss = 1;
    }
    let cargo = world.cargo.as_mut().unwrap();
    cargo.holdings.clear();
    cargo.acquire_when_hungry = (1..=count as u64).collect();
    if stored {
        world.regions[0].food = 0;
        cargo.food_stores = vec![game_parallel::FoodStore {
            id: 1,
            region: 1,
            food: count as u32,
            capacity: count as u32,
            claimant: None,
            object_to_taking: false,
        }];
        if observed {
            cargo.acquire_when_hungry = (count as u64 / 2 + 1..=count as u64).collect();
            cargo.food_store_claims = (1..=if single_observer { 1 } else { count as u64 / 2 })
                .map(|person| game_parallel::FoodStoreClaim {
                    person,
                    store: 1,
                    object_to_taking: true,
                })
                .collect();
        }
        cargo.food_store_choices = (if observed { count as u64 / 2 + 1 } else { 1 }..=count as u64)
            .map(|person| game_parallel::FoodStoreChoice {
                person,
                store: 1,
                reconsider_on_failure: false,
            })
            .collect();
    }
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
            if round > 0 {
                samples.push(elapsed);
            }
            let result = kernel.export();
            assert_eq!(result.regions[0].food, 0);
            if stored { assert_eq!(result.cargo.as_ref().unwrap().food_stores[0].food, 0); }
            assert_eq!(result.cargo.unwrap().holdings.len(), count / 2);
            if observed {
                let n = result.opinions.as_ref().unwrap().records.len();
                assert_eq!(n, if single_observer { 32 } else { count * 2 });
            }
            assert!(
                result.regions[0]
                    .people
                    .iter()
                    .all(|person| person.hunger == 4)
            );
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "workers={workers} people={count} stored={stored} observed={observed} single_observer={single_observer} acquired={} rounds={repeats} tick_median={:.6}",
            count / 2,
            samples[samples.len() / 2]
        );
    });
    Ok(())
}
