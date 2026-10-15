use game_api::*;
use game_assembly::*;
fn run(s: &mut Simulation, to: SimTime) {
    while s.advance(to, 128).unwrap().stop == game_sim::Stop::Budget {}
}
#[test]
fn movement_and_observations_use_one_canonical_action() {
    let mut s = Simulation::new("kernel-fixture", 1, 1).unwrap();
    let original = s.save().unwrap();
    for _ in 0..20 {
        s.observe().unwrap();
    }
    assert_eq!(original, s.save().unwrap());
    assert!(
        s.submit(
            1,
            Command::Move {
                destination: AreaId(99999)
            }
        )
        .is_err()
    );
    assert_eq!(original, s.save().unwrap());
    s.submit(
        1,
        Command::Move {
            destination: AreaId(2),
        },
    )
    .unwrap();
    run(&mut s, SimTime(119_999));
    assert_eq!(s.observe().unwrap().action.as_deref(), Some("travel"));
    run(&mut s, SimTime(120_000));
    assert_eq!(
        s.observe().unwrap().location,
        Some(game_transport::Location::At(AreaId(2)))
    );
    assert_eq!(s.observe().unwrap().action, None);
}
#[test]
fn time_chunks_workers_save_load_and_observation_frequency_match() {
    let mut a = Simulation::new("survival-fixture", 13, 1).unwrap();
    let mut b = Simulation::load(&a.save().unwrap(), 4).unwrap();
    run(&mut a, SimTime(DAY.0));
    for at in [1, 100_000, 3_000_000, 6_000_019, DAY.0] {
        b.observe().unwrap();
        run(&mut b, SimTime(at));
        if at == 3_000_000 {
            b = Simulation::load(&b.save().unwrap(), 2).unwrap();
        }
    }
    assert_eq!(a.save().unwrap(), b.save().unwrap());
    let living = a
        .debug_host()
        .get::<game_physiology::Bodies>()
        .unwrap()
        .people
        .values()
        .filter(|p| p.alive)
        .count();
    assert_eq!(
        living, 14,
        "autonomous residents survive; unattended player starves"
    );
}
#[test]
fn public_command_retries_survive_ruler_death_and_save_load() {
    let mut s = Simulation::new("kernel-fixture", 1, 1).unwrap();
    let result = s
        .submit(
            1,
            Command::Observe {
                inventory: InventoryId(19),
            },
        )
        .unwrap();
    let ruler = s.observe().unwrap().person.unwrap();
    s.debug_host_mut()
        .dispatch(game_physiology::Damage {
            person: ruler,
            amount: 100,
        })
        .unwrap();
    let mut loaded = Simulation::load(&s.save().unwrap(), 1).unwrap();
    assert_eq!(
        loaded
            .submit(
                1,
                Command::Observe {
                    inventory: InventoryId(19)
                }
            )
            .unwrap(),
        result
    );
    assert!(loaded.observe().unwrap().person.is_none());
}
#[test]
fn ordinary_views_do_not_reveal_distant_unreported_stock() {
    let mut s = Simulation::new("kernel-fixture", 1, 1).unwrap();
    let host = s.debug_host_mut();
    let distant = host
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(AreaId(2)),
            capacity: 1000,
        })
        .unwrap();
    host.dispatch(game_cargo::Deposit {
        inventory: distant,
        good: game_cargo::FOOD,
        quantity: 777,
    })
    .unwrap();
    let before = s.save().unwrap();
    assert!(
        !s.observe()
            .unwrap()
            .facts
            .contains_key(game_knowledge::store_key(distant).as_str())
    );
    assert!(
        !serde_json::to_string(&s.observe().unwrap())
            .unwrap()
            .contains("777")
    );
    assert_eq!(before, s.save().unwrap());
}
#[test]
fn every_supported_fixture_constructs_and_resumes() {
    for scenario in scenarios().unwrap() {
        assert!(scenario.fixture);
        let mut s = Simulation::new(&scenario.id, 4, 1).unwrap();
        let first = s.advance(SimTime(60_000), 128).unwrap();
        let mut resumed = Simulation::load(&s.save().unwrap(), 2).unwrap();
        let a = s.advance(SimTime(120_000), 128).unwrap();
        let b = resumed.advance(SimTime(120_000), 128).unwrap();
        assert_eq!(a, b, "{}", scenario.id);
        assert_eq!(s.save().unwrap(), resumed.save().unwrap());
        if scenario.id == "combat-fixture" {
            assert!(matches!(first.stop, game_sim::Stop::Choice(_)));
        } else {
            assert_eq!(first.stop, game_sim::Stop::Target);
        }
    }
}
#[test]
fn malformed_references_and_reused_allocators_are_rejected() {
    let s = Simulation::new("kernel-fixture", 1, 1).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&s.save().unwrap()).unwrap();
    value["next_id"] = serde_json::json!(1);
    assert!(Simulation::load(&value.to_string(), 1).is_err());
    value = serde_json::from_str(&s.save().unwrap()).unwrap();
    value["resources"]["lineage.continuity"]["current"] = serde_json::json!(99999);
    assert!(Simulation::load(&value.to_string(), 1).is_err());
}
#[test]
fn tactical_views_include_geometry_and_hide_pending_orders() {
    let mut s = Simulation::new("combat-fixture", 1, 1).unwrap();
    let view = s.observe().unwrap();
    assert_eq!(view.battles.len(), 1);
    assert!(
        !view.battles[0]
            .map
            .obstacles
            .iter()
            .any(|(_, radius)| *radius < 0)
    );
    let snapshot = serde_json::to_value(view).unwrap();
    assert!(snapshot["battles"][0].get("orders").is_none());
    let battle = s.observe().unwrap().battles[0].id;
    s.submit(
        1,
        Command::Combat {
            battle,
            action: game_conflict::OrderKind::Wait,
        },
    )
    .unwrap();
    let p = s.advance(SimTime(10_000), 100).unwrap();
    assert_eq!(p.reached, SimTime(6000));
}
