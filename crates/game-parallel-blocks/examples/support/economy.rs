//! Complete production, pickup, travel and delivery. Arguments: workers, people, reference|blocks.
use game_parallel::{GoodsDirection, GoodsTransfer, Move, Person, StartWork, World};
use game_parallel_blocks::Engine;
use std::time::Instant;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let workers: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let count: usize = args.get(1).map_or(Ok(100_000), |v| v.parse())?;
    let reference = args.get(2).is_some_and(|v| v == "reference");
    if workers == 0 || count == 0 {
        return Err("invalid arguments".into());
    }
    let mut w = World::from_toml(include_str!("../../../../content/parallel/workshop.toml"))?;
    w.market = None;
    w.regions[1].people.clear();
    w.regions[0].people = (0..count)
        .map(|id| Person {
            id: id as u64,
            health: 100,
            hunger: 0,
        })
        .collect();
    w.rules.hunger_per_tick = 0;
    w.rules.food_growth = 0;
    let p = w.production.as_mut().unwrap();
    p.inventories[0].stocks[0].quantity = count as u64 * 3;
    p.inventories[0].stocks[0].capacity = count as u64 * 3;
    p.inventories[0].stocks[1].capacity = count as u64 * 2;
    p.inventories[1].stocks[1].capacity = count as u64 * 2;
    let jobs: Vec<_> = (0..count)
        .map(|id| StartWork {
            id: id as u64,
            worker: id as u64,
            region: 1,
            recipe: 1,
            units: 1,
        })
        .collect();
    let transfer = |direction| {
        (0..count)
            .map(|id| GoodsTransfer {
                person: id as u64,
                good: 2,
                quantity: 2,
                direction,
            })
            .collect::<Vec<_>>()
    };
    let pickup = transfer(GoodsDirection::Pickup);
    let deposit = transfer(GoodsDirection::Deposit);
    let moves: Vec<_> = (0..count)
        .map(|id| Move {
            person: id as u64,
            destination: 2,
        })
        .collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    let mut e = if reference {
        None
    } else {
        Some(Engine::compile(w.clone())?)
    };
    let mut phases = [std::time::Duration::ZERO; 6];
    let start = Instant::now();
    pool.install(|| -> Result<(), String> {
        if let Some(e) = &mut e {
            let at = Instant::now();
            e.start_work(&jobs)?;
            phases[0] = at.elapsed();
            let at = Instant::now();
            e.step()?;
            e.step()?;
            phases[1] = at.elapsed();
            let at = Instant::now();
            e.transfer_goods(&pickup)?;
            phases[2] = at.elapsed();
            let at = Instant::now();
            e.move_people(&moves)?;
            phases[3] = at.elapsed();
            let at = Instant::now();
            e.step()?;
            e.step()?;
            phases[4] = at.elapsed();
            let at = Instant::now();
            e.transfer_goods(&deposit)?;
            phases[5] = at.elapsed();
        } else {
            let at = Instant::now();
            w.start_work(&jobs)?;
            phases[0] = at.elapsed();
            let at = Instant::now();
            w.step()?;
            w.step()?;
            phases[1] = at.elapsed();
            let at = Instant::now();
            w.transfer_goods(&pickup)?;
            phases[2] = at.elapsed();
            let at = Instant::now();
            w.move_people(&moves)?;
            phases[3] = at.elapsed();
            let at = Instant::now();
            w.step()?;
            w.step()?;
            phases[4] = at.elapsed();
            let at = Instant::now();
            w.transfer_goods(&deposit)?;
            phases[5] = at.elapsed();
        }
        Ok(())
    })?;
    let elapsed = start.elapsed();
    if let Some(e) = e {
        w = e.export();
    }
    if w.production.as_ref().unwrap().inventories[1].stocks[1].quantity != count as u64 * 2
        || !w.goods_cargo.as_ref().unwrap().holdings.is_empty()
    {
        return Err("goods conservation failed".into());
    }
    println!(
        "workers={workers} people={count} reference={reference} elapsed_seconds={:.6} start={:.6} work={:.6} pickup={:.6} depart={:.6} travel={:.6} deposit={:.6}",
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
