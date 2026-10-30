//! End-to-end family-enabled benchmark; arguments: workers regions people ticks [reference|fused] [enrollment].
use game_parallel::{Person, Region, Sex, World};
use game_parallel_fused_physical::Kernel as Engine;
use std::time::Instant;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let workers: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let region_count: usize = args.get(1).map_or(Ok(100), |v| v.parse())?;
    let count: usize = args.get(2).map_or(Ok(10_000), |v| v.parse())?;
    let ticks: usize = args.get(3).map_or(Ok(20), |v| v.parse())?;
    let reference = args.get(4).is_some_and(|v| v == "reference");
    let enrollment = args.get(5).is_some_and(|v| v == "enrollment");
    if workers == 0 || region_count == 0 || count == 0 || count > u32::MAX as usize / 4 {
        return Err("workers, regions and people must be positive, and people bounded".into());
    }
    let mut world = World::from_toml(include_str!("../../../../content/parallel/families.toml"))?;
    world.routes.clear();
    world.rules.food_capacity = (count as u32 * 4).max(100);
    world.rules.food_growth = world.rules.food_capacity;
    world.regions = (0..region_count)
        .map(|id| Region {
            id: id as u64,
            food: 0,
            people: Vec::new(),
        })
        .collect();
    let family = world.families.as_mut().unwrap();
    let female = family
        .members
        .iter()
        .find(|m| m.genome.sex == Sex::Female)
        .unwrap()
        .clone();
    let male = family
        .members
        .iter()
        .find(|m| m.genome.sex == Sex::Male)
        .unwrap()
        .clone();
    family.members.clear();
    family.rules.fertile_until = 1_000_000;
    family.rules.lifespan_min = 1_000_001;
    family.rules.lifespan_max = 1_000_001;
    family.rules.conception_one_in = 100;
    for i in 0..count {
        let id = i as u64 + 1;
        let mut m = if (i / region_count).is_multiple_of(2) {
            female.clone()
        } else {
            male.clone()
        };
        m.id = id;
        family.members.push(m);
        world.regions[i % region_count].people.push(Person {
            id,
            hunger: 0,
            health: 100,
        });
    }
    family.next_id = count as u64 + 1;
    if enrollment {
        family.rules.enroll_births = true;
        let configured =
            World::from_toml(include_str!("../../../../content/parallel/enrollment.toml"))?;
        world.social = configured.social;
        world.knowledge = configured.knowledge;
        world.autonomy = configured.autonomy;
        world.social.as_mut().unwrap().participants = (1..=count as u64)
            .map(|person| game_parallel::SocialParticipant {
                person,
                willing: person % 3 != 0,
            })
            .collect();
        world.knowledge.as_mut().unwrap().minds = (1..=count as u64)
            .map(|person| game_parallel::Mind {
                person,
                records: Vec::new(),
            })
            .collect();
        world.autonomy.as_mut().unwrap().actors = (1..=count as u64)
            .map(|person| game_parallel::AutonomousActor {
                person,
                enabled: true,
                next_review: 0,
                travel_caution: 0,
                hunger_urgency: 0,
            })
            .collect();
    }
    world.validate()?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    let mut engine = if reference {
        None
    } else {
        Some(Engine::compile(world.clone())?)
    };
    let start = Instant::now();
    pool.install(|| -> Result<(), &'static str> {
        for _ in 0..ticks {
            if let Some(engine) = &mut engine {
                engine.step()?;
            } else {
                world.step()?;
            }
        }
        Ok(())
    })?;
    let elapsed = start.elapsed();
    if let Some(engine) = engine {
        world = engine.export();
    }
    println!(
        "workers={workers} regions={region_count} people={count} ticks={ticks} reference={reference} enrollment={enrollment} elapsed_seconds={:.6} births={}",
        elapsed.as_secs_f64(),
        world.families.as_ref().unwrap().totals.births
    );
    std::hint::black_box(world);
    Ok(())
}
