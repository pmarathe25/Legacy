use game_economy::core_interval::{CoreHost, CoreRunEvent};
use game_fauna::handling::Outcome;
use std::path::Path;

#[test]
fn animal_and_person_interpret_each_contact_at_the_personal_decision_time() {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let scenario = content.join("scenarios/economy-elapsed-animal-handling.toml");
    let execute =
        |workers, quantum, steps: &[u64]| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut state = game_economy_cli::core_scenario(&scenario, &content)
                        .unwrap()
                        .snapshot();
                    state.core.physical.tick_millis = quantum;
                    let mut host = CoreHost::resume(state).unwrap();
                    let mut trace = Vec::new();
                    for &step in steps {
                        let run = host.advance_continuous(step, &[]).unwrap();
                        assert_eq!(run.failure, None);
                        for event in run.events {
                            match event {
                                CoreRunEvent::ElapsedAnimalHandling { at_millis, events } => {
                                    trace.extend(events.into_iter().map(|row| {
                                        (at_millis, row.person, row.animal, row.outcome)
                                    }));
                                }
                                CoreRunEvent::ElapsedHunting { .. } => {
                                    panic!("animal was already approached")
                                }
                                _ => {}
                            }
                        }
                        let bytes = serde_json::to_vec(&host.snapshot()).unwrap();
                        host = CoreHost::resume(serde_json::from_slice(&bytes).unwrap()).unwrap();
                    }
                    let saved = host.snapshot();
                    let animal = &saved.fauna.as_ref().unwrap().animals[0];
                    assert_eq!(animal.health, 100);
                    assert_eq!(animal.opinions[0].person, 1);
                    assert_eq!(animal.opinions[0].affinity, 3);
                    assert_eq!(saved.animal_feelings.unwrap().affinity(1, 1), Some(1));
                    assert_eq!(
                        trace,
                        [(0, 1, 1, Outcome::Accepted), (83, 1, 1, Outcome::Refused)]
                    );
                    trace
                })
        };
    assert_eq!(execute(1, 37, &[165]), execute(6, 159, &[41, 43, 81]));
}
