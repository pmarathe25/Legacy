use game_economy::core_interval::CoreHost;

#[test]
fn minute_observations_and_events_match_independent_advances() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut host = game_economy_cli::core_scenario(
        &root.join("content/scenarios/economy-history-integrated-hourly.toml"),
        &root.join("content"),
    )
    .unwrap();
    host.detach_player_control().unwrap();
    let opening = host.snapshot();
    rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap()
        .install(|| {
            let mut reference = CoreHost::resume(opening).unwrap();
            host.observe_continuously(&[], |steps| {
                for millis in [
                    60_000, 1, 124_000, 60_000, 5_399_907, 60_000, 731, 12_879_252,
                ] {
                    let expected = reference.advance_continuous(millis, &[]).unwrap();
                    let actual = steps.advance(millis).unwrap();
                    assert_eq!(actual.failure, None);
                    assert_eq!(expected, actual);
                    assert_eq!(reference.snapshot(), steps.snapshot());
                }
            });
            assert_eq!(host.snapshot(), reference.snapshot());
        });
}

#[test]
fn projected_vitals_survive_reload_and_still_publish_exact_starvation() {
    use game_economy::core_interval::{HungerRate, StarvationRate};

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut opening = game_economy_cli::core_scenario(
        &root.join("content/scenarios/economy-history-integrated-hourly.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    opening.core.rules.hunger_rate = Some(HungerRate {
        units: 1,
        per_millis: 1000,
    });
    opening.core.rules.starvation_threshold = 2;
    opening.core.rules.starvation_rate = Some(StarvationRate {
        units: 3,
        per_millis: 2000,
    });
    let patient = opening
        .core
        .people
        .iter_mut()
        .find(|person| person.health > 0 && person.immortal.is_none() && !person.controlled)
        .unwrap();
    let id = patient.id;
    patient.health = 2;
    patient.hunger = 0;
    patient.hunger_remainder = 0;
    patient.starvation_remainder = 0;

    let instant = 3334;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
            let mut host = CoreHost::resume(opening.clone()).unwrap();
            let mut dense = CoreHost::resume(opening.clone()).unwrap();
            let mut all_events = Vec::new();
            let saved = host.observe_continuously(&[], |steps| {
                for elapsed in [17, 219, 326, 117, 320, 2, 247, 651, 101, 403, 264, 317] {
                    let actual = steps.advance(elapsed).unwrap();
                    let expected = dense.advance_continuous(elapsed, &[]).unwrap();
                    assert_eq!(actual, expected);
                    all_events.extend(actual.events);
                    assert_eq!(steps.snapshot(), dense.snapshot());
                    assert_eq!(steps.person(id).as_ref(), dense.core().person(id));
                    if elapsed == 219 {
                        let exposed = steps.host().core();
                        assert_eq!(exposed.person(id), dense.core().person(id));
                        assert_eq!(exposed.next_deadline(None), dense.core().next_deadline(None));
                    }
                }
                assert_eq!(steps.simulated_millis(), instant - 350);
                let snapshot = steps.snapshot();
                serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap()
            });
            assert_eq!(host.snapshot(), dense.snapshot());
            let mut resumed = CoreHost::resume(saved).unwrap();
            let advanced = dense.advance_continuous(7, &[]).unwrap();
            assert_eq!(advanced, resumed.advance_continuous(7, &[]).unwrap());
            let interrupted = host.observe_continuously(&[], |steps| {
                assert_eq!(steps.advance(7).unwrap(), advanced);
                Err::<(), _>("observer interrupted")
            });
            assert_eq!(interrupted, Err("observer interrupted"));
            assert_eq!(host.snapshot(), dense.snapshot());
            let finishing = host.observe_continuously(&[], |steps| {
                let run = steps.advance(343).unwrap();
                assert_eq!(steps.host().core().person(id).unwrap().health, 0);
                run
            });
            assert_eq!(finishing, dense.advance_continuous(343, &[]).unwrap());
            assert_eq!(finishing, resumed.advance_continuous(343, &[]).unwrap());
            assert!(finishing.events.iter().any(|event| matches!(event,
                game_economy::core_interval::CoreRunEvent::Incapacitations { at_millis, people }
                    if *at_millis == instant && people.iter().any(|row| row.person == id && row.died)
            )));
            all_events.extend(finishing.events);
            assert_eq!(resumed.snapshot(), host.snapshot());
            (host.snapshot(), all_events)
        })
    };
    assert_eq!(run(1), run(6));
}
