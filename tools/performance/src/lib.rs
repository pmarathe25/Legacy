//! Benchmark fixtures only. No alternate simulation engine.
pub mod exercise;
pub mod metrics;
use game_assembly::Simulation;
use game_core::*;
pub fn setting<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .map(|v| v.parse().ok().expect("invalid benchmark setting"))
        .unwrap_or(default)
}
pub fn fixture(count: usize, workers: usize, distributed: bool) -> (Simulation, Vec<InventoryId>) {
    assert!(count >= 15);
    let mut sim = Simulation::new("kernel-fixture", 1, workers).unwrap();
    let h = sim.debug_host_mut();
    let names = &h.get::<game_openings::Opening>().unwrap().names;
    let village = AreaId(names["village"]);
    let store = InventoryId(names["store"]);
    let ruler = PersonId(names["ruler"]);
    let mut areas = vec![village];
    let mut stores = vec![store];
    if distributed {
        for i in 1..32 {
            let area = h
                .dispatch(game_land::CreateArea {
                    name: format!("Settlement {i}"),
                })
                .unwrap();
            h.dispatch(game_land::Link {
                from: village,
                to: area,
                duration: MINUTE,
            })
            .unwrap();
            let inv = h
                .dispatch(game_cargo::Create {
                    custody: game_cargo::Custody::Area(area),
                    capacity: 1_000_000_000,
                })
                .unwrap();
            h.dispatch(game_property::ClaimInventory {
                claimant: ruler,
                inventory: inv,
                recognized_by: Default::default(),
            })
            .unwrap();
            areas.push(area);
            stores.push(inv);
        }
    }
    for store in &stores {
        h.dispatch(game_cargo::Deposit {
            inventory: *store,
            good: game_cargo::FOOD,
            quantity: count as u64 * 1000,
        })
        .unwrap();
    }
    for index in 15..count {
        let person = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        let group = index % areas.len();
        h.dispatch(game_transport::Place {
            person,
            area: areas[group],
        })
        .unwrap();
        h.dispatch(game_property::Permit {
            issuer: ruler,
            inventory: stores[group],
            person,
            allow: true,
        })
        .unwrap();
        h.dispatch(game_characters::Register {
            person,
            mind: Default::default(),
        })
        .unwrap();
        if index % 4 == 0 {
            h.dispatch(game_relationships::Interpret {
                observer: person,
                actor: ruler,
                shared: true,
                harmed: false,
                credible: true,
            })
            .unwrap();
        }
    }
    h.dispatch(game_characters::Start).unwrap();
    (sim, stores)
}
pub fn stock(sim: &Simulation, stores: &[InventoryId]) -> u64 {
    stores
        .iter()
        .map(|id| {
            sim.debug_host().get::<game_cargo::Cargo>().unwrap().0[id].amount(game_cargo::FOOD)
        })
        .sum()
}
pub fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}
