//! Automatic decisions for independent local pairs. Arguments: workers, pairs.
use game_parallel::{Amount, GoodsHolding, Person, TradePolicy, TradeProposal, TradeValue, World};
use std::time::Instant;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let workers: usize = args.first().map_or(Ok(1), |value| value.parse())?;
    let count: usize = args.get(1).map_or(Ok(50_000), |value| value.parse())?;
    let contended = args.get(2).is_some_and(|arg| arg == "contended");
    if workers == 0 || count == 0 {
        return Err("workers and pairs must be positive".into());
    }
    let mut w = World::from_toml(include_str!(
        "../../../../content/parallel/auto_market.toml"
    ))?;
    w.regions[1].people.clear();
    w.regions[0].people = (0..count * 2)
        .map(|id| Person {
            id: id as u64,
            health: 100,
            hunger: 0,
        })
        .collect();
    w.rules.hunger_per_tick = 0;
    w.rules.food_growth = 0;
    w.goods_cargo.as_mut().unwrap().holdings = (0..count * 2)
        .map(|id| GoodsHolding {
            person: id as u64,
            items: vec![Amount {
                good: (id % 2 + 1) as u32,
                quantity: 1,
            }]
            .into(),
        })
        .collect();
    w.market.as_mut().unwrap().policies = (0..count)
        .map(|i| TradePolicy {
            person: 2 * i as u64 + 1,
            enabled: true,
            propose: false,
            request_help: false,
            min_gain: 0,
            help_value: 0,
            influences: Vec::new(),
            values: vec![
                TradeValue { good: 1, value: 1 },
                TradeValue { good: 2, value: 1 },
            ],
        })
        .collect();
    let offers: Vec<_> = (0..count)
        .map(|i| TradeProposal {
            proposer: if contended { 0 } else { 2 * i as u64 },
            recipient: 2 * i as u64 + 1,
            give: vec![Amount {
                good: 1,
                quantity: 1,
            }],
            receive: vec![Amount {
                good: 2,
                quantity: 1,
            }],
        })
        .collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    pool.install(|| w.propose_trades(&offers))?;
    let start = Instant::now();
    pool.install(|| w.step())?;
    let elapsed = start.elapsed();
    let accepted = if contended { 1 } else { count };
    if w.market.as_ref().unwrap().offers.len() != count - accepted
        || w.market.as_ref().unwrap().events.len() != accepted
    {
        return Err("automatic decisions incomplete".into());
    }
    let holdings = &w.goods_cargo.as_ref().unwrap().holdings;
    if holdings.iter().enumerate().any(|(i, h)| {
        let traded = !contended || i < 2;
        h.items[0].good
            != if traded {
                if i % 2 == 0 { 2 } else { 1 }
            } else if i % 2 == 0 {
                1
            } else {
                2
            }
    }) {
        return Err("goods not conserved".into());
    }
    println!(
        "workers={workers} pairs={count} contended={contended} elapsed_seconds={:.6}",
        elapsed.as_secs_f64()
    );
    Ok(())
}
