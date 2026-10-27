//! Provisional stock-claim arbitration on actual cargo state; no stock is consumed.
use game_cargo::{Cargo, FOOD};
use game_cargo_planning::{StockDemand, StockIntent, plan_stock};
use game_core::*;
use game_drafting::arbitration::Decision;
use game_sim::*;
use std::time::Instant;

fn main() {
    let actors = game_performance::setting("LEGACY_POPULATION", 10_000usize);
    let stores = game_performance::setting("LEGACY_STORES", 64usize);
    let rounds = game_performance::setting("LEGACY_ROUNDS", 3usize);
    assert!(actors > 0 && stores > 0 && rounds > 0);
    game_performance::metrics::metadata();
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    let mut host = b.build(7, 1).unwrap();
    host.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = host
        .dispatch(game_land::CreateArea {
            name: "claim fixture".into(),
        })
        .unwrap();
    let mut inventories = Vec::new();
    for _ in 0..stores {
        let inventory = host
            .dispatch(game_cargo::Create {
                custody: game_cargo::Custody::Area(area),
                capacity: u64::MAX,
            })
            .unwrap();
        host.dispatch(game_cargo::Deposit {
            inventory,
            good: FOOD,
            quantity: (actors.div_ceil(stores) / 2).max(1) as u64,
        })
        .unwrap();
        inventories.push(inventory);
    }
    let claims: Vec<_> = (0..actors)
        .map(|i| StockIntent {
            id: i as u64,
            actor: PersonId(i as u64),
            participants: vec![],
            priority: 1,
            demands: vec![StockDemand {
                inventory: inventories[i % stores],
                good: FOOD,
                quantity: 1,
            }],
        })
        .collect();
    let pools: Vec<_> = [1, 4, 12]
        .into_iter()
        .map(|n| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(n)
                .build()
                .unwrap()
        })
        .collect();
    let before = host.save().unwrap();
    let cargo = host.get::<Cargo>().unwrap();
    let mut reference = None;
    for round in 0..rounds {
        let order = if round % 2 == 0 { [0, 1, 2] } else { [2, 1, 0] };
        for index in order {
            let start = Instant::now();
            // A fixed logical round keeps results comparable across timing rounds.
            let plan = plan_stock(cargo, &pools[index], 7, &claims).unwrap();
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            assert!(plan.stock_unchanged(cargo));
            if let Some(expected) = &reference {
                assert_eq!(&plan.resolution, expected);
            } else {
                reference = Some(plan.resolution.clone());
            }
            let accepted = plan
                .resolution
                .decisions
                .values()
                .filter(|d| **d == Decision::Granted)
                .count();
            assert_eq!(
                accepted as u64,
                plan.resolution.granted.values().sum::<u64>()
            );
            println!(
                "round={round} actors={actors} stores={stores} workers={} accepted={accepted} arbitration_ms={elapsed:.3}",
                pools[index].current_num_threads()
            );
        }
    }
    assert_eq!(before, host.save().unwrap());
}
