//! Whole-tick decision and departure benchmark, with remembered remote stock.
use game_parallel::{AutonomousActor, KnowledgeRecord, Mind, Person, World};
use std::time::Duration;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let workers: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let people: usize = args.get(1).map_or(Ok(10_000), |v| v.parse())?;
    let repetitions: usize = args.get(2).map_or(Ok(10), |v| v.parse())?;
    if workers == 0 || repetitions == 0 {
        return Err("invalid arguments".into());
    }
    let mut base = World::from_toml(include_str!("../../../../content/parallel/autonomy.toml"))?;
    base.social = None;
    base.rules.food_growth = 0;
    base.regions[0].people = (0..people)
        .rev()
        .map(|id| Person {
            id: id as u64,
            hunger: 1,
            health: 100,
        })
        .collect();
    base.regions[1].people.clear();
    base.knowledge.as_mut().unwrap().minds = (0..people)
        .map(|id| Mind {
            person: id as u64,
            records: vec![
                KnowledgeRecord {
                    region: 1,
                    food: 0,
                    observed_at: 0,
                    witness: id as u64,
                    learned_from: None,
                },
                KnowledgeRecord {
                    region: 2,
                    food: 100,
                    observed_at: 0,
                    witness: id as u64,
                    learned_from: None,
                },
            ],
        })
        .collect();
    base.autonomy.as_mut().unwrap().actors = (0..people)
        .map(|id| AutonomousActor {
            person: id as u64,
            enabled: true,
            next_review: 0,
            travel_caution: 0,
            hunger_urgency: 0,
        })
        .collect();
    base.validate()?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    let mut samples = Vec::new();
    for _ in 0..repetitions {
        let mut w = base.clone();
        let start = std::time::Instant::now();
        pool.install(|| w.step())?;
        samples.push(start.elapsed());
        assert_eq!(w.travelers.len(), people);
        assert_eq!(w.autonomy.as_ref().unwrap().events.len(), people);
        std::hint::black_box(w);
    }
    samples.sort_unstable();
    let total: Duration = samples.iter().sum();
    println!(
        "workers={workers} people={people} repetitions={repetitions} median_seconds={:.6} mean_seconds={:.6}",
        samples[repetitions / 2].as_secs_f64(),
        total.as_secs_f64() / repetitions as f64
    );
    Ok(())
}
