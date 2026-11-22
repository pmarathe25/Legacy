use game_economy::core_interval::{CoreHost, CoreRunEvent};
use std::path::Path;

#[test]
fn elapsed_encounters_keep_their_ordinal_and_infect_at_the_actual_time() {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let scene = content.join("scenarios/economy-social-disease-clocks.toml");
    let run = |workers, quantum, parts: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = game_economy_cli::core_scenario(&scene, &content)
                    .unwrap()
                    .snapshot();
                initial.core.physical.tick_millis = quantum;
                let mut host = CoreHost::resume(initial).unwrap();
                let mut encounters = Vec::new();
                let mut disease = Vec::new();
                for &part in parts {
                    let result = host.advance_continuous(part, &[]).unwrap();
                    assert_eq!(result.failure, None);
                    for event in result.events {
                        match event {
                            CoreRunEvent::ElapsedEncounters {
                                at_millis,
                                meetings,
                                ..
                            } => {
                                encounters.extend(
                                    meetings
                                        .into_iter()
                                        .map(|row| (at_millis, row.tick, row.a, row.b)),
                                );
                            }
                            CoreRunEvent::Disease(rows) => disease.extend(
                                rows.into_iter()
                                    .filter(|row| row.person == 2)
                                    .map(|row| (row.at_millis.unwrap(), row.person, row.source)),
                            ),
                            _ => {}
                        }
                    }
                    let save = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&save).unwrap()).unwrap();
                }
                let state = host.snapshot();
                let infected = state
                    .health
                    .unwrap()
                    .owner
                    .cases
                    .into_iter()
                    .find(|row| row.person == 2)
                    .unwrap();
                assert_eq!(infected.infected_at, 83 / quantum);
                assert_eq!(infected.infected_at_millis, Some(83));
                assert_eq!(encounters, [(83, 1, 1, 2), (166, 2, 1, 2)]);
                assert_eq!(disease, [(83, 2, Some(1))]);
                (encounters, disease)
            })
    };
    assert_eq!(run(1, 37, &[170]), run(6, 129, &[11, 72, 46, 41]));
}
