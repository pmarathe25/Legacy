use game_economy::core_interval::CoreHost;
use game_economy::{LocalEndpoint, LocalLocation, LocalWalkOutcome};
use game_lots::Place;
use std::path::Path;

#[test]
fn people_can_notice_forget_or_return_to_a_store_they_visited() {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let scene = content.join("scenarios/economy-store-memory.toml");
    let run = |workers, parts: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = game_economy_cli::core_scenario(&scene, &content).unwrap();
                let store = LocalEndpoint::Store { id: 50 };
                let advance = |host: &mut CoreHost, millis| {
                    assert_eq!(host.advance_continuous(millis, &[]).unwrap().failure, None);
                    let checkpoint = serde_json::to_vec(&host.snapshot()).unwrap();
                    *host = CoreHost::resume(serde_json::from_slice(&checkpoint).unwrap()).unwrap();
                };
                for person in 1..=3 {
                    assert_eq!(host.walk_controlled(person, store).unwrap(), LocalWalkOutcome::Progressed);
                }
                advance(&mut host, 22);
                assert!(host.snapshot().local_transport.unwrap().preservation.iter().all(|row|
                    row.visits.unwrap().observed.is_none()));
                advance(&mut host, 1);
                let state = host.snapshot();
                let profiles = &state.local_transport.as_ref().unwrap().preservation;
                let first = profiles[0].visits.unwrap().observed.unwrap();
                assert_eq!((first.store, first.celsius, first.at_millis), (50, 4, 23));
                assert!(profiles[1].visits.unwrap().observed.is_none());
                assert_eq!(profiles[2].visits.unwrap().observed, Some(first));
                let mut future = state;
                future.local_transport.as_mut().unwrap().preservation[0].visits.as_mut().unwrap()
                    .observed.as_mut().unwrap().at_millis = 24;
                assert!(CoreHost::resume(future).is_err());

                for person in 1..=3 {
                    host.walk_controlled(person, LocalEndpoint::Stock).unwrap();
                }
                advance(&mut host, 23);
                let mut autonomous = host.snapshot();
                for person in &mut autonomous.core.people {
                    person.controlled = false;
                }
                host = CoreHost::resume(autonomous).unwrap();
                for &part in parts {
                    advance(&mut host, part);
                }
                assert!(matches!(host.local_location(1), Some(LocalLocation::At {endpoint, ..}) if endpoint == store));
                for person in [2, 3] {
                    assert!(matches!(host.local_location(person), Some(LocalLocation::At { endpoint: LocalEndpoint::Stock, .. })));
                }
                let ending = host.snapshot();
                let stock = ending.core.physical.custody.containers.iter()
                    .find(|row| row.place == Place::Inventory(50)).unwrap();
                assert_eq!(stock.used, 2);
                ending
            })
    };
    assert_eq!(run(1, &[40]), run(6, &[5, 14, 10, 11]));
}
