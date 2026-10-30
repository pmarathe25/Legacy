use game_parallel_social::{Encounter, Relationship, Rules};
use game_parallel_social_edges::Runtime;
use std::time::Instant;
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let workers: usize = args[0].parse().unwrap();
    let people: u64 = args[1].parse().unwrap();
    let ticks: u64 = args[2].parse().unwrap();
    let half = people / 2;
    let historical: Vec<_> = (0..8)
        .flat_map(|round| {
            (0..half).map(move |i| Relationship {
                a: i + 1,
                b: half + (i + round) % half + 1,
                last_met: 0,
                encounters: 1,
                familiarity: 1,
            })
        })
        .collect();
    let phases: Vec<Vec<_>> = (1..=ticks)
        .map(|tick| {
            (0..half)
                .map(|i| Encounter {
                    tick,
                    region: 1,
                    a: i + 1,
                    b: half + (i + tick) % half + 1,
                    pressured: None,
                })
                .collect()
        })
        .collect();
    let rules = Rules {
        interval: 1,
        min_health: 1,
        max_hunger: 10,
        familiarity_gain: 2,
        familiarity_max: 100,
        relationship_retention: Some(4),
    };
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap();
    pool.install(|| {
        let mut runtime=Runtime::compile_validated(&historical,rules,0).unwrap();
        let start=Instant::now();
        for (i,events) in phases.iter().enumerate() {
            let draft=runtime.prepare_validated(events,i as u64+1).unwrap();
            runtime.commit(draft).unwrap();
        }
        println!("workers={workers} people={people} ticks={ticks} edge_only_ms={:.3} live={} slots={} expiry={}",start.elapsed().as_secs_f64()*1000.,runtime.live_len(),runtime.allocated_slots(),runtime.expiry_len());
        assert!(runtime.expiry_len()<=runtime.live_len());
        std::hint::black_box(runtime.export());
    });
}
