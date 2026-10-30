//! Complete goods lifecycle, with one person per job. Arguments: workers, people.
use game_parallel::{GoodsDirection, GoodsTransfer, Move, Person, StartWork, World};
use std::time::Instant;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let threads: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let count: usize = args.get(1).map_or(Ok(10_000), |v| v.parse())?;
    if threads == 0 || count == 0 {
        return Err("workers and people must be positive".into());
    }
    let mut w = World::from_toml(include_str!("../../../../content/parallel/workshop.toml"))?;
    w.rules.hunger_per_tick = 0;
    w.rules.food_growth = 0;
    w.regions[0].people = (0..count)
        .map(|id| Person {
            id: id as u64 + 100,
            health: 100,
            hunger: 0,
        })
        .collect();
    let production = w.production.as_mut().unwrap();
    production.inventories[0].stocks[0].capacity = count as u64 * 3;
    production.inventories[0].stocks[0].quantity = count as u64 * 3;
    production.inventories[0].stocks[1].capacity = count as u64 * 2;
    production.inventories[1].stocks[1].capacity = count as u64 * 2;
    let jobs: Vec<_> = (0..count)
        .map(|id| StartWork {
            id: id as u64,
            worker: id as u64 + 100,
            region: 1,
            recipe: 1,
            units: 1,
        })
        .collect();
    let transfer = |direction| {
        (0..count)
            .map(|id| GoodsTransfer {
                person: id as u64 + 100,
                good: 2,
                quantity: 2,
                direction,
            })
            .collect::<Vec<_>>()
    };
    let pickups = transfer(GoodsDirection::Pickup);
    let deposits = transfer(GoodsDirection::Deposit);
    let moves: Vec<_> = (0..count)
        .map(|id| Move {
            person: id as u64 + 100,
            destination: 2,
        })
        .collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()?;
    let mut phases = [std::time::Duration::ZERO; 6];
    let start = Instant::now();
    pool.install(|| -> Result<(), String> {
        let mark = Instant::now();
        w.start_work(&jobs)?;
        phases[0] = mark.elapsed();
        let mark = Instant::now();
        w.step()?;
        w.step()?;
        phases[1] = mark.elapsed();
        let mark = Instant::now();
        w.transfer_goods(&pickups)?;
        phases[2] = mark.elapsed();
        let mark = Instant::now();
        w.move_people(&moves)?;
        phases[3] = mark.elapsed();
        let mark = Instant::now();
        w.step()?;
        w.step()?;
        phases[4] = mark.elapsed();
        let mark = Instant::now();
        w.transfer_goods(&deposits)?;
        phases[5] = mark.elapsed();
        Ok(())
    })?;
    let elapsed = start.elapsed();
    if w.production.as_ref().unwrap().inventories[1].stocks[1].quantity != count as u64 * 2
        || !w.goods_cargo.as_ref().unwrap().holdings.is_empty()
    {
        return Err("lifecycle conservation failed".into());
    }
    w.validate()?;
    println!(
        "workers={threads} people={count} ticks={} elapsed_seconds={:.6} start={:.6} work={:.6} pickup={:.6} depart={:.6} travel={:.6} deposit={:.6}",
        w.tick,
        elapsed.as_secs_f64(),
        phases[0].as_secs_f64(),
        phases[1].as_secs_f64(),
        phases[2].as_secs_f64(),
        phases[3].as_secs_f64(),
        phases[4].as_secs_f64(),
        phases[5].as_secs_f64()
    );
    Ok(())
}
