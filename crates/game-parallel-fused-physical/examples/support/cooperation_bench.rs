//! Timed full tick with independent requests and learned relationships.
use super::*;
use game_parallel::{
    Amount, GoodsHolding, OpinionRules, Opinions, TradeInfluence, TradeOffer, TradePolicy,
    TradeValue,
};

pub fn run(
    workers: usize,
    count: usize,
    repeats: usize,
    requests: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if workers == 0
        || count < 2
        || !count.is_multiple_of(2)
        || count > (u32::MAX / 3) as usize
        || repeats == 0
    {
        return Err("invalid benchmark arguments".into());
    }
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/auto_market.toml"
    ))?;
    world.rules.hunger_per_tick = 0;
    world.rules.food_capacity = (count * 3) as u32;
    world.regions[0].food = (count * 2) as u32;
    world.regions[0].people = (1..=count as u64)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    world.regions[1].people.clear();
    let half = count as u64 / 2;
    world.goods_cargo.as_mut().unwrap().holdings = (1..=half)
        .map(|id| GoodsHolding {
            person: id * 2,
            items: vec![Amount {
                good: 1,
                quantity: 1,
            }]
            .into(),
        })
        .collect();
    let market = world.market.as_mut().unwrap();
    market.next_id = half + 1;
    market.offers = (1..=half)
        .map(|id| TradeOffer {
            id,
            proposer: id * 2 - 1,
            recipient: id * 2,
            give: Default::default(),
            receive: vec![Amount {
                good: 1,
                quantity: 1,
            }]
            .into(),
            proposed_at: 0,
        })
        .collect();
    market.policies = (1..=half)
        .map(|id| TradePolicy {
            person: id * 2,
            enabled: true,
            propose: false,
            request_help: false,
            min_gain: 0,
            help_value: 0,
            values: vec![TradeValue { good: 1, value: 1 }],
            influences: vec![TradeInfluence {
                person: id * 2 - 1,
                attachment: 0,
                obligation: 0,
                fear: 1,
            }],
        })
        .collect();
    if requests {
        world.goods_cargo.as_mut().unwrap().holdings = (half + 1..=half * 2)
            .map(|person| GoodsHolding {
                person,
                items: vec![Amount {
                    good: 1,
                    quantity: 1,
                }]
                .into(),
            })
            .collect();
        let market = world.market.as_mut().unwrap();
        market.next_id = 1;
        market.offers.clear();
        market.policies = (1..=half * 2)
            .map(|person| TradePolicy {
                person,
                enabled: true,
                propose: false,
                request_help: person <= half,
                min_gain: 0,
                help_value: 0,
                values: vec![TradeValue { good: 1, value: 1 }],
                influences: vec![],
            })
            .collect();
    }
    world.opinions = Some(Opinions {
        sensitivities: Vec::new(),
        rules: OpinionRules {
            minimum: -5,
            maximum: 5,
            initial: 0,
            fulfilled_gain: 0,
            default_loss: 0,
            theft_loss: 0,
            encounter_gain: 0,
            fear_per_damage: 0,
            fear_decay_per_tick: 0,
            gratitude_gain: 1,
            obligation_relief: 0,
            minimum_report_trust: None,
            affinity: None,
            newborn_obligation_development: None,
        },
        records: vec![],
        events: vec![],
    });
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    pool.install(|| {
        let mut samples = Vec::new();
        for round in 0..=repeats {
            let mut kernel = Kernel::compile(world.clone()).unwrap();
            let start = Instant::now();
            kernel.step().unwrap();
            let elapsed = start.elapsed().as_secs_f64();
            if round > 0 { samples.push(elapsed); }
            let output = kernel.export();
            if requests {
                assert_eq!(output.market.unwrap().offers.len(), half as usize);
            } else {
                assert!(output.market.unwrap().offers.is_empty());
                assert_eq!(output.opinions.unwrap().records.len(), half as usize);
            }
        }
        samples.sort_by(f64::total_cmp);
        println!("workers={workers} people={count} mode={} requests={half} rounds={repeats} tick_median={:.6}", if requests { "requests" } else { "cooperation" }, samples[samples.len()/2]);
    });
    Ok(())
}
