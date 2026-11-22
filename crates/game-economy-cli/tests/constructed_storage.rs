use game_economy::LocalEndpoint;
use game_economy::core_interval::CoreHost;
use game_economy_player::interval::Player;
use game_lots::Place;
use std::path::Path;

fn scenario() -> CoreHost {
    load("economy-constructed-storage.toml")
}

fn load(scenario: &str) -> CoreHost {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let path = content.join("scenarios").join(scenario);
    game_economy_cli::core_scenario(&path, &content).unwrap()
}

#[test]
fn a_builder_reconsiders_preservation_after_reaching_their_cellar() {
    use game_economy::core_interval::CoreRunEvent;
    use game_economy::{LocalLocation as Location, LocalWalkOutcome as Outcome};

    let run = |workers, chunks: &[u64], destroyed| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = load("economy-own-cellar.toml");
                let mut arrivals = Vec::new();
                let advance = |host: &mut CoreHost, elapsed, arrivals: &mut Vec<_>| {
                    let run = host.advance_continuous(elapsed, &[]).unwrap();
                    assert_eq!(run.failure, None);
                    for event in run.events {
                        if let CoreRunEvent::LocalWalking { at_millis, events } = event {
                            arrivals.extend(events.into_iter().filter_map(|row| {
                                (row.outcome == Outcome::Arrived).then_some((row.person, at_millis))
                            }));
                        }
                    }
                    let checkpoint = serde_json::to_vec(&host.snapshot()).unwrap();
                    *host = CoreHost::resume(serde_json::from_slice(&checkpoint).unwrap()).unwrap();
                };
                advance(&mut host, 1, &mut arrivals);
                let opening = host.snapshot();
                let local = opening.local_transport.as_ref().unwrap();
                assert_eq!(local.stores.len(), 3);
                let known = |person| {
                    local
                        .preservation
                        .iter()
                        .find(|row| row.person == person)
                        .unwrap()
                        .own_cellar
                        .unwrap()
                        .known
                        .unwrap()
                };
                let (preferred, unwanted) = (known(2), known(3));
                assert_ne!(preferred.store, unwanted.store);
                assert_eq!(preferred.built_celsius, 4);
                let structure = local
                    .stores
                    .iter()
                    .find(|row| row.id == preferred.store)
                    .unwrap()
                    .structure
                    .unwrap();
                assert_eq!(structure.builder, Some(2));
                let mut forged = opening.clone();
                forged.local_transport.as_mut().unwrap().preservation[0]
                    .own_cellar
                    .as_mut()
                    .unwrap()
                    .known = Some(unwanted);
                assert!(CoreHost::resume(forged).is_err());

                advance(&mut host, 17, &mut arrivals);
                assert!(matches!(host.local_location(2), Some(Location::Walking {
                    remaining: 22, to: LocalEndpoint::Store { id }, ..
                }) if id == preferred.store));
                assert!(matches!(
                    host.local_location(3),
                    Some(Location::At {
                        endpoint: LocalEndpoint::Stock,
                        ..
                    })
                ));
                if destroyed {
                    assert!(host.demolish_controlled_site(1, structure.site).unwrap());
                }
                for &chunk in chunks {
                    advance(&mut host, chunk, &mut arrivals);
                }
                assert_eq!(host.core().simulated_millis(), 55);
                assert_eq!(arrivals, [(2, 40)]);
                let ending = host.snapshot();
                let used = |place| {
                    ending
                        .core
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == place)
                        .unwrap()
                        .used
                };
                assert_eq!(
                    used(Place::Inventory(preferred.store)),
                    if destroyed { 0 } else { 2 }
                );
                assert_eq!(used(Place::Person(2)), if destroyed { 2 } else { 0 });
                assert_eq!(used(Place::Inventory(unwanted.store)), 0);
                assert_eq!(used(Place::Person(3)), 2);
                (ending, arrivals)
            })
    };
    assert_eq!(run(1, &[37], false), run(6, &[2, 15, 6, 14], false));
    assert_eq!(run(1, &[37], true), run(6, &[2, 15, 6, 14], true));
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
