use game_economy::core_interval::{CoreHost, CoreRunEvent};

#[test]
fn an_unattended_lineage_keeps_its_newborns_autonomous_through_save_and_reload() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let template =
        std::fs::read_to_string(root.join("content/scenarios/economy-core-succession.toml"))
            .unwrap();
    let mut source = template
        .split_once("[care]")
        .unwrap()
        .0
        .replace(
            "tick_millis = 3600000",
            "tick_millis = 1000\ndecision_period_millis = 50",
        )
        .replace("father = 1, due = 1", "father = 1, due = 3");
    source.push_str("\n[family.elapsed_lifetimes]\nunit_millis = 100\nelapsed_gestation = true\n\n[campaign.lineage]\nstarted = 0\nwaiting_since = 0\n");
    let path = std::env::temp_dir().join(format!(
        "legacy-campaign-observer-{}.toml",
        std::process::id()
    ));
    std::fs::write(&path, source).unwrap();
    let loaded = game_economy_cli::core_scenario(&path, &root.join("content"));
    std::fs::remove_file(path).unwrap();
    let mut opening = loaded.unwrap().snapshot();
    opening.core.people[0].controlled = true;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .unwrap();
    pool.install(|| {
        let mut interactive = CoreHost::resume(opening.clone()).unwrap();
        let mut observer = CoreHost::resume(opening).unwrap();
        observer.detach_player_control().unwrap();
        assert!(observer.controlled_person(1).is_err());
        assert_eq!(observer.advance_continuous(299, &[]).unwrap().failure, None);
        let saved = serde_json::to_string(&observer.snapshot()).unwrap();
        let mut observer = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
        let frame = observer.advance_continuous(2, &[]).unwrap();
        assert_eq!(frame.failure, None);
        assert!(
            frame
                .events
                .iter()
                .any(|event| matches!(event, CoreRunEvent::Campaign(changes)
            if changes.iter().any(|change| change.person == 3 && change.at_millis == 300)))
        );
        assert_eq!(observer.campaign_view().unwrap().current, Some(3));
        assert_eq!(observer.campaign_view().unwrap().controlled, None);
        assert!(observer.controlled_person(3).is_err());
        assert!(
            observer
                .snapshot()
                .core
                .people
                .iter()
                .all(|person| !person.controlled)
        );

        assert_eq!(
            interactive.advance_continuous(301, &[]).unwrap().failure,
            None
        );
        assert_eq!(interactive.campaign_view().unwrap().controlled, Some(3));
        assert!(interactive.controlled_person(3).is_ok());
    });
}
