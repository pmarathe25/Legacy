use game_economy::core_interval::{CoreHost, CoreRunEvent};
use std::path::Path;

#[test]
fn observation_does_not_make_a_person_volunteer_a_report() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let opening = game_economy_cli::core_scenario(
        &root.join("content/scenarios/economy-chosen-stock-reporting.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();

    let run = |workers, choice, delta, steps: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = CoreHost::resume(opening.clone()).unwrap();
                if let Some(choice) = choice {
                    host.set_food_reporting_controlled(1, choice).unwrap();
                }
                host.set_food_misreport_controlled(1, delta, Some(2))
                    .unwrap();
                let source = host.food_reports_controlled(1).unwrap();
                assert_eq!(source.iter().find(|row| row.region == 2).unwrap().food, 7);
                host.detach_player_control().unwrap();
                let mut times = Vec::new();
                for &elapsed in steps {
                    let result = host.advance_continuous(elapsed, &[]).unwrap();
                    assert_eq!(result.failure, None);
                    for event in result.events {
                        if let CoreRunEvent::ElapsedEncounters {
                            at_millis,
                            meetings,
                            ..
                        } = event
                        {
                            assert_eq!(meetings.len(), 1);
                            times.push(at_millis);
                        }
                    }
                    let save = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&save).unwrap()).unwrap();
                }
                assert_eq!(times, [41]);
                let state = host.snapshot().knowledge.unwrap();
                let firsthand = state.minds[0]
                    .records
                    .iter()
                    .find(|row| row.region == 2)
                    .unwrap();
                assert_eq!(
                    (firsthand.food, firsthand.witness, firsthand.learned_from),
                    (7, 1, None)
                );
                state.minds[1]
                    .records
                    .iter()
                    .find(|row| row.region == 2)
                    .cloned()
            })
    };

    assert!(run(1, None, 15, &[41]).is_none());
    assert!(run(6, Some(false), 15, &[19, 22]).is_none());
    let received = run(1, Some(true), 15, &[41]).unwrap();
    assert_eq!(received, run(6, Some(true), 15, &[12, 29]).unwrap());
    assert_eq!(
        (received.food, received.witness, received.learned_from),
        (22, 1, Some(1))
    );
}
