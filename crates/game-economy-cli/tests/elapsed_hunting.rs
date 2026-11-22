use game_economy::core_interval::{CoreHost, CoreRunEvent};
use game_economy_player::interval::Player;
use game_lots::Place;
use std::path::Path;

#[test]
fn a_hunt_follows_a_persons_clock_and_yield_is_atomic_with_the_animal() {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let scene = content.join("scenarios/economy-elapsed-hunting.toml");
    let run = |workers, quantum, parts: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = game_economy_cli::core_scenario(&scene, &content)
                    .unwrap()
                    .snapshot();
                state.core.physical.tick_millis = quantum;
                let mut host = CoreHost::resume(state).unwrap();
                let mut hunts = Vec::new();
                let mut deaths = Vec::new();
                for &part in parts {
                    let advanced = host.advance_continuous(part, &[]).unwrap();
                    assert_eq!(advanced.failure, None);
                    for event in advanced.events {
                        match event {
                            CoreRunEvent::ElapsedHunting { at_millis, events } => {
                                hunts.extend(events.into_iter().map(|row| (at_millis, row)));
                            }
                            CoreRunEvent::Fauna(row) => {
                                deaths.extend(
                                    row.deaths
                                        .into_iter()
                                        .map(|death| (row.at_millis, death.animal)),
                                );
                            }
                            _ => {}
                        }
                    }
                    let bytes = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&bytes).unwrap()).unwrap();
                }
                assert_eq!(hunts.len(), 1);
                assert_eq!(
                    (hunts[0].0, hunts[0].1.person, hunts[0].1.animal),
                    (83, 1, 1)
                );
                assert_eq!(hunts[0].1.quantity, 3);
                assert_eq!(deaths, [(83, 1)]);
                assert!(
                    Player::new(&mut host, 2)
                        .unwrap()
                        .hunt(2)
                        .unwrap()
                        .is_none()
                );
                assert_eq!(host.snapshot().fauna.unwrap().animals[1].health, 100);
                let second = Player::new(&mut host, 3).unwrap().hunt(2).unwrap().unwrap();
                assert_eq!((second.person, second.animal, second.quantity), (3, 2, 3));
                let state = host.snapshot();
                assert_eq!(state.fauna.unwrap().animals[1].health, 0);
                let carrying = |person| {
                    state
                        .core
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Person(person))
                        .unwrap()
                };
                let first = carrying(1);
                assert_eq!(first.used, 6);
                assert_eq!(first.lots[0].quantity, 3);
                assert_eq!(carrying(2).used, 8);
                assert_eq!(carrying(3).used, 6);
                (hunts, deaths, first.lots[0].expires_at)
            })
    };
    assert_eq!(run(1, 37, &[300]), run(6, 159, &[51, 32, 27, 190]));
}
