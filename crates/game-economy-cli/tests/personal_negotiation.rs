use game_agreements::Status;
use game_economy::core_interval::{CoreHost, CoreRunEvent};
use std::path::Path;

#[test]
fn hungry_people_can_offer_rank_accept_and_decline_unfunded_promises() {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let path = content.join("scenarios/economy-personal-negotiation.toml");
    let run = |workers, partitions: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = game_economy_cli::core_scenario(&path, &content).unwrap();
                let mut responses = Vec::new();
                for &elapsed in partitions {
                    let advanced = host.advance_continuous(elapsed, &[]).unwrap();
                    assert_eq!(advanced.failure, None);
                    for event in advanced.events {
                        if let CoreRunEvent::PromiseDeclines { at_millis, events } = event {
                            responses.extend(events.into_iter().map(|row| (at_millis, row.actor)));
                        }
                    }
                    let bytes = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&bytes).unwrap()).unwrap();
                }
                let world = host.snapshot();
                let agreements = world.agreements.as_ref().unwrap();
                let decisions: Vec<_> = agreements
                    .contracts
                    .iter()
                    .map(|promise| (promise.debtor, promise.creditor, promise.status))
                    .collect();
                assert_eq!(
                    decisions,
                    [
                        (1, 2, Status::Active),
                        (3, 4, Status::Declined),
                        (5, 6, Status::Active),
                        (7, 8, Status::Proposed),
                        (9, 8, Status::Active),
                    ]
                );
                assert_eq!(responses, [(0, 4)]);
                assert!(
                    world
                        .core
                        .physical
                        .custody
                        .containers
                        .iter()
                        .all(|holder| holder.used == 0)
                );
                (responses, world)
            })
    };
    assert_eq!(run(1, &[19]), run(6, &[3, 11, 5]));
}
