//! Concentrated autonomous approaches and individual pressure responses.
use super::*;

pub fn run(workers: usize, count: usize, repeats: usize) -> Result<(), Box<dyn std::error::Error>> {
    if workers == 0
        || count < 32
        || !count.is_multiple_of(2)
        || (!count.is_multiple_of(4096) && count % 4096 < 32)
        || repeats == 0
    {
        return Err("invalid benchmark arguments".into());
    }
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/social_needs.toml"
    ))?;
    world.rules.hunger_per_tick = 0;
    world.regions[0].people = (1..=count as u64)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    world.regions[1].people.clear();
    let social = world.social.as_mut().unwrap();
    social.needs = None;
    social.participants = (1..=count as u64)
        .map(|person| game_parallel::SocialParticipant {
            person,
            willing: !person.is_multiple_of(2),
        })
        .collect();
    social.pressures = (1..=count as u64)
        .filter(|id| id.is_multiple_of(2))
        .map(|person| game_parallel::SocialPressurePolicy {
            person,
            fear_at_least: Some(2),
            obligation_at_least: None,
            enabled: true,
        })
        .collect();
    let mut opinions =
        World::from_toml(include_str!("../../../../content/parallel/campaign.toml"))?
            .opinions
            .unwrap();
    opinions.rules.fear_decay_per_tick = 0;
    opinions.rules.encounter_gain = 0;
    opinions.records = (1..=count as u64)
        .filter(|id| id.is_multiple_of(2))
        .flat_map(|id| {
            let first = ((id - 1) / 4096) * 4096 + 1;
            let len = (count as u64 - first + 1).min(4096);
            (-15i64..=15)
                .filter(|offset| offset & 1 != 0)
                .map(move |offset| {
                    first + ((id - first) as i64 + offset).rem_euclid(len as i64) as u64
                })
                .map(move |target| game_parallel::Opinion {
                    observer: id,
                    target,
                    trust: 0,
                    fear: 2,
                    obligation: 0,
                    affinity: 0,
                })
        })
        .collect();
    opinions
        .records
        .sort_unstable_by_key(|row| (row.observer, row.target));
    world.opinions = Some(opinions);
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
            assert!(
                result
                    .social
                    .unwrap()
                    .events
                    .iter()
                    .filter(|event| event.pressured.is_some())
                    .count()
                    > count / 6
            );
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "workers={workers} people={count} rounds={repeats} tick_median={:.6}",
            samples[repeats / 2]
        );
    });
    Ok(())
}
