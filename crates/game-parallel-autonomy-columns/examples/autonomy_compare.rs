use game_parallel_autonomy::{Actor, Autonomy, Path, Rules, Situation};
use game_parallel_autonomy_columns::{Runtime, SituationView};
use game_parallel_knowledge::Record;
use rayon::prelude::*;
use std::time::Instant;
struct View(Vec<Situation>);
impl SituationView for View {
    fn read(&self, id: u64) -> Situation {
        self.0[id as usize - 1]
    }
}
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let workers: usize = args[0].parse().unwrap();
    let count: u64 = args[1].parse().unwrap();
    let ticks: u64 = args[2].parse().unwrap();
    let routed = args.get(3).is_some_and(|s| s == "routed");
    let initial = Autonomy {
        rules: Rules {
            review_interval: 2,
            min_health: 1,
            min_hunger: 1,
            min_destination_food: 1,
            min_food_gain: 1,
            max_observation_age: 100,
        },
        actors: (1..=count)
            .map(|person| Actor {
                person,
                enabled: true,
                next_review: 0,
                travel_caution: 0,
                hunger_urgency: 0,
            })
            .collect(),
        events: vec![],
    };
    let view = View(
        (1..=count)
            .map(|person| Situation {
                person,
                region: Some(1),
                health: 100,
                hunger: 4,
                busy: false,
            })
            .collect(),
    );
    let people: Vec<_> = view
        .0
        .iter()
        .map(|s| (s.person, s.region, s.health, s.hunger))
        .collect();
    let records = [
        Record {
            region: 1,
            food: 0,
            observed_at: 0,
            witness: 1,
            learned_from: None,
        },
        Record {
            region: 2,
            food: 100,
            observed_at: 0,
            witness: 1,
            learned_from: None,
        },
    ];
    let paths = if routed {
        vec![Path {
            source: 1,
            destination: 2,
            ticks: 2,
        }]
    } else {
        vec![]
    };
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap();
    pool.install(|| {
        let mut expected=initial.clone();let start=Instant::now();
        for now in 0..ticks {
            // Include the existing parallel compatibility-snapshot construction.
            let situations:Vec<_>=people.par_iter().map(|&(person,region,health,hunger)|Situation {person,region,health,hunger,busy:false}).collect();
            let draft=expected.prepare_records_validated(&situations,&paths,|_|Some(&records),now).unwrap();
            let available=vec![true;draft.decisions.len()];
            expected=game_parallel_autonomy::adapter::finish(draft,&available,&Ok(()),now+1);
        }
        let reference=start.elapsed();
        let mut runtime=Runtime::compile_validated(&initial,&paths,0).unwrap();let start=Instant::now();
        for now in 0..ticks {
            let draft=runtime.prepare(&view,|_|Some(&records),now).unwrap();let available=vec![true;draft.decisions.len()];
            runtime.commit(draft,&available,&Ok(())).unwrap();
        }
        let persistent=start.elapsed();assert_eq!(runtime.export(),expected);
        println!("workers={workers} people={count} ticks={ticks} routed={routed} reference_ms={:.3} persistent_ms={:.3}",reference.as_secs_f64()*1000.,persistent.as_secs_f64()*1000.);
    });
}
