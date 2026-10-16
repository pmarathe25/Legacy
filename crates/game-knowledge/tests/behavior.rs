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
    assert_eq!(
        facts.get(store_key(f.store).as_str()).unwrap().source,
        f.ruler
    );
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
    assert!(facts.contains_key(store_key(f.store).as_str()));
    assert!(
        !facts.contains_key(location_key(f.store).as_str()),
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
            .contains_key(store_key(inventory).as_str())
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
    let remembered = view(&f.host, f.other)
        .unwrap()
        .get(store_key(inventory).as_str())
        .unwrap()
        .to_owned();
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
        view(&f.host, f.other)
            .unwrap()
            .get(store_key(inventory).as_str())
            .unwrap(),
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
    let newer = view(&f.host, f.other)
        .unwrap()
        .get(store_key(f.store).as_str())
        .unwrap()
        .to_owned();
    assert_eq!(newer.observed, SimTime(0));
    run(&mut f.host, SimTime(MINUTE.0));
    assert_eq!(
        view(&f.host, f.other)
            .unwrap()
            .get(store_key(f.store).as_str())
            .unwrap(),
        newer
    );
}

#[test]
fn observers_share_unchanged_payloads_but_keep_individual_evidence_and_stale_snapshots() {
    use std::sync::Arc;
    let mut f = setup();
    for person in [f.ruler, f.other] {
        f.host.dispatch(ObserveSurroundings { person }).unwrap();
        f.host
            .dispatch(ObserveStore {
                person,
                inventory: f.store,
            })
            .unwrap();
    }
    let first = view(&f.host, f.ruler).unwrap();
    let other = view(&f.host, f.other).unwrap();
    let roads = roads_key(f.home);
    let stock = store_key(f.store);
    assert!(Arc::ptr_eq(
        first.get(roads.as_str()).unwrap().value,
        other.get(roads.as_str()).unwrap().value
    ));
    assert!(Arc::ptr_eq(
        first.get(stock.as_str()).unwrap().value,
        other.get(stock.as_str()).unwrap().value
    ));
    assert_eq!(first.get(roads.as_str()).unwrap().source, f.ruler);
    assert_eq!(other.get(roads.as_str()).unwrap().source, f.other);
    assert!(
        other.get(roads.as_str()).unwrap().revision > first.get(roads.as_str()).unwrap().revision
    );
    let reloaded = builder().load(&f.host.save().unwrap(), 2).unwrap();
    let a = view(&reloaded, f.ruler).unwrap();
    let b = view(&reloaded, f.other).unwrap();
    assert!(Arc::ptr_eq(
        a.get(roads.as_str()).unwrap().value,
        b.get(roads.as_str()).unwrap().value
    ));
    assert!(Arc::ptr_eq(
        a.keys().next().unwrap(),
        b.keys().next().unwrap()
    ));
    f.host
        .dispatch(game_land::Link {
            from: f.home,
            to: f.away,
            duration: MINUTE,
        })
        .unwrap();
    f.host
        .dispatch(game_cargo::Consume {
            inventory: f.store,
            good: game_cargo::FOOD,
            quantity: 10,
        })
        .unwrap();
    f.host
        .dispatch(ObserveSurroundings { person: f.ruler })
        .unwrap();
    f.host
        .dispatch(ObserveStore {
            person: f.ruler,
            inventory: f.store,
        })
        .unwrap();
    let refreshed = view(&f.host, f.ruler).unwrap();
    assert!(!Arc::ptr_eq(
        first.get(roads.as_str()).unwrap().value,
        refreshed.get(roads.as_str()).unwrap().value
    ));
    assert!(!Arc::ptr_eq(
        first.get(stock.as_str()).unwrap().value,
        refreshed.get(stock.as_str()).unwrap().value
    ));
    assert_eq!(view(&f.host, f.other).unwrap(), other);
    assert_eq!(
        route(&first, f.home, f.away).unwrap().duration,
        SimDuration(120_000)
    );
    assert_eq!(route(&refreshed, f.home, f.away).unwrap().duration, MINUTE);
    let mut loaded = builder().load(&f.host.save().unwrap(), 4).unwrap();
    // Reconstructing an empty cache cannot affect recorded facts or future output.
    for host in [&mut f.host, &mut loaded] {
        host.dispatch(ObserveSurroundings { person: f.ruler })
            .unwrap();
        host.dispatch(ObserveStore {
            person: f.ruler,
            inventory: f.store,
        })
        .unwrap();
    }
    assert_eq!(f.host.save().unwrap(), loaded.save().unwrap());
}

#[test]
fn typed_loading_preserves_opaque_extensions_and_numeric_evidence() {
    let data = serde_json::json!({"roads:1": {"value": {"name": "home", "links": {}, "unrecognized": 7}, "source": 1, "observed": 0, "revision": 0, "confidence": 90}});
    let facts: Facts = serde_json::from_value(data.clone()).unwrap();
    assert_eq!(serde_json::to_value(&facts).unwrap(), data);
    assert!(matches!(
        facts.get("roads:1").unwrap().value.as_ref(),
        FactValue::Other(_)
    ));
    let f = setup();
    let mut host = f.host;
    host.dispatch(ObserveStore {
        person: f.ruler,
        inventory: f.store,
    })
    .unwrap();
    assert!(matches!(
        view(&host, f.ruler)
            .unwrap()
            .get(store_key(f.store).as_str())
            .unwrap()
            .value
            .as_ref(),
        FactValue::Stock(_)
    ));
}

#[test]
fn numeric_payloads_and_opaque_sources_keep_canonical_wire_order_across_loading() {
    use std::sync::Arc;
    let mut facts = Facts::default();
    for (key, value) in [
        (
            "roads:1",
            serde_json::json!({"name": "home", "links": {"2": 10, "10": 20}}),
        ),
        ("store:1", serde_json::json!({"2": 10, "10": 20})),
        (
            "store-location:1",
            serde_json::json!({"area": 1, "carrier": null, "capacity": 50, "mass": 30}),
        ),
        (
            "patch:1",
            serde_json::json!({"area": 1, "good": 1, "stock": 10, "capacity": 50, "renew": 2}),
        ),
    ] {
        facts.insert(
            key.into(),
            Fact {
                value: Arc::new(value.into()),
                source: PersonId(1),
                observed: SimTime(0),
                revision: 0,
                confidence: 100,
            },
        );
    }
    let encoded = serde_json::to_string(&facts).unwrap();
    let loaded: Facts = serde_json::from_str(&encoded).unwrap();
    assert_eq!(facts, loaded);
    assert_eq!(serde_json::to_string(&loaded).unwrap(), encoded);
}
