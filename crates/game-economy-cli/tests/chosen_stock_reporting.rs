use game_economy::core_interval::{CoreHost, CoreRunEvent};
use game_economy_knowledge::StockTime;
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

#[test]
fn hearing_refreshes_memory_at_the_encounter_instead_of_at_a_custody_boundary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let canonical = std::fs::read_to_string(
        root.join("content/scenarios/economy-chosen-stock-reporting.toml"),
    ).unwrap();
    let run = |workers, quantum, before_contact: u64| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let text = canonical.replace("tick_millis = 1000", &format!("tick_millis = {quantum}"));
                let path = std::env::temp_dir().join(format!(
                    "legacy-stock-time-{}-{quantum}.toml",
                    std::process::id(),
                ));
                std::fs::write(&path, text).unwrap();
                let loaded = game_economy_cli::core_scenario(&path, &root.join("content"));
                std::fs::remove_file(path).unwrap();
                let mut host = loaded.unwrap();
                host.set_food_reporting_controlled(1, true).unwrap();
                host.detach_player_control().unwrap();
                if before_contact != 0 {
                    assert_eq!(host.advance_continuous(before_contact, &[]).unwrap().failure, None);
                }
                assert_eq!(host.advance_continuous(41 - before_contact, &[]).unwrap().failure, None);
                let opening = host.snapshot();
                let recall = opening.knowledge.as_ref().unwrap();
                let firsthand = recall.minds[1].records.iter().find(|row| row.region == 1).unwrap();
                assert_eq!(firsthand.observed_at_millis, Some(41));
                let trace = recall.retention[0].traces.iter().find(|trace| trace.region == 2).unwrap();
                assert_eq!(trace.reinforced_at_millis, Some(41));
                let mut invalid = opening.clone();
                invalid.knowledge.as_mut().unwrap().retention[0].traces
                    .iter_mut().find(|trace| trace.region == 2).unwrap().reinforced_at_millis = Some(42);
                assert!(CoreHost::resume(invalid).is_err());
                let saved = serde_json::to_vec(&opening).unwrap();
                let mut host = CoreHost::resume(serde_json::from_slice(&saved).unwrap()).unwrap();
                let recalled = |host: &CoreHost, at| {
                    let time = StockTime::elapsed(at / quantum, u128::from(at), quantum);
                    host.snapshot().knowledge.unwrap().recall_at_time(2, time).unwrap()
                        .into_iter().find(|row| row.region == 2)
                };
                assert_eq!(host.advance_continuous(22, &[]).unwrap().failure, None);
                let received = recalled(&host, 63).unwrap();
                assert_eq!((received.food, received.witness, received.learned_from), (7, 1, Some(1)));
                assert_eq!(host.advance_continuous(1, &[]).unwrap().failure, None);
                assert!(recalled(&host, 64).is_none());
                received
            })
    };
    assert_eq!(run(1, 1000, 0), run(6, 17, 19));
}
