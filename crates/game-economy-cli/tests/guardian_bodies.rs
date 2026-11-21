use game_economy::core_interval::CoreHost;

#[test]
fn older_authored_guardians_use_the_same_bodies_custody_and_clock_as_other_people() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let source = root.join("content/scenarios/economy-fleece.toml");
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .unwrap();
    pool.install(|| {
        let mut host = game_economy_cli::core_scenario(&source, &root.join("content")).unwrap();
        let opening = host.snapshot();
        let guardian = opening.core.people.iter().find(|row| row.id == 99).unwrap();
        assert_eq!(guardian.region, Some(2));
        assert_eq!(
            guardian
                .immortal
                .as_ref()
                .unwrap()
                .recovery_rate
                .unwrap()
                .per_millis,
            opening.core.physical.tick_millis
        );
        assert!(opening.family.as_ref().unwrap().next_id > 99);
        assert!(
            opening
                .family
                .as_ref()
                .unwrap()
                .members
                .iter()
                .all(|row| row.id != 99)
        );
        let view = host.campaign_view().unwrap();
        assert_eq!(view.guardians[0].person, 99);
        assert_eq!(view.guardians[0].student, Some(1));
        assert_eq!(
            view.guardians[0].required_millis,
            3 * opening.core.physical.tick_millis
        );
        assert!(host.controlled_person(99).is_err());

        let interval = opening.core.physical.tick_millis;
        let mut resumed = CoreHost::resume(
            serde_json::from_str(&serde_json::to_string(&opening).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            host.advance_continuous(interval, &[]).unwrap().failure,
            None
        );
        assert_eq!(
            resumed
                .advance_continuous(interval / 2, &[])
                .unwrap()
                .failure,
            None
        );
        assert_eq!(
            resumed
                .advance_continuous(interval - interval / 2, &[])
                .unwrap()
                .failure,
            None
        );
        let state = host.snapshot();
        assert_eq!(state, resumed.snapshot());
        assert!(
            state
                .core
                .travel
                .as_ref()
                .unwrap()
                .journeys
                .iter()
                .any(|row| row.person == 99)
                || state
                    .core
                    .people
                    .iter()
                    .any(|row| row.id == 99 && row.region == Some(1))
        );
    });
}
