use game_economy::LocalEndpoint;
use game_economy::core_interval::CoreHost;
use game_economy_player::interval::Player;
use game_lots::Place;
use std::path::Path;

fn scenario() -> CoreHost {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let path = content.join("scenarios/economy-constructed-storage.toml");
    game_economy_cli::core_scenario(&path, &content).unwrap()
}

#[test]
fn only_a_builder_who_pays_the_kit_creates_a_holder() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = scenario();
                let outcome = host.build_sites_autonomously(&[]).unwrap();
                assert_eq!(outcome.events.len(), 2);
                assert_eq!(
                    outcome
                        .events
                        .iter()
                        .filter(|event| event.site.is_some())
                        .count(),
                    1
                );
                let built = host.snapshot();
                let stores = &built.local_transport.as_ref().unwrap().stores;
                assert_eq!(stores.len(), 2);
                assert_eq!(stores[0].id, 50);
                assert_eq!(stores[1].id, 51);
                let structure = stores[1].structure.unwrap();
                assert_eq!(
                    outcome.events.iter().find_map(|event| event.site),
                    Some(structure.site)
                );
                let holders = &built.core.physical.custody.containers;
                assert_eq!(
                    holders
                        .iter()
                        .find(|row| row.place == Place::Site(structure.site))
                        .unwrap()
                        .used,
                    2
                );
                assert_eq!(
                    holders
                        .iter()
                        .find(|row| row.place == Place::Inventory(51))
                        .unwrap()
                        .capacity,
                    8
                );
                assert!(CoreHost::resume(built.clone()).is_ok());
                (outcome.events, built)
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn a_reached_store_loses_insulation_on_demolition_without_losing_food() {
    let run = |workers, parts: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = scenario();
                let opening = host.snapshot();
                let holder = |place| {
                    opening
                        .core
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|holder| holder.place == place)
                        .unwrap()
                };
                let bread = holder(Place::Person(1)).lots[0].good;
                let original_expiry = holder(Place::Person(1)).lots[0].expires_at.unwrap();
                let site = Player::new(&mut host, 1)
                    .unwrap()
                    .build(1)
                    .unwrap()
                    .unwrap();
                let store = 51;
                let mut player = Player::new(&mut host, 1).unwrap();
                assert!(player.carry_store(store, bread, 2, true).is_err());
                assert!(
                    !player
                        .stores()
                        .unwrap()
                        .iter()
                        .find(|row| row.id == store)
                        .unwrap()
                        .reachable
                );
                player.walk(LocalEndpoint::Store { id: store }).unwrap();
                let advance = |world: &mut CoreHost, steps: &[u64]| {
                    for &elapsed in steps {
                        assert_eq!(
                            world.advance_continuous(elapsed, &[]).unwrap().failure,
                            None
                        );
                        let checkpoint = serde_json::to_vec(&world.snapshot()).unwrap();
                        *world =
                            CoreHost::resume(serde_json::from_slice(&checkpoint).unwrap()).unwrap();
                    }
                };
                advance(&mut host, parts);
                assert_eq!(host.core().simulated_millis(), 23);
                assert!(
                    Player::new(&mut host, 1)
                        .unwrap()
                        .carry_store(store, bread, 2, true)
                        .unwrap()
                );
                let stored = host.snapshot();
                let place = Place::Inventory(store);
                let food_lot = |world: &game_economy::core_interval::CoreHostSnapshot| {
                    world
                        .core
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|holder| holder.place == place)
                        .unwrap()
                        .lots[0]
                        .clone()
                };
                let cold_expiry = food_lot(&stored).expires_at.unwrap();
                assert_eq!(cold_expiry, 23 + (original_expiry - 23) * 2);
                Player::new(&mut host, 1)
                    .unwrap()
                    .walk(LocalEndpoint::Stock)
                    .unwrap();
                advance(&mut host, parts);
                assert_eq!(host.core().simulated_millis(), 46);
                assert!(Player::new(&mut host, 1).unwrap().demolish(site).unwrap());
                let ruin = host.snapshot();
                let stored = food_lot(&ruin);
                assert_eq!((stored.good, stored.quantity), (bread, 2));
                assert_eq!(stored.expires_at, Some(46 + (cold_expiry - 46).div_ceil(2)));
                let stores = &ruin.local_transport.as_ref().unwrap().stores;
                assert!(!stores[1].structure.unwrap().intact);
                assert!(
                    ruin.core
                        .physical
                        .custody
                        .storage_climate
                        .as_ref()
                        .unwrap()
                        .override_at(place)
                        .is_none()
                );
                assert!(CoreHost::resume(ruin.clone()).is_ok());
                ruin
            })
    };
    assert_eq!(run(1, &[23]), run(6, &[7, 15, 1]));
}
