use game_economy::core_interval::{CoreHost, CoreRunEvent};
use game_lots::Place;
use std::path::Path;

#[test]
fn personal_destinations_change_actual_shelf_life_with_finite_space() {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let path = content.join("scenarios/economy-chosen-preservation.toml");
    let run = |workers, partitions: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = game_economy_cli::core_scenario(&path, &content).unwrap();
                let opening = host.snapshot();
                let expires = opening
                    .core
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|holder| holder.place == Place::Person(2))
                    .unwrap()
                    .lots[0]
                    .expires_at
                    .unwrap();
                let mut events = Vec::new();
                for &elapsed in partitions {
                    let advanced = host.advance_continuous(elapsed, &[]).unwrap();
                    assert_eq!(advanced.failure, None);
                    for event in advanced.events {
                        if let CoreRunEvent::LocalCarries(rows) = event {
                            events.extend(rows);
                        }
                    }
                    let bytes = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&bytes).unwrap()).unwrap();
                }
                let world = host.snapshot();
                let stores = &world.core.physical.custody.containers;
                let at = |place| stores.iter().find(|row| row.place == place).unwrap();
                let cold = at(Place::Inventory(1));
                let warm = at(Place::Inventory(2));
                assert_eq!((cold.used, warm.used), (2, 2));
                assert!(cold.lots[0].expires_at.unwrap() > expires);
                assert!(warm.lots[0].expires_at.unwrap() < expires);
                assert_eq!(at(Place::Person(3)).used, 0);
                assert_eq!(at(Place::Person(4)).used, 2);
                assert_eq!(at(Place::Person(2)).used + at(Place::Person(5)).used, 2);
                assert_eq!(stores.iter().map(|holder| holder.used).sum::<u64>(), 8);
                assert!(events.iter().any(|event| event.person == 3
                    && event.to == Place::Inventory(2)
                    && event.moved));
                (events, world)
            })
    };
    assert_eq!(run(1, &[2000]), run(6, &[13, 987, 536, 464]));
}
