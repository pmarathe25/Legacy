//! Args: workers, people (even), [hot|spread], repetitions. Compile/export are separate.
use game_parallel_conflict::{Conflict, Encounter, Fighter, Order};
use game_parallel_conflict_packed::{Damage, Plan};
use game_parallel_production_model::PersonSnapshot;
use rayon::prelude::*;
use std::time::Instant;

fn median(samples: &mut [f64]) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let workers = args.first().map_or(Ok(1), |v| v.parse::<usize>())?;
    let count = args.get(1).map_or(Ok(1_000_000), |v| v.parse::<usize>())?;
    let hot = args.get(2).is_some_and(|v| v == "hot");
    let repetitions = args.get(3).map_or(Ok(5), |v| v.parse::<usize>())?;
    if workers == 0 || count < 2 || count % 2 != 0 || repetitions == 0 {
        return Err("workers/repetitions positive; population positive and even".into());
    }
    let people: Vec<PersonSnapshot> = (1..=count as u64).map(|id| (id, Some(1), 100, 0)).collect();
    let fighters = (0..count)
        .map(|i| {
            let side = if i < count / 2 { 1 } else { 2 };
            let target = if side == 1 {
                if hot {
                    count / 2 + 1
                } else {
                    i + count / 2 + 1
                }
            } else if hot {
                1
            } else {
                i - count / 2 + 1
            };
            Fighter {
                person: i as u64 + 1,
                side,
                order: Order::Attack {
                    target: target as u64,
                },
            }
        })
        .collect();
    let conflict = Conflict {
        damage: 1,
        next_id: 2,
        encounters: vec![Encounter {
            id: 1,
            region: 1,
            fighters,
        }],
        events: vec![],
    };
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    pool.install(|| {
        let start = Instant::now();
        let plan = Plan::compile(&conflict).unwrap();
        let compile = start.elapsed().as_secs_f64();
        let mut old_times = Vec::new();
        let mut new_times = Vec::new();
        let mut export_times = Vec::new();
        for round in 0..=repetitions {
            let start = Instant::now();
            let old = game_parallel_conflict::prepare(&conflict, &people, 1);
            let effects: Vec<_> = old.events.par_chunk_by(|a, b| a.target == b.target).map(|events| Damage {
                target: events[0].target,
                amount: if events.len() > 2048 {
                    events.par_iter().map(|event| event.damage).reduce(|| 0, u32::saturating_add)
                } else {
                    events.iter().fold(0u32, |sum, event| sum.saturating_add(event.damage))
                },
            }).collect();
            let old_time = start.elapsed().as_secs_f64();
            let start = Instant::now();
            let packed = plan.prepare(&people, 1);
            let new_time = start.elapsed().as_secs_f64();
            assert_eq!(packed.damage(), effects);
            assert_eq!(packed.event_count(), count as u64);
            let start = Instant::now();
            let expanded = packed.expand();
            let export_time = start.elapsed().as_secs_f64();
            assert_eq!(expanded, old.events);
            if round > 0 {
                old_times.push(old_time); new_times.push(new_time); export_times.push(export_time);
            }
        }
        println!("workers={workers} people={count} hot={hot} compile={compile:.6} old_plan_reduce={:.6} compact_plan_reduce={:.6} compact_expand={:.6}", median(&mut old_times), median(&mut new_times), median(&mut export_times));
    });
    Ok(())
}
