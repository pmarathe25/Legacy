//! Structural churn and real in-flight cargo at the persistence boundary.
use game_assembly::Simulation;
use game_core::*;
pub struct FollowUp {
    destination: InventoryId,
    quantity: u64,
    timber: u64,
    reservation: u64,
}
fn total(sim: &Simulation) -> u64 {
    sim.debug_host()
        .get::<game_cargo::Cargo>()
        .unwrap()
        .0
        .values()
        .map(|inv| inv.amount(game_cargo::TIMBER))
        .sum()
}
pub fn prepare(sim: &mut Simulation, population: usize) -> FollowUp {
    let count = (population / 1000).max(1);
    let h = sim.debug_host_mut();
    let names = &h.get::<game_openings::Opening>().unwrap().names;
    let ruler = PersonId(names["ruler"]);
    let caregiver = PersonId(names["guardian-1"]);
    let village = AreaId(names["village"]);
    let farm = AreaId(names["farm"]);
    let store = InventoryId(names["store"]);
    let victims: Vec<_> = h
        .get::<game_physiology::Bodies>()
        .unwrap()
        .people
        .iter()
        .filter(|(id, p)| **id != ruler && p.kind == game_physiology::Kind::Mortal && p.alive)
        .take(count)
        .map(|(id, _)| *id)
        .collect();
    let father = victims[0];
    for _ in 0..count {
        h.dispatch(game_lifecycle::Birth {
            mother: ruler,
            father,
            attendant: caregiver,
            area: village,
        })
        .unwrap();
    }
    for victim in victims {
        h.dispatch(game_physiology::Damage {
            person: victim,
            amount: 1000,
        })
        .unwrap();
    }
    let destination = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(farm),
            capacity: count as u64 + 1,
        })
        .unwrap();
    let reservation = h
        .dispatch(game_cargo::ReserveSpace {
            inventory: destination,
            good: game_cargo::TIMBER,
            quantity: 1,
        })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: store,
        good: game_cargo::TIMBER,
        quantity: count as u64,
    })
    .unwrap();
    for _ in 0..count {
        let carrier = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: h.now(),
            })
            .unwrap();
        h.dispatch(game_transport::Place {
            person: carrier,
            area: village,
        })
        .unwrap();
        h.dispatch(game_property::Permit {
            issuer: ruler,
            inventory: store,
            person: carrier,
            allow: true,
        })
        .unwrap();
        let pack = h
            .dispatch(game_cargo::Create {
                custody: game_cargo::Custody::Carried(carrier),
                capacity: 1,
            })
            .unwrap();
        h.dispatch(game_logistics::Haul {
            carrier,
            from: store,
            to: destination,
            pack,
            good: game_cargo::TIMBER,
            quantity: 1,
        })
        .unwrap();
    }
    assert_eq!(h.get::<game_logistics::Logistics>().unwrap().0.len(), count);
    println!(
        "births={count} deaths={count} shipments={count} capacity_reservations=1 pending_arrivals={count}"
    );
    FollowUp {
        destination,
        quantity: count as u64,
        timber: total(sim),
        reservation,
    }
}
impl FollowUp {
    pub fn check(&self, sim: &Simulation) {
        assert_eq!(total(sim), self.timber);
        let cargo = sim.debug_host().get::<game_cargo::Cargo>().unwrap();
        assert_eq!(
            cargo.0[&self.destination].amount(game_cargo::TIMBER),
            self.quantity
        );
        assert!(
            cargo.0[&self.destination]
                .reserved
                .contains_key(&self.reservation)
        );
        assert!(
            sim.debug_host()
                .get::<game_logistics::Logistics>()
                .unwrap()
                .0
                .is_empty()
        );
    }
}
