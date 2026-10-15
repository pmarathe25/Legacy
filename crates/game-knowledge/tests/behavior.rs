mod support;
use game_core::*;
use game_knowledge::*;
use support::*;

#[test]
fn reports_deliver_known_facts_only_after_travel_delay() {
    let mut f = setup();
    assert!(view(&f.host, f.other).unwrap().is_empty());
    f.host
        .dispatch(ObserveStore {
            person: f.ruler,
            inventory: f.store,
        })
        .unwrap();
    f.host
        .dispatch(Report {
            sender: f.ruler,
            recipient: f.other,
            key: store_key(f.store),
            delay: SimDuration(1000),
        })
        .unwrap();
    run(&mut f.host, SimTime(999));
    assert!(view(&f.host, f.other).unwrap().is_empty());
    run(&mut f.host, SimTime(1000));
    let facts = view(&f.host, f.other).unwrap();
    assert_eq!(facts[&store_key(f.store)].source, f.ruler);
    f.host
        .dispatch(game_cargo::Consume {
            inventory: f.store,
            good: game_cargo::FOOD,
            quantity: 1000,
        })
        .unwrap();
    assert_eq!(facts, view(&f.host, f.other).unwrap());
}
#[test]
fn distant_observation_cannot_reveal_stock() {
    let mut f = setup();
    let dest = f
        .host
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(f.away),
            capacity: 100,
        })
        .unwrap();
    let before = f.host.save().unwrap();
    assert!(
        f.host
            .dispatch(ObserveStore {
                person: f.ruler,
                inventory: dest
            })
            .is_err()
    );
    assert_eq!(before, f.host.save().unwrap());
}

#[test]
fn fixture_state_roundtrips_with_only_required_plugins() {
    let f = setup();
    let save = f.host.save().unwrap();
    let loaded = builder().load(&save, 2).unwrap();
    assert_eq!(save, loaded.save().unwrap());
}

#[test]
fn a_person_cannot_plan_through_an_unobserved_intermediate_road() {
    let mut f = setup();
    let beyond = f
        .host
        .dispatch(game_land::CreateArea {
            name: "beyond".into(),
        })
        .unwrap();
    f.host
        .dispatch(game_land::Link {
            from: f.away,
            to: beyond,
            duration: MINUTE,
        })
        .unwrap();
    f.host
        .dispatch(ObserveSurroundings { person: f.other })
        .unwrap();
    let facts = view(&f.host, f.other).unwrap();
    assert_eq!(
        route(&facts, f.home, f.away).unwrap().duration,
        SimDuration(2 * MINUTE.0)
    );
    assert!(route(&facts, f.home, beyond).is_err());
    // Visiting only the intermediate area makes its outgoing road learnable.
    let arrives = f
        .host
        .dispatch(game_transport::Move {
            person: f.ruler,
            destination: f.away,
        })
        .unwrap();
    run(&mut f.host, arrives);
    f.host
        .dispatch(ObserveSurroundings { person: f.ruler })
        .unwrap();
    f.host
        .dispatch(Report {
            sender: f.ruler,
            recipient: f.other,
            key: roads_key(f.away),
            delay: MINUTE,
        })
        .unwrap();
    run(&mut f.host, SimTime(arrives.0 + MINUTE.0 - 1));
    assert!(route(&view(&f.host, f.other).unwrap(), f.home, beyond).is_err());
    run(&mut f.host, arrives.after(MINUTE).unwrap());
    let path = route(&view(&f.host, f.other).unwrap(), f.home, beyond).unwrap();
    assert_eq!(path.areas, vec![f.home, f.away, beyond]);
    assert_eq!(path.duration, SimDuration(3 * MINUTE.0));
}
#[test]
fn spoken_reports_are_local_and_reveal_only_the_selected_fact() {
    let mut f = setup();
    f.host
        .dispatch(ObserveStore {
            person: f.ruler,
            inventory: f.store,
        })
        .unwrap();
    f.host
        .dispatch(Tell {
            sender: f.ruler,
            recipient: f.other,
            key: store_key(f.store),
        })
        .unwrap();
    let now = f.host.now();
    run(&mut f.host, now);
    let facts = view(&f.host, f.other).unwrap();
    assert!(facts.contains_key(&store_key(f.store)));
    assert!(
        !facts.contains_key(&location_key(f.store)),
        "stock report does not also disclose location"
    );
    let arrives = f
        .host
        .dispatch(game_transport::Move {
            person: f.other,
            destination: f.away,
        })
        .unwrap();
    run(&mut f.host, arrives);
    let before = f.host.save().unwrap();
    assert!(
        f.host
            .dispatch(Tell {
                sender: f.ruler,
                recipient: f.other,
                key: location_key(f.store)
            })
            .is_err()
    );
    assert_eq!(before, f.host.save().unwrap());
}

