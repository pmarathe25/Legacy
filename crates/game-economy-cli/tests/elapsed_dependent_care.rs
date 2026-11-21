use game_economy::core_interval::{CoreHost, CoreRunEvent};

#[test]
fn care_visits_use_elapsed_deadlines_and_real_local_food() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let source =
        std::fs::read_to_string(root.join("content/scenarios/economy-core-succession.toml"))
            .unwrap();
    let mut source = source
        .split_once("[care]")
        .unwrap()
        .0
        .replace(
            "tick_millis = 3600000",
            "tick_millis = 1000\ndecision_period_millis = 50",
        )
        .replace(
            "hunger_per_tick = 1",
            "hunger_per_tick = 1\nhunger_rate = { units = 1, per_millis = 20 }",
        );
    source.push_str(
        r#"
[family.elapsed_lifetimes]
unit_millis = 100
elapsed_gestation = true

[care]
policies = [{ person = 2, willing = true }]

[care.rules]
dependent_until = 6
care_interval = 1000
max_caregiver_hunger = 20
neglect_damage = 3
fetch_local_food = true

[care.elapsed]
period_millis = 83
"#,
    );
    let path =
        std::env::temp_dir().join(format!("legacy-elapsed-care-{}.toml", std::process::id()));
    std::fs::write(&path, source).unwrap();
    let loaded = game_economy_cli::core_scenario(&path, &root.join("content"));
    std::fs::remove_file(path).unwrap();
    let opening = loaded.unwrap().snapshot();
    let run = |workers, steps: &[u64], willing| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = opening.clone();
                state.dependent_care.as_mut().unwrap().policies[0].willing = willing;
                let mut host = CoreHost::resume(state).unwrap();
                let mut visits = Vec::new();
                for &step in steps {
                    let result = host.advance_continuous(step, &[]).unwrap();
                    assert_eq!(result.failure, None);
                    visits.extend(result.events.into_iter().filter_map(|event| match event {
                        CoreRunEvent::Care(visit) => Some(visit),
                        _ => None,
                    }));
                    host = CoreHost::resume(
                        serde_json::from_str(&serde_json::to_string(&host.snapshot()).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                }
                assert_eq!(visits.len(), 1);
                let visit = &visits[0];
                assert_eq!(visit.at_millis, 166);
                assert_eq!(visit.events[0].child, 3);
                let state = host.snapshot();
                if willing {
                    assert_eq!(visit.events[0].caregiver, Some(2));
                    assert!(visit.transfers.iter().any(|row| row.accepted));
                    assert_eq!(state.core.people[2].health, 100);
                } else {
                    assert_eq!(visit.events[0].caregiver, None);
                    assert!(visit.transfers.is_empty());
                    assert_eq!(state.core.people[2].health, 97);
                }
                (visits, state)
            })
    };
    assert_eq!(run(1, &[166], true), run(4, &[43, 99, 24], true));
    run(2, &[166], false);

    let mut coincident = opening;
    let care = coincident.dependent_care.as_mut().unwrap();
    care.rules.dependent_until = 12;
    care.elapsed.as_mut().unwrap().period_millis = 500;
    let mut host = CoreHost::resume(coincident).unwrap();
    let result = host.advance_continuous(1000, &[]).unwrap();
    assert_eq!(result.failure, None);
    assert_eq!(result.events.iter().filter_map(|event| match event {
        CoreRunEvent::Care(visit) => Some(visit.at_millis),
        _ => None,
    }).collect::<Vec<_>>(), [500, 1000]);
}
