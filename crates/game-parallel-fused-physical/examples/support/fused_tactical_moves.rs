//! Args: workers, even population, move count, repetitions. Includes full next round.
use game_parallel::*;
use game_parallel_fused_physical::Kernel;
use std::time::Instant;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let workers: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let count: usize = args.get(1).map_or(Ok(1_000_000), |v| v.parse())?;
    let amount: usize = args.get(2).map_or(Ok(1), |v| v.parse())?;
    let repeats: usize = args.get(3).map_or(Ok(5), |v| v.parse())?;
    assert!(count >= 2 && count.is_multiple_of(2) && amount <= count);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    let mut world = World::from_toml(include_str!("../../../../content/parallel/tactics.toml"))?;
    world.rules.max_health = u32::MAX;
    world.rules.hunger_per_tick = 0;
    world.rules.recovery = 0;
    world.regions[0].people = (1..=count as u64)
        .map(|id| Person {
            id,
            health: u32::MAX,
            hunger: 0,
        })
        .collect();
    world.regions[1].people.clear();
    let conflict = world.conflict.as_mut().unwrap();
    conflict.next_id = 2;
    conflict.encounters = vec![CombatEncounter {
        id: 1,
        region: 1,
        fighters: (0..count)
            .map(|i| CombatFighter {
                person: i as u64 + 1,
                side: (i % 2) as u32,
                order: CombatOrder::Attack {
                    target: (i ^ 1) as u64 + 1,
                },
            })
            .collect(),
    }];
    let tactics = world.tactics.as_mut().unwrap();
    tactics.maps[0].rules.width = count as u32 + 4;
    tactics.maps[0].rules.attack_range = 3;
    tactics.battles = vec![TacticalBattle {
        encounter: 1,
        region: 1,
        people: (0..count)
            .map(|i| TacticalPlacement {
                person: i as u64 + 1,
                position: TacticalPoint { x: i as u32, y: 1 },
                last_move: None,
            })
            .collect(),
    }];
    pool.install(|| -> Result<(), String> {
        let mut kernel = Kernel::compile(world)?;
        let begin = Instant::now();
        kernel.step().map_err(str::to_owned)?;
        let warmup = begin.elapsed().as_secs_f64() * 1000.;
        let mut commands = Vec::new();
        let mut complete = Vec::new();
        for tick in 0..repeats {
            let moves: Vec<_> = (0..amount).map(|i| TacticalMove { encounter: 1, person: i as u64 + 1, destination: TacticalPoint { x: i as u32, y: 1 + ((tick + 1) % 2) as u32 } }).collect();
            let begin = Instant::now();
            kernel.tactical_moves(&moves)?;
            commands.push(begin.elapsed().as_secs_f64() * 1000.);
            kernel.step().map_err(str::to_owned)?;
            complete.push(begin.elapsed().as_secs_f64() * 1000.);
        }
        commands.sort_by(f64::total_cmp);
        complete.sort_by(f64::total_cmp);
        println!("workers={workers} people={count} moves={amount} warmup_ms={warmup:.3} command_ms={:.3} command_round_ms={:.3}", commands[repeats / 2], complete[repeats / 2]);
        Ok(())
    })?;
    Ok(())
}