#[test]
fn arriving_discovers_accessible_local_containers_but_does_not_refresh_every_remembered_stock() {
    let mut f = setup();
    let inventory = f
        .host
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(f.away),
            capacity: 100,
        })
        .unwrap();
    f.host
        .dispatch(game_property::ClaimInventory {
            claimant: f.other,
            inventory,
            recognized_by: std::collections::BTreeSet::from([f.other]),
        })
        .unwrap();
    f.host
        .dispatch(game_cargo::Deposit {
            inventory,
            good: game_cargo::FOOD,
            quantity: 20,
        })
        .unwrap();
    f.host
        .dispatch(ObserveSurroundings { person: f.other })
        .unwrap();
    assert!(
        !view(&f.host, f.other)
            .unwrap()
            .contains_key(&store_key(inventory))
    );
    let arrives = f
        .host
        .dispatch(game_transport::Move {
            person: f.other,
            destination: f.away,
        })
        .unwrap();
    run(&mut f.host, arrives);
    f.host
        .dispatch(ObserveSurroundings { person: f.other })
        .unwrap();
    let remembered = view(&f.host, f.other).unwrap()[&store_key(inventory)].clone();
    f.host
        .dispatch(game_cargo::Consume {
            inventory,
            good: game_cargo::FOOD,
            quantity: 20,
        })
        .unwrap();
    f.host
        .dispatch(ObserveSurroundings { person: f.other })
        .unwrap();
    assert_eq!(
        view(&f.host, f.other).unwrap()[&store_key(inventory)],
        remembered
    );
    f.host
        .dispatch(ObserveStore {
            person: f.other,
            inventory,
        })
        .unwrap();
    let stock: std::collections::BTreeMap<GoodId, u64> =
        known(&view(&f.host, f.other).unwrap(), &store_key(inventory)).unwrap();
    assert_eq!(stock.get(&game_cargo::FOOD).copied().unwrap_or(0), 0);
}

#[test]
fn a_delayed_report_cannot_overwrite_newer_observation_from_the_same_clock_time() {
    let mut f = setup();
    f.host
        .dispatch(ObserveStore {
            person: f.ruler,
            inventory: f.store,
        })
        .unwrap();
    f.host
        .dispatch(Report {
            sender: f.ruler,
            recipient: f.other,
            key: store_key(f.store),
            delay: MINUTE,
        })
        .unwrap();
    f.host
        .dispatch(game_cargo::Consume {
            inventory: f.store,
            good: game_cargo::FOOD,
            quantity: 1000,
        })
        .unwrap();
    f.host
        .dispatch(ObserveStore {
            person: f.other,
            inventory: f.store,
        })
        .unwrap();
    let newer = view(&f.host, f.other).unwrap()[&store_key(f.store)].clone();
    assert_eq!(newer.observed, SimTime(0));
    run(&mut f.host, SimTime(MINUTE.0));
    assert_eq!(view(&f.host, f.other).unwrap()[&store_key(f.store)], newer);
}
