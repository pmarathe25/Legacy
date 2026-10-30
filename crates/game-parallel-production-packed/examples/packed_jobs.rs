//! Compare persistent packed and original job ticks. Arguments: workers, jobs.
use game_parallel_production::{Amount, Inventory, Job, JobPhase, Production, Stock};
use game_parallel_production_packed::PackedJobs;
use std::time::Instant;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let workers: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let count: u64 = args.get(1).map_or(Ok(1_000_000), |v| v.parse())?;
    if workers == 0 || count == 0 {
        return Err("invalid arguments".into());
    }
    let people: Vec<_> = (0..count).map(|id| (id, Some(1), 100, 0)).collect();
    let jobs: Vec<_> = (0..count)
        .map(|id| Job {
            id,
            worker: id,
            region: 1,
            recipe: 1,
            remaining: 2,
            phase: JobPhase::Working,
            reserved: vec![Amount {
                good: 1,
                quantity: 3,
            }]
            .into(),
            outputs: vec![Amount {
                good: 2,
                quantity: 2,
            }]
            .into(),
        })
        .collect();
    let inventory = vec![Inventory {
        region: 1,
        stocks: vec![
            Stock {
                good: 1,
                quantity: 0,
                capacity: count * 3,
            },
            Stock {
                good: 2,
                quantity: 0,
                capacity: count * 2,
            },
        ],
    }];
    let mut original = Production {
        max_work_hunger: 10,
        goods: vec![],
        recipes: vec![],
        recipe_categories: vec![],
        inventories: inventory.clone(),
        jobs,
        events: vec![],
    };
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    let pack_start = Instant::now();
    let mut packed = pool.install(|| PackedJobs::from_jobs(&original.jobs));
    let pack_time = pack_start.elapsed();
    let mut packed_inventory = inventory;
    let start = Instant::now();
    pool.install(|| {
        for tick in 1..=2 {
            original
                .prepare_validated(people.clone(), tick)
                .commit(&mut original);
        }
    });
    let original_time = start.elapsed();
    let start = Instant::now();
    pool.install(|| {
        for tick in 1..=2 {
            packed
                .prepare(&people, &packed_inventory, 10, tick)
                .commit(&mut packed, &mut packed_inventory);
        }
    });
    let packed_time = start.elapsed();
    if !packed.is_empty()
        || original.inventories != packed_inventory
        || original.events != packed.events()
    {
        return Err("different outcomes".into());
    }
    println!(
        "workers={workers} jobs={count} pack={:.6} original={:.6} packed={:.6}",
        pack_time.as_secs_f64(),
        original_time.as_secs_f64(),
        packed_time.as_secs_f64()
    );
    Ok(())
}
