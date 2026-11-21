use game_economy::core_interval::{CoreHost, CoreRunEvent, TacticalEventKind};

#[test]
fn a_local_person_can_choose_to_intervene_under_subjective_pressure() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let template =
        std::fs::read_to_string(root.join("content/scenarios/economy-intervention.toml")).unwrap();
    let source = template
        .replace(
            "tick_millis = 3600000",
            "tick_millis = 1000\ncombat_round_millis = 137\ndecision_period_millis = 50",
        )
        .replace(
            "opponent = 1, willing = true",
            "opponent = 1, willing = false, pressure_from = 3, fear_at_least = 2",
        );
    let path = std::env::temp_dir().join(format!(
        "legacy-elapsed-reinforcements-{}.toml",
        std::process::id()
    ));
    std::fs::write(&path, source).unwrap();
    let loaded = game_economy_cli::core_scenario(&path, &root.join("content"));
    std::fs::remove_file(path).unwrap();
    let opening = loaded.unwrap().snapshot();
    let run = |workers, steps: &[u64], feared| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut snapshot = opening.clone();
                if feared {
                    let opinions = snapshot.opinions.as_mut().unwrap();
                    opinions.records = serde_json::from_value(serde_json::json!([
                        {"observer":2,"target":3,"trust":0,"fear":2}
                    ]))
                    .unwrap();
                }
                let mut host = CoreHost::resume(snapshot).unwrap();
                host.start_controlled_engagement(1, 3).unwrap();
                let mut entry = None;
                let mut strike = None;
                for &step in steps {
                    let run = host.advance_continuous(step, &[]).unwrap();
                    assert_eq!(run.failure, None);
                    for event in run.events {
                        if let CoreRunEvent::Tactical(round) = event {
                            if let Some(event) = round.events.iter().find(|row| {
                                row.person == Some(2) && row.kind == TacticalEventKind::Entered
                            }) {
                                entry = Some(*event);
                            }
                            if let Some(event) = round.events.iter().find(|row| {
                                row.person == Some(2) && row.kind == TacticalEventKind::Attacked
                            }) {
                                strike = Some(*event);
                            }
                        }
                    }
                    host = CoreHost::resume(
                        serde_json::from_str(&serde_json::to_string(&host.snapshot()).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                }
                if feared {
                    assert!(entry.unwrap().at_millis < 137);
                    assert_eq!(strike.unwrap().at_millis, 137);
                    assert_eq!(strike.unwrap().target, Some(1));
                    assert!(host.tactical_engagement(2).is_some());
                } else {
                    assert!(entry.is_none());
                    assert!(strike.is_none());
                    assert!(host.tactical_engagement(2).is_none());
                }
                (entry, strike, host.snapshot())
            })
    };
    run(2, &[137], false);
    assert_eq!(run(1, &[137], true), run(4, &[29, 52, 56], true));

    let mut manual = opening;
    manual.core.people[1].controlled = true;
    let mut host = CoreHost::resume(manual).unwrap();
    host.start_controlled_engagement(1, 3).unwrap();
    assert_eq!(
        host.join_controlled_engagement(2, 3, 1).unwrap().kind,
        TacticalEventKind::Entered
    );
    assert!(host.join_controlled_engagement(2, 3, 1).is_err());
}
