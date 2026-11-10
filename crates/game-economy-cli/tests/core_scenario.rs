use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run_scenario(workers: usize) -> serde_json::Value {
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let root = root();
            let mut host = game_economy_cli::core_scenario(
                &root.join("content/scenarios/economy-core.toml"),
                &root.join("content"),
            )
            .unwrap();
            assert_eq!(
                host.advance_continuous(7_200_000, &[]).unwrap().failure,
                None
            );
            let snapshot = host.snapshot();
            assert_eq!(snapshot.core.people.len(), 3);
            assert_eq!(snapshot.core.people[0].health, 94);
            assert_eq!(snapshot.family.as_ref().unwrap().totals.births, 1);
            assert!(snapshot.animal_restraints.is_some());
            assert!(
                snapshot
                    .dependent_care
                    .as_ref()
                    .unwrap()
                    .events
                    .iter()
                    .any(|event| event.child == 3 && event.caregiver == Some(2))
            );
            assert!(snapshot.gathering.as_ref().unwrap().nodes[0].available < 20);
            assert_eq!(snapshot.fauna.as_ref().unwrap().animals[0].age, 4);
            let saved = serde_json::to_string(&snapshot).unwrap();
            let mut resumed = game_economy::core_interval::CoreHost::resume(
                serde_json::from_str(&saved).unwrap(),
            )
            .unwrap();
            assert_eq!(
                host.advance_continuous(3_600_000, &[]).unwrap().failure,
                None
            );
            assert_eq!(
                resumed.advance_continuous(3_600_000, &[]).unwrap().failure,
                None
            );
            let result = serde_json::to_value(host.snapshot()).unwrap();
            assert_eq!(result, serde_json::to_value(resumed.snapshot()).unwrap());
            result
        })
}

#[test]
fn authored_core_scenario_runs_and_survives_save_resume() {
    assert_eq!(run_scenario(1), run_scenario(6));
}

#[test]
fn unsupported_scenario_owners_are_reported() {
    let root = root();
    let state = game_economy_cli::scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    let error = game_economy::core_interval::CoreHostSnapshot::from_scenario(state).unwrap_err();
    assert!(error.contains("campaign"), "{error}");
}

#[test]
fn renewable_animal_products_follow_the_animals_choice_and_physical_capacity() {
    fn run(workers: usize, willing: bool, capacity: bool) -> serde_json::Value {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let root = root();
                let initial = game_economy_cli::core_scenario(
                    &root.join("content/scenarios/economy-core.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let mut snapshot = initial.snapshot();
                snapshot.dependent_care = None;
                snapshot.fauna.as_mut().unwrap().animals[0]
                    .handling_response
                    .as_mut()
                    .unwrap()
                    .willing = willing;
                snapshot.animal_products.as_mut().unwrap().progress =
                    vec![game_economy::AnimalProductProgress {
                        animal: 1,
                        growth: 1,
                    }];
                if !capacity {
                    let good = snapshot
                        .core
                        .physical
                        .goods
                        .iter()
                        .find(|good| good.name == "grain")
                        .unwrap()
                        .id;
                    let container = snapshot
                        .core
                        .physical
                        .custody
                        .containers
                        .iter_mut()
                        .find(|container| container.place == game_lots::Place::Person(2))
                        .unwrap();
                    *container = game_lots::Container::new(
                        game_lots::Place::Person(2),
                        1,
                        vec![game_lots::Lot {
                            item: None,
                            good,
                            quantity: 1,
                            expires_at: None,
                        }],
                    )
                    .unwrap();
                }
                let encoded = serde_json::to_string(&snapshot).unwrap();
                let mut host = game_economy::core_interval::CoreHost::resume(
                    serde_json::from_str(&encoded).unwrap(),
                )
                .unwrap();
                assert_eq!(
                    host.advance_continuous(3_600_000, &[]).unwrap().failure,
                    None
                );
                let result = host.snapshot();
                let products = result.animal_products.as_ref().unwrap();
                assert_eq!(products.events.len(), 1);
                let expected = if !willing {
                    game_economy::AnimalProductOutcome::Refused
                } else if !capacity {
                    game_economy::AnimalProductOutcome::NoCapacity
                } else {
                    game_economy::AnimalProductOutcome::Collected
                };
                assert_eq!(products.events[0].outcome, expected);
                assert_eq!(
                    products.progress[0].growth,
                    if willing && capacity { 0 } else { 1 }
                );
                let encoded = serde_json::to_string(&result).unwrap();
                assert!(
                    game_economy::core_interval::CoreHost::resume(
                        serde_json::from_str(&encoded).unwrap()
                    )
                    .is_ok()
                );
                serde_json::to_value(result).unwrap()
            })
    }
    for (willing, capacity) in [(true, true), (false, true), (true, false)] {
        assert_eq!(run(1, willing, capacity), run(6, willing, capacity));
    }
}

