//! Full fused expedition-party ticks; arguments: workers, people, ticks.
use game_parallel::{
    Mission, MissionPhase, Missions, Person, Personality, PersonalityPerson, World,
};
use game_parallel_fused_physical::Kernel;
use std::time::Instant;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let workers: usize = args.first().map_or(Ok(1), |s| s.parse())?;
    let count: u64 = args.get(1).map_or(Ok(100_000), |s| s.parse())?;
    let ticks: usize = args.get(2).map_or(Ok(8), |s| s.parse())?;
    if workers == 0 || count < 2 || !count.is_multiple_of(2) || ticks == 0 {
        return Err("workers and ticks must be positive; people must be positive and even".into());
    }
    let mut world = World::from_toml(include_str!("../../../../content/parallel/missions.toml"))?;
    world.cargo = None;
    world.rules.hunger_per_tick = 0;
    world.regions[0].people = (1..=count)
        .map(|id| Person {
            id,
            hunger: 0,
            health: 100,
        })
        .collect();
    world.regions[1].people.clear();
    world.missions = Some(Missions {
        next_id: count + 1,
        pressure_threshold: None,
        events: vec![],
        entries: (1..=count)
            .map(|id| Mission {
                id,
                person: id,
                party: Some(id - (id - 1) % 2),
                party_opt_in: true,
                party_compliant: false,
                objective: None,
                objective_met: false,
                origin: 1,
                destination: 2,
                visit_ticks: 2,
                remaining: 0,
                phase: MissionPhase::Ready,
            })
            .collect(),
    });
    world.personality = Some(Personality {
        social_urgency_per_isolation: 100,
        inherit_births: false,
        people: (1..=count)
            .map(|person| PersonalityPerson {
                person,
                sociability: 500,
                diligence: 700,
            })
            .collect(),
    });
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    let mut kernel = pool.install(|| Kernel::compile(world))?;
    let start = Instant::now();
    let (prepare, commit) = pool.install(|| -> Result<_, &'static str> {
        let mut prepare = std::time::Duration::ZERO;
        let mut commit = std::time::Duration::ZERO;
        for _ in 0..ticks {
            let at = Instant::now();
            let draft = kernel.prepare()?;
            prepare += at.elapsed();
            let at = Instant::now();
            kernel.commit(draft)?;
            commit += at.elapsed();
        }
        Ok((prepare, commit))
    })?;
    let elapsed = start.elapsed();
    let result = kernel.export();
    result.validate()?;
    println!(
        "workers={workers} people={count} parties={} ticks={ticks} seconds={:.6} prepare_seconds={:.6} commit_seconds={:.6}",
        count / 2,
        elapsed.as_secs_f64(),
        prepare.as_secs_f64(),
        commit.as_secs_f64()
    );
    Ok(())
}
