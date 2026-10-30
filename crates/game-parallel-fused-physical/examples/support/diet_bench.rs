//! Measure independent physical eating and the subsequent digestion boundary.
use super::*;

pub fn run(
    workers: usize,
    count: usize,
    repeats: usize,
    nutrition: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if workers == 0 || count == 0 || repeats == 0 {
        return Err("invalid benchmark arguments".into());
    }
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/store_building.toml"
    ))?;
    world.market = None;
    world.autowork = None;
    if nutrition {
        world.nutrition = Some(game_parallel::Nutrition {
            rules: game_parallel::NutritionRules {
                interval: 5,
                capacity: [200; 6],
                initial: [80; 6],
                metabolism: [2; 6],
                minimum: [10; 6],
                deficiency_damage: 2,
                scalar_meal: [4; 6],
            },
            people: Vec::new(),
        });
    }
    world.regions[0].people = (1..=count as u64)
        .map(|id| Person {
            id,
            hunger: 5,
            health: 100,
        })
        .collect();
    world.regions[1].people.clear();
    world.cargo.as_mut().unwrap().acquire_when_hungry.clear();
    world
        .production
        .as_mut()
        .unwrap()
        .goods
        .push(game_parallel::Good {
            id: 9,
            name: "meal".into(),
            food: Some(game_parallel::FoodProfile {
                portion: 2,
                hunger_relief: 7,
                nutrients: if nutrition {
                    [("protein".to_string(), 7), ("energy".to_string(), 10)]
                        .into_iter()
                        .collect()
                } else {
                    Default::default()
                },
                health_effect: -1,
                shelf_days: None,
            }),
        });
    world.goods_cargo.as_mut().unwrap().holdings = (1..=count as u64)
        .map(|person| game_parallel::GoodsHolding {
            person,
            items: vec![game_parallel::Amount {
                good: 9,
                quantity: 2,
            }]
            .into(),
        })
        .collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    pool.install(|| {
        let mut samples = Vec::new();
        let mut phases: [Vec<f64>; 4] = Default::default();
        for round in 0..=repeats {
            let mut kernel = Kernel::compile(world.clone()).unwrap();
            let start = Instant::now();
            let draft = kernel.prepare().unwrap();
            let p1 = start.elapsed();
            let at = Instant::now();
            kernel.commit(draft).unwrap();
            let c1 = at.elapsed();
            let at = Instant::now();
            let draft = kernel.prepare().unwrap();
            let p2 = at.elapsed();
            let at = Instant::now();
            kernel.commit(draft).unwrap();
            let c2 = at.elapsed();
            let elapsed = start.elapsed().as_secs_f64();
            if round > 0 {
                samples.push(elapsed);
                for (rows, value) in phases.iter_mut().zip([p1, c1, p2, c2]) { rows.push(value.as_secs_f64()); }
            }
            let result = kernel.export();
            assert!(result.food_effects.is_none());
            assert!(result.goods_cargo.unwrap().holdings.is_empty());
            assert!(result.regions[0].people.iter().all(|person| person.hunger == 1 && person.health == 99));
            if nutrition {
                assert!(result.nutrition.unwrap().people.iter().all(|person| person.reserves == [90, 87, 80, 80, 80, 80]));
            }
        }
        samples.sort_by(f64::total_cmp);
        for rows in &mut phases { rows.sort_by(f64::total_cmp); }
        let mid = repeats / 2;
        println!("workers={workers} people={count} eaters={count} nutrition={nutrition} rounds={repeats} two_ticks_median={:.6} prepare1={:.6} commit1={:.6} prepare2={:.6} commit2={:.6}", samples[mid], phases[0][mid], phases[1][mid], phases[2][mid], phases[3][mid]);
    });
    Ok(())
}
