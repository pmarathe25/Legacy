//! Mutually accepted exchange batches. Arguments: workers, pairs.
use game_parallel::{
    Amount, GoodsHolding, Person, TradeDecision, TradeOutcome, TradeProposal, World,
};
use std::time::Instant;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let workers: usize = args.first().map_or(Ok(1), |value| value.parse())?;
    let count: usize = args.get(1).map_or(Ok(50_000), |value| value.parse())?;
    if workers == 0 || count == 0 {
        return Err("workers and pairs must be positive".into());
    }
    let mut w = World::from_toml(include_str!("../../../../content/parallel/workshop.toml"))?;
    w.regions[1].people.clear();
    w.regions[0].people = (0..count * 2)
        .map(|id| Person {
            id: id as u64,
            health: 100,
            hunger: 0,
        })
        .collect();
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
    let offers: Vec<_> = (0..count)
        .map(|i| TradeProposal {
            proposer: 2 * i as u64,
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
    let start = Instant::now();
    let (proposal_time, decision_time) = pool.install(|| -> Result<_, String> {
        let at = Instant::now();
        let ids = w.propose_trades(&offers)?;
        let proposal_time = at.elapsed();
        let decisions: Vec<_> = ids
            .into_iter()
            .enumerate()
            .map(|(i, id)| TradeDecision {
                id,
                actor: 2 * i as u64 + 1,
                outcome: TradeOutcome::Accepted,
            })
            .collect();
        let at = Instant::now();
        w.decide_trades(&decisions)?;
        Ok((proposal_time, at.elapsed()))
    })?;
    let elapsed = start.elapsed();
    if !w.market.as_ref().unwrap().offers.is_empty()
        || w.market.as_ref().unwrap().events.len() != count
    {
        return Err("incomplete decisions".into());
    }
    let holdings = &w.goods_cargo.as_ref().unwrap().holdings;
    if holdings.len() != count * 2
        || holdings
            .iter()
            .enumerate()
            .any(|(i, h)| h.items[0].good != if i % 2 == 0 { 2 } else { 1 })
    {
        return Err("exchange conservation failed".into());
    }
    println!(
        "workers={workers} pairs={count} elapsed_seconds={:.6} propose={:.6} decide={:.6}",
        elapsed.as_secs_f64(),
        proposal_time.as_secs_f64(),
        decision_time.as_secs_f64()
    );
    Ok(())
}
