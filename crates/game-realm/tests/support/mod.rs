#![allow(dead_code)]
use game_core::*;
use game_sim::*;
use std::collections::BTreeSet;
pub struct Fixture {
    pub host: Host,
    pub home: AreaId,
    pub away: AreaId,
    pub ruler: PersonId,
    pub other: PersonId,
    pub guardian: PersonId,
    pub store: InventoryId,
    pub pack: InventoryId,
}
pub fn setup() -> Fixture {
    let mut host = builder().build(7, 1).unwrap();
    host.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let home = host
        .dispatch(game_land::CreateArea {
            name: "home".into(),
        })
        .unwrap();
    let away = host
        .dispatch(game_land::CreateArea {
            name: "away".into(),
        })
        .unwrap();
    host.dispatch(game_land::Link {
        from: home,
        to: away,
        duration: SimDuration(120_000),
    })
    .unwrap();
    let ruler = host
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    let other = host
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    let guardian = host
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Guardian,
            born: SimTime(0),
        })
        .unwrap();
    for person in [ruler, other, guardian] {
        host.dispatch(game_transport::Place { person, area: home })
            .unwrap();
    }
    let store = host
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(home),
            capacity: 10_000,
        })
        .unwrap();
    let pack = host
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(ruler),
            capacity: 1000,
        })
        .unwrap();
    for inv in [store, pack] {
        host.dispatch(game_property::ClaimInventory {
            claimant: ruler,
            inventory: inv,
            recognized_by: BTreeSet::from([ruler]),
        })
        .unwrap();
    }
    for (good, quantity) in [
        (game_cargo::FOOD, 1000),
        (game_cargo::SEED, 100),
        (game_cargo::TIMBER, 100),
        (game_cargo::MEDICINE, 10),
    ] {
        host.dispatch(game_cargo::Deposit {
            inventory: store,
            good,
            quantity,
        })
        .unwrap();
    }
    Fixture {
        host,
        home,
        away,
        ruler,
        other,
        guardian,
        store,
        pack,
    }
}
pub fn run(host: &mut Host, target: SimTime) {
    loop {
        let progress = host.advance(target, 1000).unwrap();
        if progress.stop == Stop::Target {
            break;
        }
        assert_eq!(progress.stop, Stop::Budget, "unexpected stop");
    }
}
pub fn food(host: &Host, inventory: InventoryId) -> u64 {
    host.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(game_cargo::FOOD)
}

pub fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_skills::plugin(&mut b).unwrap();
    game_institutions::plugin(&mut b).unwrap();
    game_realm::plugin(&mut b).unwrap();
    b
}
