//! workers, children, hot|spread, repetitions. Timing excludes import.
use game_parallel_dependent_care::{DependentCare, Policy, Rules};
use game_parallel_family_columns::Store;
use game_parallel_family_model::Member;
use game_parallel_genetics::{Genome, Sex};
use std::time::Instant;
fn member(id: u64, age: u64, parents: Vec<u64>) -> Member {
    Member {
        id,
        born: None,
        age,
        lifespan_loss: 0,
        genome: Genome {
            sex: Sex::Female,
            alleles: [[0; 2]; 4],
        },
        parents: parents.into(),
        partner: None,
        pregnancy: None,
        next_conception: 0,
        alive: true,
        reproduction_allowed: false,
        reconsider_partnership: false,
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let workers: usize = args.first().map_or(Ok(1), |s| s.parse())?;
    let n: usize = args.get(1).map_or(Ok(100_000), |s| s.parse())?;
    let hot = args.get(2).is_some_and(|s| s == "hot");
    let repeats: usize = args.get(3).map_or(Ok(4), |s| s.parse())?;
    if workers == 0 || n == 0 || repeats == 0 {
        return Err("invalid arguments".into());
    }
    let parents = if hot { 2 } else { n * 2 };
    let mut family: Vec<_> = (1..=parents as u64)
        .map(|id| member(id, 20, vec![]))
        .collect();
    family.extend((0..n).map(|i| {
        member(
            (parents + i + 1) as u64,
            1,
            if hot {
                vec![1, 2]
            } else {
                vec![(i * 2 + 1) as u64, (i * 2 + 2) as u64]
            },
        )
    }));
    let family = Store::compile(&family)?;
    let people: Vec<_> = (1..=(parents + n) as u64)
        .map(|id| (id, Some(1), 100, 0))
        .collect();
    let care = DependentCare {
        inherit_births: false,
        rules: Rules {
            dependent_until: 5,
            max_caregiver_hunger: 10,
            neglect_damage: 2,
            feed_from_cargo: false,
            fetch_local_food: false,
            pressure_threshold: None,
        },
        policies: (1..=parents as u64)
            .map(|person| Policy {
                person,
                willing: true,
            })
            .collect(),
        events: vec![],
    };
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    pool.install(|| {
        let mut times = vec![];
        for round in 0..=repeats {
            let start = Instant::now();
            let draft = care.prepare(&family, 12, &people, &[], (42, 1), (|_, _| true, |_, _| 0));
            let elapsed = start.elapsed().as_secs_f64();
            assert_eq!(draft.events.len(), n);
            assert_eq!(draft.actors.len(), if hot { 2.min(n) } else { n });
            if round > 0 {
                times.push(elapsed);
            }
        }
        times.sort_by(f64::total_cmp);
        println!(
            "workers={workers} children={n} hot={hot} median_seconds={:.6}",
            times[times.len() / 2]
        );
    });
    Ok(())
}
