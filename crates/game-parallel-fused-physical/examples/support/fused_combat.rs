//! Concentrated simultaneous conflict. Args: workers, people, [hot].
use game_parallel::{CombatEncounter, CombatFighter, CombatOrder, Conflict, Person, World};
use game_parallel_fused_physical::Kernel;
use std::time::Instant;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let workers: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let count: usize = args.get(1).map_or(Ok(100000), |v| v.parse())?;
    let hot = args.get(2).is_some_and(|v| v == "hot");
    if workers == 0 || count < 2 || !count.is_multiple_of(2) {
        return Err("workers positive and people positive even".into());
    }
    let mut world = World::from_toml(include_str!("../../../../content/parallel/example.toml"))?;
    world.regions[0].people = (0..count)
        .map(|id| Person {
            id: id as u64 + 1,
            health: 100,
            hunger: 0,
        })
        .collect();
    world.regions[1].people.clear();
    world.rules.hunger_per_tick = 0;
    world.rules.recovery = 0;
    let fighters = (0..count)
        .map(|i| {
            let side = if i < count / 2 { 1 } else { 2 };
            let target = if side == 1 {
                if hot {
                    count / 2 + 1
                } else {
                    i + count / 2 + 1
                }
            } else {
                if hot { 1 } else { i - count / 2 + 1 }
            };
            CombatFighter {
                person: i as u64 + 1,
                side,
                order: CombatOrder::Attack {
                    target: target as u64,
                },
            }
        })
        .collect();
    world.conflict = Some(Conflict {
        damage: 1,
        next_id: 2,
        encounters: vec![CombatEncounter {
            id: 1,
            region: world.regions[0].id,
            fighters,
        }],
        events: vec![],
    });
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    let mut kernel = Kernel::compile(world)?;
    let start = Instant::now();
    let (prepare, commit) = pool.install(|| {
        let draft = kernel.prepare()?;
        let prepare = start.elapsed();
        kernel.commit(draft)?;
        Ok::<_, &'static str>((prepare, start.elapsed() - prepare))
    })?;
    let elapsed = start.elapsed();
    let result = kernel.export();
    assert_eq!(result.conflict.as_ref().unwrap().events.len(), count);
    println!(
        "workers={workers} people={count} hot={hot} elapsed_seconds={:.6} prepare_seconds={:.6} commit_seconds={:.6}",
        elapsed.as_secs_f64(),
        prepare.as_secs_f64(),
        commit.as_secs_f64()
    );
    Ok(())
}
