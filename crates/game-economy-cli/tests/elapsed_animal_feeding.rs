use game_economy::core_interval::{CoreHost, CoreRunEvent};
use game_economy::{AnimalCare, AnimalCarePolicy};
use game_lots::{Container, Place};
use std::path::Path;

#[test]
fn feeding_uses_real_supplies_at_personal_decisions_and_reserves_the_animal() {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let scene = content.join("scenarios/economy-elapsed-hunting.toml");
    let run = |workers, quantum, chunks: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut opening = game_economy_cli::core_scenario(&scene, &content)
                    .unwrap()
                    .snapshot();
                opening.core.physical.tick_millis = quantum;
                opening.core.people[2].controlled = false;
                let hunting = &mut opening.hunting.as_mut().unwrap().policies[0];
                hunting.person = 3;
                hunting.hunger_at_least = None;
                let barley = opening
                    .core
                    .physical
                    .goods
                    .iter()
                    .find(|row| row.name == "barley")
                    .unwrap()
                    .id;
                let custody = &mut opening.core.physical.custody;
                let mut seed = custody
                    .containers
                    .iter()
                    .find(|row| {
                        row.place
                            == (Place::Stock {
                                region: 1,
                                good: barley,
                            })
                    })
                    .unwrap()
                    .lots[0]
                    .clone();
                seed.quantity = 2;
                let holder = custody
                    .containers
                    .iter_mut()
                    .find(|row| row.place == Place::Person(1))
                    .unwrap();
                *holder = Container::with_footprints(
                    holder.place,
                    holder.capacity,
                    vec![seed],
                    &custody.footprints,
                )
                .unwrap();
                opening.fauna.as_mut().unwrap().animals[0].hunger = 10;
                opening.animal_care = Some(AnimalCare {
                    elapsed_actions: true,
                    inherit_births: false,
                    policies: vec![AnimalCarePolicy {
                        person: 1,
                        animal: Some(1),
                        good: barley,
                        quantity: 1,
                        willing: true,
                        health_at_most: 100,
                        affinity_at_least: None,
                        affinity_at_most: None,
                        pressured_by: None,
                        pressure_threshold: None,
                    }],
                });
                let mut host = CoreHost::resume(opening).unwrap();
                let mut offered = Vec::new();
                let mut hunts = Vec::new();
                let mut advance = |host: &mut CoreHost, part| {
                    let result = host.advance_continuous(part, &[]).unwrap();
                    assert_eq!(result.failure, None);
                    for event in result.events {
                        match event {
                            CoreRunEvent::ElapsedAnimalFeeding { at_millis, events } => {
                                offered.extend(
                                    events.into_iter().map(|row| {
                                        (at_millis, row.animal, row.person, row.quantity)
                                    }),
                                );
                            }
                            CoreRunEvent::ElapsedHunting { at_millis, events } => {
                                hunts.extend(
                                    events
                                        .into_iter()
                                        .map(|row| (at_millis, row.animal, row.person)),
                                );
                            }
                            _ => {}
                        }
                    }
                    let checkpoint = serde_json::to_vec(&host.snapshot()).unwrap();
                    *host = CoreHost::resume(serde_json::from_slice(&checkpoint).unwrap()).unwrap();
                };
                for &part in chunks {
                    advance(&mut host, part);
                }
                let alive = host.snapshot();
                let animal = &alive.fauna.as_ref().unwrap().animals[0];
                assert_eq!((animal.health, animal.hunger), (100, 2));
                assert_eq!(
                    alive
                        .core
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Person(1))
                        .unwrap()
                        .used,
                    0
                );
                advance(&mut host, 2);
                assert_eq!(host.snapshot().fauna.unwrap().animals[0].health, 0);
                assert_eq!(offered, [(0, 1, Some(1), 1), (83, 1, Some(1), 1)]);
                assert_eq!(hunts, [(166, 1, 3)]);
                (offered, hunts)
            })
    };
    assert_eq!(run(1, 37, &[165]), run(6, 159, &[30, 53, 82]));
}
