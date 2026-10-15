mod support;
use game_core::*;
use game_knowledge::*;
use support::*;

#[test]
fn completed_container_discovery_tracks_permissions_without_refreshing_old_stock() {
    let mut f = setup();
    for _ in 0..2 {
        f.host
            .dispatch(ObserveSurroundings { person: f.ruler })
            .unwrap();
    }
    let initial: Stock = known(&view(&f.host, f.ruler).unwrap(), &store_key(f.store)).unwrap();
    f.host
        .dispatch(game_cargo::Consume {
            inventory: f.store,
            good: game_cargo::FOOD,
            quantity: 10,
        })
        .unwrap();
    run(&mut f.host, SimTime(1));
    f.host
        .dispatch(ObserveSurroundings { person: f.ruler })
        .unwrap();
    let facts = view(&f.host, f.ruler).unwrap();
    assert_eq!(known::<Stock>(&facts, &store_key(f.store)), Some(initial));
    assert_eq!(
        facts.get(roads_key(f.home).as_str()).unwrap().observed,
        SimTime(1)
    );

    let carrier = f
        .host
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: f.host.now(),
        })
        .unwrap();
    f.host
        .dispatch(game_transport::Place {
            person: carrier,
            area: f.away,
        })
        .unwrap();
    let incoming = f
        .host
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(carrier),
            capacity: 100,
        })
        .unwrap();
    f.host
        .dispatch(game_property::ClaimInventory {
            claimant: f.ruler,
            inventory: incoming,
            recognized_by: Default::default(),
        })
        .unwrap();
    f.host
        .dispatch(ObserveSurroundings { person: f.ruler })
        .unwrap();
    assert!(
        !view(&f.host, f.ruler)
            .unwrap()
            .contains_key(store_key(incoming).as_str())
    );
    let saved = f.host.save().unwrap();
    let mut loaded = builder().load(&saved, 4).unwrap();
    for host in [&mut f.host, &mut loaded] {
        host.dispatch(game_transport::Move {
            person: carrier,
            destination: f.home,
        })
        .unwrap();
        run(host, SimTime(1 + MINUTE.0 * 2));
        host.dispatch(ObserveSurroundings { person: f.ruler })
            .unwrap();
        assert!(
            view(host, f.ruler)
                .unwrap()
                .contains_key(store_key(incoming).as_str())
        );
        let local = host
            .dispatch(game_cargo::Create {
                custody: game_cargo::Custody::Area(f.home),
                capacity: 100,
            })
            .unwrap();
        host.dispatch(game_property::ClaimInventory {
            claimant: f.ruler,
            inventory: local,
            recognized_by: Default::default(),
        })
        .unwrap();
        host.dispatch(ObserveSurroundings { person: f.ruler })
            .unwrap();
        assert!(
            view(host, f.ruler)
                .unwrap()
                .contains_key(store_key(local).as_str())
        );
    }
    assert_eq!(f.host.save().unwrap(), loaded.save().unwrap());
}
