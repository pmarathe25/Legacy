use game_economy::core_interval::{CoreHost, CoreRunEvent};
use std::path::Path;
use std::sync::Arc;

#[test]
fn a_person_can_decline_one_partner_or_meet_because_of_their_own_fear() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let scenario = root.join("content/scenarios/economy-personal-meeting-refusal.toml");
    let opening = game_economy_cli::core_scenario(&scenario, &root.join("content"))
        .unwrap()
        .snapshot();
    let run = |workers, fear, affinity_weight, chunks: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = opening.clone();
                let opinions = state.opinions.as_mut().unwrap();
                opinions.records[1].fear = fear;
                Arc::make_mut(&mut opinions.sensitivities)[0].relationship_cases[0]
                    .social_affinity_weight = Some(affinity_weight);
                let mut host = CoreHost::resume(state).unwrap();
                let mut receipts = Vec::new();
                for &elapsed in chunks {
                    let advance = host.advance_continuous(elapsed, &[]).unwrap();
                    assert_eq!(advance.failure, None);
                    for event in advance.events {
                        if let CoreRunEvent::ElapsedEncounters {
                            at_millis,
                            meetings,
                            ..
                        } = event
                        {
                            receipts.extend(
                                meetings
                                    .into_iter()
                                    .map(|row| (at_millis, row.a, row.b, row.pressured)),
                            );
                        }
                    }
                    let save = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&save).unwrap()).unwrap();
                }
                receipts
            })
    };
    assert!(run(1, 0, 1, &[41]).is_empty());
    assert_eq!(run(1, 8, 1, &[41]), [(41, 1, 2, Some(2))]);
    assert_eq!(run(6, 8, 1, &[19, 22]), [(41, 1, 2, Some(2))]);
    assert_eq!(run(6, 0, -1, &[17, 24]), [(41, 1, 2, None)]);
}