#[test]
fn handling_reserves_animal_and_actor_before_product_collection() {
    let root = root();
    let initial = game_economy_cli::core_scenario(
        &root.join("content/scenarios/economy-core.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut snapshot = initial.snapshot();
    snapshot.dependent_care = None;
    snapshot.animal_products.as_mut().unwrap().progress =
        vec![game_economy::AnimalProductProgress {
            animal: 1,
            growth: 1,
        }];
    snapshot.animal_handling = Some(game_economy::AnimalHandling {
        inherit_births: false,
        policies: vec![game_economy::AnimalHandlingPolicy {
            person: 2,
            animal: Some(1),
            willing: true,
            affinity_at_least: None,
            affinity_at_most: None,
            pressured_by: None,
            pressure_threshold: None,
        }],
        events: vec![],
    });
    let mut host = game_economy::core_interval::CoreHost::resume(snapshot).unwrap();
    assert_eq!(
        host.advance_continuous(3_600_000, &[]).unwrap().failure,
        None
    );
    let result = host.snapshot();
    assert_eq!(result.animal_handling.unwrap().events.len(), 1);
    let products = result.animal_products.unwrap();
    assert!(products.events.is_empty());
    assert_eq!(products.progress[0].growth, 1);
}

#[test]
fn forced_animal_collection_requires_a_real_hold_and_preserves_subjective_reactions() {
    fn run(workers: usize) -> serde_json::Value {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let root = root();
                let initial = game_economy_cli::core_scenario(
                    &root.join("content/scenarios/economy-core.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let mut snapshot = initial.snapshot();
                snapshot.dependent_care = None;
                let animal = &mut snapshot.fauna.as_mut().unwrap().animals[0];
                animal.handling_response.as_mut().unwrap().willing = false;
                animal.social_response = Some(game_fauna::SocialResponse {
                    feed_affinity: 0,
                    minimum: -10,
                    maximum: 10,
                    capacity: 2,
                });
                animal.restraint_response = Some(game_economy::AnimalRestraintResponse {
                    yield_willingly: false,
                    yield_affinity_at_least: None,
                    yield_affinity_at_most: None,
                    resistance: 3,
                    escape_willing: false,
                    escape_effort: 0,
                    on_yielded: 0,
                    on_forced: -3,
                    on_resisted: 2,
                    on_released: 0,
                    on_release_blocked: 0,
                });
                let product = snapshot.animal_products.as_mut().unwrap();
                product.progress = vec![game_economy::AnimalProductProgress {
                    animal: 1,
                    growth: 1,
                }];
                product.policies[0].force_if_held = true;
                snapshot.animal_restraints = Some(game_economy::AnimalRestraints {
                    policies: vec![game_economy::AnimalCapturePolicy {
                        person: 2,
                        animal: Some(1),
                        willing: true,
                        affinity_at_least: None,
                        affinity_at_most: None,
                        effort: 4,
                        pressured_by: None,
                        pressure_threshold: None,
                    }]
                    .into(),
                    ..Default::default()
                });
                let mut host = game_economy::core_interval::CoreHost::resume(snapshot).unwrap();
                assert_eq!(
                    host.advance_continuous(3_600_000, &[]).unwrap().failure,
                    None
                );
                let captured = host.snapshot();
                assert_eq!(captured.animal_restraints.as_ref().unwrap().holds.len(), 1);
                assert_eq!(
                    captured.animal_restraints.as_ref().unwrap().events[0].kind,
                    game_economy::AnimalRestraintEventKind::Forced
                );
                assert_eq!(
                    captured.fauna.as_ref().unwrap().animals[0].opinions[0].affinity,
                    -3
                );
                assert!(captured.animal_products.as_ref().unwrap().events.is_empty());
                let saved = serde_json::to_string(&captured).unwrap();
                let mut resumed = game_economy::core_interval::CoreHost::resume(
                    serde_json::from_str(&saved).unwrap(),
                )
                .unwrap();
                assert_eq!(
                    host.advance_continuous(3_600_000, &[]).unwrap().failure,
                    None
                );
                assert_eq!(
                    resumed.advance_continuous(3_600_000, &[]).unwrap().failure,
                    None
                );
                let result = host.snapshot();
                assert_eq!(result, resumed.snapshot());
                let event = &result.animal_products.as_ref().unwrap().events[0];
                assert_eq!(event.outcome, game_economy::AnimalProductOutcome::Collected);
                assert!(event.forced);
                let mut escape = result.clone();
                let response = escape.fauna.as_mut().unwrap().animals[0]
                    .restraint_response
                    .as_mut()
                    .unwrap();
                response.escape_willing = true;
                response.escape_effort = 100;
                let mut escaping = game_economy::core_interval::CoreHost::resume(escape).unwrap();
                assert_eq!(
                    escaping.advance_continuous(3_600_000, &[]).unwrap().failure,
                    None
                );
                let escaped = escaping.snapshot();
                let restraint = escaped.animal_restraints.unwrap();
                assert!(restraint.holds.is_empty());
                assert!(
                    restraint
                        .events
                        .iter()
                        .any(|event| event.kind == game_economy::AnimalRestraintEventKind::Escaped)
                );
                serde_json::to_value(result).unwrap()
            })
    }
    assert_eq!(run(1), run(6));
}
