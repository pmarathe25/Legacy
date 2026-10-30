use game_parallel_knowledge::{Knowledge, Mind, Observation, Record};
use game_parallel_knowledge_columns::{ObservationView, Runtime};
use game_parallel_social::Encounter;
use std::{collections::HashSet, time::Instant};
struct View(Vec<Observation>);
impl ObservationView for View {
    fn read(&self, id: u64) -> Observation {
        self.0[id as usize - 1]
    }
}
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let workers: usize = args[0].parse().unwrap();
    let count: usize = args[1].parse().unwrap();
    let ticks: u64 = args[2].parse().unwrap();
    let initial = Knowledge {
        max_records: 4,
        minds: (1..=count as u64)
            .map(|person| Mind {
                person,
                records: (2..=5)
                    .map(|region| Record {
                        region,
                        food: person as u32 % 99,
                        observed_at: 0,
                        witness: person,
                        learned_from: None,
                    })
                    .collect(),
            })
            .collect(),
    };
    let view = View(
        (1..=count as u64)
            .map(|person| Observation {
                person,
                health: 100,
                local: Some((1, 100)),
            })
            .collect(),
    );
    let regions: HashSet<_> = (1..=5).collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap();
    pool.install(|| {
        let mut expected = initial.clone();
        let start = Instant::now();
        for now in 0..ticks {
            let encounters: Vec<_> = (1..count as u64)
                .step_by(2)
                .map(|a| Encounter {
                    tick: now + 1,
                    region: 1,
                    a,
                    b: a + 1,
                    pressured: None,
                })
                .collect();
            expected = expected
                .prepare_validated(&view.0, &regions, &encounters, now)
                .unwrap();
        }
        let reference = start.elapsed();
        let mut runtime = Runtime::compile_validated(&initial, 0).unwrap();
        let start = Instant::now();
        for now in 0..ticks {
            let encounters: Vec<_> = (1..count as u64)
                .step_by(2)
                .map(|a| Encounter {
                    tick: now + 1,
                    region: 1,
                    a,
                    b: a + 1,
                    pressured: None,
                })
                .collect();
            let draft = runtime.prepare_validated(&view, &encounters, now).unwrap();
            runtime.commit(draft).unwrap();
        }
        let columns = start.elapsed();
        assert_eq!(runtime.export(), expected);
        println!(
            "workers={workers} people={count} ticks={ticks} reference_ms={:.3} persistent_ms={:.3}",
            reference.as_secs_f64() * 1000.,
            columns.as_secs_f64() * 1000.
        );
    });
}
