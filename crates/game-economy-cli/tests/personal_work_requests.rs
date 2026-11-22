use game_autowork::requests::{OfferValue, Outcome, Request};
use game_economy::core_interval::CoreHost;
use std::path::Path;

#[test]
fn reachable_workers_rank_later_requests_and_feel_the_same_hunger_differently() {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let scene = content.join("scenarios/economy-personal-work-requests.toml");
    let run = |workers, quantum| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = game_economy_cli::core_scenario(&scene, &content)
                    .unwrap()
                    .snapshot();
                state.core.physical.tick_millis = quantum;
                let recipe = state
                    .core
                    .physical
                    .recipes
                    .iter()
                    .find(|row| row.name == "fixture-tool")
                    .unwrap()
                    .id;
                let bread = state
                    .core
                    .physical
                    .goods
                    .iter()
                    .find(|row| row.name == "bread")
                    .unwrap()
                    .id;
                let work = state.autowork.as_mut().unwrap().requests.as_mut().unwrap();
                work.next_id = 4;
                work.preferences.iter_mut().for_each(|person| {
                    person.offer_value = Some(OfferValue {
                        good: bread,
                        value_per_unit: 1,
                        max_quantity: 30,
                    });
                });
                work.pending = [(1, 1, 3), (2, 2, 3), (3, 2, 4)]
                    .into_iter()
                    .map(|(id, requester, worker)| Request {
                        id,
                        requester,
                        worker,
                        recipe,
                        region: 1,
                        created_at: 0,
                        expires: 20,
                    })
                    .collect();
                let mut host = CoreHost::resume(state).unwrap();
                // There is no food in either proposer's bag.
                host.propose_agreements(&[(1, 3, 2), (2, 3, 20), (2, 4, 20)].map(
                    |(debtor, creditor, quantity)| game_agreements::Proposal {
                        debtor,
                        creditor,
                        quantity,
                        good: bread,
                        due: 79,
                    },
                ))
                .unwrap();
                assert_eq!(host.advance_continuous(20, &[3, 4]).unwrap().failure, None);
                let checkpoint = serde_json::to_vec(&host.snapshot()).unwrap();
                let mut host =
                    CoreHost::resume(serde_json::from_slice(&checkpoint).unwrap()).unwrap();
                let responses = host.respond_to_work_requests(&[], 1).unwrap();
                assert_eq!(responses.actors, vec![3, 4]);
                assert!(matches!(
                    responses.events[0].outcome,
                    Outcome::Started { .. }
                ));
                assert_eq!(responses.events[0].request, 2);
                assert_eq!(responses.events[1].request, 3);
                assert_eq!(responses.events[1].outcome, Outcome::Refused);
                let saved = host.snapshot();
                let work = saved.autowork.as_ref().unwrap().requests.as_ref().unwrap();
                assert_eq!(
                    work.pending.iter().map(|row| row.id).collect::<Vec<_>>(),
                    [1]
                );
                assert_eq!(saved.core.physical.work.len(), 1);
                assert_eq!(saved.core.physical.work[0].job.worker, 3);
                assert!(
                    saved
                        .agreements
                        .unwrap()
                        .contracts
                        .iter()
                        .all(|row| row.delivered == 0)
                );
                responses
                    .events
                    .iter()
                    .map(|event| (event.request, event.outcome.clone()))
                    .collect::<Vec<_>>()
            })
    };
    assert_eq!(run(1, 37), run(6, 13));
}
