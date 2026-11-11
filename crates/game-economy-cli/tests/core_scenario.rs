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
            assert_eq!(snapshot.medical.as_ref().unwrap().policies.len(), 1);
            assert_eq!(snapshot.goals.as_ref().unwrap().people.len(), 1);
            let purpose = &snapshot.purpose.as_ref().unwrap().owner;
            assert_eq!(purpose.people.len(), 3);
            assert_eq!(purpose.people[0].deprivation, 2);
            assert_eq!(purpose.people[2].eligible_at, 13);
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

#[test]
fn medical_help_consumes_real_medicine_and_can_transmit_infection() {
    fn run(workers: usize, willing: bool) -> serde_json::Value {
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
                let medicine = snapshot
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
                    100,
                    vec![game_lots::Lot {
                        item: None,
                        good: medicine,
                        quantity: 1,
                        expires_at: None,
                    }],
                )
                .unwrap();
                snapshot.health = Some(game_economy::core_interval::CoreHealth {
                    owner: game_disease::Disease {
                        strains: vec![game_disease::Strain {
                            id: 1,
                            name: "illness".into(),
                            incubation: 0,
                            infectious_ticks: 5,
                            immunity_ticks: 0,
                            damage_per_tick: 0,
                            transmission_per_million: 1_000_000,
                            combat_transmission_per_million: 0,
                            birth_transmission_per_million: 0,
                        }],
                        treatments: vec![game_disease::Treatment {
                            strain: 1,
                            medicine,
                        }],
                        cases: vec![game_disease::Case {
                            person: 1,
                            strain: 1,
                            infected_at: 0,
                            source: None,
                        }],
                        events: vec![],
                    },
                    treatment: vec![],
                    last_tick: None,
                });
                snapshot.opinions = Some(serde_json::from_value(serde_json::json!({
                    "rules": {"minimum": -10, "maximum": 10, "initial": 0, "fulfilled_gain": 1, "default_loss": 1,
                        "gratitude_gain": 5, "affinity": {"limit": 10}},
                    "sensitivities": [{"person": 1, "gratitude_gain": 0, "affinity_help": -2}],
                    "records": []
                })).unwrap());
                snapshot.medical = Some(game_economy::Medical {
                    policies: vec![game_economy::MedicalPolicy {
                        person: 2,
                        patient: 1,
                        medicine,
                        quantity: 1,
                        willing,
                        patient_health_at_most: None,
                        pressured_by: None,
                        pressure_threshold: None,
                    }],
                    acquaintances: vec![],
                });
                let saved = serde_json::to_string(&snapshot).unwrap();
                let mut host = game_economy::core_interval::CoreHost::resume(snapshot).unwrap();
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
                assert!(game_economy::core_interval::CoreHost::resume(result.clone()).is_ok());
                let relationship = result.opinions.as_ref().unwrap().records.iter()
                    .find(|record| record.observer == 1 && record.target == 2);
                if willing {
                    let relationship = relationship.unwrap();
                    assert_eq!(relationship.affinity, -2);
                    assert_eq!(relationship.obligation, 0);
                } else {
                    assert!(relationship.is_none());
                }
                let cases = &result.health.as_ref().unwrap().owner.cases;
                assert_eq!(cases.iter().any(|case| case.person == 1), !willing);
                assert_eq!(cases.iter().any(|case| case.person == 2), willing);
                assert!(!cases.iter().any(|case| case.person == 3));
                let container = result
                    .core
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|container| container.place == game_lots::Place::Person(2))
                    .unwrap();
                assert_eq!(container.used, u64::from(!willing));
                serde_json::to_value(result).unwrap()
            })
    }
    assert_eq!(run(1, true), run(6, true));
    assert_eq!(run(1, false), run(6, false));
}

#[test]
fn personal_food_requests_and_pressured_giving_use_real_custody() {
    fn run(workers: usize, willing: bool, pressured: bool) -> serde_json::Value {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
            let root = root();
            let initial = game_economy_cli::core_scenario(
                &root.join("content/scenarios/economy-core.toml"), &root.join("content"),
            ).unwrap();
            let mut snapshot = initial.snapshot();
            snapshot.dependent_care = None;
            snapshot.medical = None;
            snapshot.core.people[0].hunger = 5;
            let bread = snapshot.core.physical.goods.iter().find(|good| good.name == "bread").unwrap().id;
            let container = snapshot.core.physical.custody.containers.iter_mut()
                .find(|container| container.place == game_lots::Place::Person(2)).unwrap();
            *container = game_lots::Container::new(game_lots::Place::Person(2), 100,
                vec![game_lots::Lot { item: None, good: bread, quantity: 2, expires_at: None }]).unwrap();
            let giving = snapshot.giving.as_mut().unwrap();
            let policy = &mut giving.policies_mut()[0];
            policy.willing = willing;
            if pressured { policy.pressured_by = Some(1); policy.pressure_threshold = Some(2); }
            snapshot.opinions = Some(serde_json::from_value(serde_json::json!({
                "rules": {"minimum": -10, "maximum": 10, "initial": 0, "fulfilled_gain": 1, "default_loss": 1,
                    "gratitude_gain": 5, "affinity": {"limit": 10}},
                "sensitivities": [{"person": 1, "gratitude_gain": 0, "affinity_help": -2}],
                "records": if pressured { vec![serde_json::json!({"observer":2,"target":1,"trust":0,"fear":3})] } else { vec![] }
            })).unwrap());
            snapshot.health.as_mut().unwrap().owner.cases = vec![game_disease::Case {
                person: 1, strain: 1, infected_at: 0, source: None,
            }];
            let saved = serde_json::to_string(&snapshot).unwrap();
            let mut host = game_economy::core_interval::CoreHost::resume(snapshot).unwrap();
            let mut resumed = game_economy::core_interval::CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
            assert_eq!(host.advance_continuous(3_600_000, &[]).unwrap().failure, None);
            assert_eq!(resumed.advance_continuous(3_600_000, &[]).unwrap().failure, None);
            let result = host.snapshot();
            assert_eq!(result, resumed.snapshot());
            assert!(game_economy::core_interval::CoreHost::resume(result.clone()).is_ok());
            let shared = willing || pressured;
            let recipient = result.core.physical.custody.containers.iter()
                .find(|container| container.place == game_lots::Place::Person(1)).unwrap();
            assert_eq!(recipient.used, if shared { 2 } else { 0 });
            assert_eq!(result.giving.as_ref().unwrap().requests.is_empty(), shared);
            assert!(result.health.as_ref().unwrap().owner.cases.iter().any(|case| case.person == 2));
            let relation = result.opinions.as_ref().unwrap().records.iter()
                .find(|record| record.observer == 1 && record.target == 2);
            if shared { let relation = relation.unwrap(); assert_eq!((relation.affinity, relation.obligation), (-2, 0)); }
            else { assert!(relation.is_none()); }
            serde_json::to_value(result).unwrap()
        })
    }
    for (willing, pressured) in [(true, false), (false, false), (false, true)] {
        assert_eq!(run(1, willing, pressured), run(6, willing, pressured));
    }
}

#[test]
fn personal_practice_can_report_false_readiness_without_creating_goods() {
    fn run(workers: usize) -> serde_json::Value {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
            let root = root();
            let initial = game_economy_cli::core_scenario(
                &root.join("content/scenarios/economy-core.toml"), &root.join("content"),
            ).unwrap();
            let mut snapshot = initial.snapshot();
            snapshot.dependent_care = None;
            snapshot.medical = None;
            snapshot.core.people[0].hunger = 5;
            let bread = snapshot.core.physical.goods.iter().find(|good| good.name == "bread").unwrap().id;
            let proposal = serde_json::json!({"author":1,"sequence":1,"recipient":2,"good":bread,
                "quantity":2,"reserve":1,"ask_hunger":5,"created":0,"expires":5});
            let practices = serde_json::from_value(serde_json::json!({"minds":[
                {"person":1,"invent":false,"approve":true,"willing":true,"ask_willing":true,
                 "shortage_threshold":5,"minimum_shortages":1,"review_interval":2,"proposal_lifetime":8,
                 "sequence":1,"authored":[proposal.clone()],"report_attitude":"trust"},
                {"person":2,"invent":false,"approve":true,"willing":true,"shortage_threshold":5,
                 "minimum_shortages":1,"review_interval":2,"proposal_lifetime":8,"reporting":"lie",
                 "inbox":[{"proposal":proposal,"decision":"adopted","approved":true,"delivered":0}]}
            ]})).unwrap();
            snapshot.giving.as_mut().unwrap().practices = Some(practices);
            let saved = serde_json::to_string(&snapshot).unwrap();
            let mut host = game_economy::core_interval::CoreHost::resume(snapshot).unwrap();
            let mut resumed = game_economy::core_interval::CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
            assert_eq!(host.advance_continuous(3_600_000, &[]).unwrap().failure, None);
            assert_eq!(resumed.advance_continuous(3_600_000, &[]).unwrap().failure, None);
            let result = host.snapshot();
            assert_eq!(result, resumed.snapshot());
            assert!(game_economy::core_interval::CoreHost::resume(result.clone()).is_ok());
            let practices = result.giving.as_ref().unwrap().practices.as_ref().unwrap();
            assert!(practices.minds[1].sent_reports[0].claimed_ready);
            assert!(practices.minds[0].reports[0].statement.claimed_ready);
            let inventory = result.core.physical.custody.containers.iter()
                .find(|container| container.place == game_lots::Place::Person(2)).unwrap();
            assert_eq!(inventory.used, 0);
            let recipient = result.core.physical.custody.containers.iter()
                .find(|container| container.place == game_lots::Place::Person(1)).unwrap();
            assert_eq!(recipient.used, 0);
            let mut moved = result.clone();
            moved.core.people[0].region = Some(2);
            moved.core.physical.custody.containers.push(game_lots::Container::new(
                game_lots::Place::Stock { region: 2, good: bread }, 100, vec![],
            ).unwrap());
            moved.core.physical.custody.containers.sort_unstable_by_key(|row| row.place);
            let minds = &mut moved.giving.as_mut().unwrap().practices.as_mut().unwrap().minds;
            std::sync::Arc::make_mut(&mut minds[0].reports).clear();
            std::sync::Arc::make_mut(&mut minds[1].sent_reports).clear();
            moved.social = Some(game_economy::core_interval::CoreSocial {
                owner: game_social::Social {
                    inherit_births: false, rules: game_social::Rules { interval: 1, min_health: 1, max_hunger: 100,
                        familiarity_gain: 1, familiarity_max: 10, relationship_retention: None },
                    participants: vec![game_social::Participant { person: 1, willing: false }, game_social::Participant { person: 2, willing: false }], pressures: vec![], relationships: vec![],
                    events: vec![game_social::Encounter { tick: 1, region: 1, a: 1, b: 2,
                        pressured: None, also_pressured: None }], needs: None,
                }, last_tick: Some(1),
            });
            let mut moved = game_economy::core_interval::CoreHost::resume(moved).unwrap();
            assert_eq!(moved.advance_continuous(3_600_000, &[]).unwrap().failure, None);
            assert!(moved.snapshot().giving.unwrap().practices.unwrap().minds[1].sent_reports[0].claimed_ready);
            serde_json::to_value(result).unwrap()
        })
    }
    assert_eq!(run(1), run(6));
}

#[test]
fn authored_practice_can_lie_about_readiness() {
    let root = root();
    let mut host = game_economy_cli::core_scenario(
        &root.join("content/scenarios/economy-core-practices.toml"),
        &root.join("content"),
    )
    .unwrap();
    assert_eq!(
        host.advance_continuous(3_600_000, &[]).unwrap().failure,
        None
    );
    let state = host.snapshot();
    let practices = state.giving.unwrap().practices.unwrap();
    assert!(practices.minds[1].sent_reports[0].claimed_ready);
    assert!(practices.minds[0].reports[0].statement.claimed_ready);
}

#[test]
fn personal_taking_respects_resistance_and_subjective_reactions() {
    fn run(workers: usize, notices: bool, resist: bool, stocked: bool) -> serde_json::Value {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
            let root = root();
            let initial = game_economy_cli::core_scenario(&root.join("content/scenarios/economy-core.toml"), &root.join("content")).unwrap();
            let mut snapshot = initial.snapshot();
            snapshot.dependent_care = None;
            snapshot.family = None;
            let mut witness = snapshot.core.people[0].clone();
            witness.id = 3;
            snapshot.core.people.push(witness);
            snapshot.core.physical.custody.containers.push(game_lots::Container::new(game_lots::Place::Person(3), 100, vec![]).unwrap());
            snapshot.core.physical.custody.containers.sort_unstable_by_key(|container| container.place);
            snapshot.medical = None;
            snapshot.giving = None;
            snapshot.gathering = None;
            let bread = snapshot.core.physical.goods.iter().find(|good| good.name == "bread").unwrap().id;
            let target = snapshot.core.physical.custody.containers.iter_mut().find(|container| container.place == game_lots::Place::Person(1)).unwrap();
            *target = game_lots::Container::new(game_lots::Place::Person(1), 100, if stocked { vec![game_lots::Lot { item: None, good: bread, quantity: 2, expires_at: None }] } else { vec![] }).unwrap();
            snapshot.taking = Some(serde_json::from_value(serde_json::json!({
                "policies": [{"person":2,"target":1,"good":bread,"quantity":2,"willing":true,"interval":1}],
                "responses": [{"person":1,"notices":notices,"resist":resist}],
                "reactions": [{"person":1,"as_target":{"transferred":{"affinity":3},"resisted":{"affinity":-2}}}],
                "witnesses": [{"person":3,"actor":2,"target":1,"notices":true,"toward_actor":{"transferred":{"affinity":-4}}}]
            })).unwrap());
            snapshot.opinions = Some(serde_json::from_value(serde_json::json!({
                "rules": {"minimum":-10,"maximum":10,"initial":0,"fulfilled_gain":1,"default_loss":1,"affinity":{"limit":10}}
            })).unwrap());
            let saved = serde_json::to_string(&snapshot).unwrap();
            let mut host = game_economy::core_interval::CoreHost::resume(snapshot).unwrap();
            let mut restored = game_economy::core_interval::CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
            assert_eq!(host.advance_continuous(3_600_000, &[]).unwrap().failure, None);
            assert_eq!(restored.advance_continuous(3_600_000, &[]).unwrap().failure, None);
            let result = host.snapshot();
            assert_eq!(result, restored.snapshot());
            assert!(game_economy::core_interval::CoreHost::resume(result.clone()).is_ok());
            let event = &result.taking.as_ref().unwrap().events[0];
            let outcome = if notices && resist { "resisted" } else if stocked { "transferred" } else { "unavailable" };
            assert_eq!(serde_json::to_value(event.outcome).unwrap(), outcome);
            let received = result.core.physical.custody.containers.iter().find(|container| container.place == game_lots::Place::Person(2)).unwrap();
            assert_eq!(received.used, if outcome == "transferred" { 2 } else { 0 });
            assert!(result.taking.as_ref().unwrap().witnesses[0].last_seen.is_some());
            let witness_opinion = result.opinions.as_ref().unwrap().records.iter().find(|record| record.observer == 3 && record.target == 2);
            if outcome == "transferred" { assert_eq!(witness_opinion.unwrap().affinity, -4); } else { assert!(witness_opinion.is_none()); }
            let relationship = result.opinions.as_ref().unwrap().records.iter().find(|record| record.observer == 1 && record.target == 2);
            if notices && outcome == "transferred" { assert_eq!(relationship.unwrap().affinity, 3); }
            else if outcome == "resisted" { assert_eq!(relationship.unwrap().affinity, -2); }
            else { assert!(relationship.is_none()); }
            serde_json::to_value(result).unwrap()
        })
    }
    for (notices, resist, stocked) in [
        (false, true, true),
        (true, false, true),
        (true, true, true),
        (true, false, false),
    ] {
        assert_eq!(
            run(1, notices, resist, stocked),
            run(6, notices, resist, stocked)
        );
    }
}

#[test]
fn authored_taking_loads_named_goods() {
    let root = root();
    let host = game_economy_cli::core_scenario(
        &root.join("content/scenarios/economy-core-taking.toml"),
        &root.join("content"),
    )
    .unwrap();
    let state = host.snapshot();
    let bread = state
        .core
        .physical
        .goods
        .iter()
        .find(|good| good.name == "bread")
        .unwrap()
        .id;
    assert_eq!(state.taking.unwrap().policies[0].good, bread);
}

#[test]
fn food_request_can_carry_a_doubted_taking_account() {
    fn run(workers: usize) -> serde_json::Value {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
            let root = root();
            let initial = game_economy_cli::core_scenario(&root.join("content/scenarios/economy-core.toml"), &root.join("content")).unwrap();
            let mut snapshot = initial.snapshot();
            snapshot.dependent_care = None;
            snapshot.medical = None;
            snapshot.core.people[0].hunger = 5;
            let bread = snapshot.core.physical.goods.iter().find(|good| good.name == "bread").unwrap().id;
            snapshot.taking = Some(serde_json::from_value(serde_json::json!({
                "witnesses":[{"person":1,"actor":2,"target":1,"notices":true,
                    "last_seen":{"tick":0,"target":1,"good":bread,"quantity":1,"outcome":"transferred"}}],
                "report_policies":[{"person":1,"recipient":2,"actor":2,"willing":true,"claimed_outcome":"resisted"}],
                "receptions":[{"person":2,"stance":"doubt"}]
            })).unwrap());
            let saved = serde_json::to_string(&snapshot).unwrap();
            let mut host = game_economy::core_interval::CoreHost::resume(snapshot).unwrap();
            let mut restored = game_economy::core_interval::CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
            assert_eq!(host.advance_continuous(3_600_000, &[]).unwrap().failure, None);
            assert_eq!(restored.advance_continuous(3_600_000, &[]).unwrap().failure, None);
            let result = host.snapshot();
            assert_eq!(result, restored.snapshot());
            let taking = result.taking.as_ref().unwrap();
            let report = &taking.reports.get(2)[0];
            assert_eq!(serde_json::to_value(report.claimed_outcome).unwrap(), "resisted");
            assert_eq!(serde_json::to_value(report.belief).unwrap(), "doubts");
            assert_eq!(serde_json::to_value(taking.witnesses[0].last_seen.unwrap().outcome).unwrap(), "transferred");
            serde_json::to_value(result).unwrap()
        })
    }
    assert_eq!(run(1), run(6));
}

#[test]
fn authored_work_practice_loads_named_recipe_and_good() {
    let root = root();
    let host = game_economy_cli::core_scenario(
        &root.join("content/scenarios/economy-core-work-practices.toml"),
        &root.join("content"),
    )
    .unwrap();
    let snapshot = host.snapshot();
    let proposal = snapshot
        .giving
        .as_ref()
        .unwrap()
        .practices
        .as_ref()
        .unwrap()
        .minds[0]
        .work
        .as_ref()
        .unwrap()
        .authored
        .as_ref()
        .unwrap();
    assert_eq!(
        proposal.good,
        snapshot
            .core
            .physical
            .goods
            .iter()
            .find(|good| good.name == "bread")
            .unwrap()
            .id
    );
    assert_eq!(
        proposal.recipe,
        snapshot
            .core
            .physical
            .recipes
            .iter()
            .find(|recipe| recipe.name == "fixture-tool")
            .unwrap()
            .id
    );
}

#[test]
fn work_terms_emerge_from_personal_experience_and_can_be_adopted_under_pressure() {
    fn run(workers: usize) -> serde_json::Value {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
            let root = root();
            let initial = game_economy_cli::core_scenario(&root.join("content/scenarios/economy-core-work-practices.toml"), &root.join("content")).unwrap();
            let mut snapshot = initial.snapshot();
            snapshot.dependent_care = None;
            snapshot.medical = None;
            snapshot.gathering = None;
            snapshot.family = None;
            snapshot.core.people[0].hunger = 5;
            let bread = snapshot.core.physical.goods.iter().find(|good| good.name == "bread").unwrap().id;
            let recipe = snapshot.core.physical.recipes.iter().find(|recipe| recipe.name == "fixture-tool").unwrap().id;
            let requests = snapshot.autowork.as_mut().unwrap().requests.as_mut().unwrap();
            requests.next_id = 2;
            requests.memories = vec![serde_json::from_value(serde_json::json!({"observer":2,"worker":1,"recipe":recipe,"successes":1,"failures":0,"last_tick":0,"last_success_at":0,"last_success":{"request":1,"job":1}})).unwrap()];
            snapshot.opinions = Some(serde_json::from_value(serde_json::json!({"rules":{"minimum":-10,"maximum":10,"initial":0,"fulfilled_gain":1,"default_loss":1},"records":[{"observer":1,"target":2,"trust":0,"fear":2}]})).unwrap());
            snapshot.giving.as_mut().unwrap().practices = Some(serde_json::from_value(serde_json::json!({"minds":[
                {"person":1,"invent":false,"approve":false,"willing":false,"shortage_threshold":5,"minimum_shortages":1,"review_interval":2,"proposal_lifetime":8,
                 "work":{"invent":false,"approve":false,"willing":false,"ask_willing":false,"review_interval":2,"proposal_lifetime":8,"pressure_threshold":1}},
                {"person":2,"invent":false,"approve":false,"willing":false,"shortage_threshold":5,"minimum_shortages":1,"review_interval":2,"proposal_lifetime":8,
                 "work":{"invent":true,"approve":false,"willing":false,"ask_willing":false,"review_interval":2,"proposal_lifetime":8,
                         "gifts":[{"tick":0,"other":1,"good":bread,"quantity":1,"given":true}]}}
            ]})).unwrap());
            let mut host = game_economy::core_interval::CoreHost::resume(snapshot).unwrap();
            assert_eq!(host.advance_continuous(3_600_000, &[]).unwrap().failure, None);
            let first = host.snapshot();
            let minds = &first.giving.as_ref().unwrap().practices.as_ref().unwrap().minds;
            let proposal = minds[1].work.as_ref().unwrap().authored.as_ref().unwrap();
            assert_eq!((proposal.author, proposal.worker, proposal.recipe, proposal.good), (2, 1, recipe, bread));
            assert_eq!(serde_json::to_value(minds[0].work.as_ref().unwrap().inbox[0].decision).unwrap(), "pending");
            let saved = serde_json::to_string(&first).unwrap();
            let mut restored = game_economy::core_interval::CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
            assert_eq!(host.advance_continuous(3_600_000, &[]).unwrap().failure, None);
            assert_eq!(restored.advance_continuous(3_600_000, &[]).unwrap().failure, None);
            let result = host.snapshot();
            assert_eq!(result, restored.snapshot());
            let reception = &result.giving.as_ref().unwrap().practices.as_ref().unwrap().minds[0].work.as_ref().unwrap().inbox[0];
            assert!(!reception.approved);
            assert_eq!(serde_json::to_value(reception.decision).unwrap(), "adopted");
            serde_json::to_value(result).unwrap()
        })
    }
    assert_eq!(run(1), run(6));
}

#[test]
fn authored_skill_catalog_loads_into_the_interval_host() {
    let root = root();
    let host = game_economy_cli::core_scenario(
        &root.join("content/scenarios/economy-core-skills.toml"),
        &root.join("content"),
    )
    .unwrap();
    let snapshot = host.snapshot();
    let recipe = snapshot
        .core
        .physical
        .recipes
        .iter()
        .find(|recipe| recipe.name == "fixture-tool")
        .unwrap()
        .id;
    let skills = &snapshot.skills.unwrap().owner;
    assert!(
        skills.rules.iter().any(|rule| rule.recipe == recipe)
            || skills.aliases.iter().any(|alias| alias.recipe == recipe)
    );
}

#[test]
fn newborn_inherits_teaching_inclination_without_skill() {
    fn run(workers: usize) -> serde_json::Value {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let root = root();
                let initial = game_economy_cli::core_scenario(
                    &root.join("content/scenarios/economy-core-skills.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let mut snapshot = initial.snapshot();
                let owner = &mut snapshot.skills.as_mut().unwrap().owner;
                let recipe = owner.rules[0].recipe;
                owner.inherit_teachers = true;
                owner.teachers.push(game_economy_skills::TeacherPolicy {
                    person: 1,
                    recipe,
                    enabled: true,
                    eligible_at: 0,
                });
                let mut host = game_economy::core_interval::CoreHost::resume(snapshot).unwrap();
                assert_eq!(
                    host.advance_continuous(3_600_000, &[]).unwrap().failure,
                    None
                );
                let state = host.snapshot();
                let skills = &state.skills.as_ref().unwrap().owner;
                let inherited = skills
                    .teachers
                    .iter()
                    .find(|teacher| teacher.person == 3 && teacher.recipe == recipe)
                    .unwrap();
                assert!(inherited.enabled);
                assert_eq!(inherited.eligible_at, 13);
                assert!(skills.records.iter().all(|record| record.person != 3));
                assert!(skills.lessons.iter().all(|lesson| lesson.student != 3));
                let saved = serde_json::to_string(&state).unwrap();
                let restored = game_economy::core_interval::CoreHost::resume(
                    serde_json::from_str(&saved).unwrap(),
                )
                .unwrap();
                assert_eq!(restored.snapshot(), state);
                serde_json::to_value(state).unwrap()
            })
    }
    assert_eq!(run(1), run(6));
}

#[test]
fn authored_personality_is_inherited_only_at_real_births() {
    fn run(workers: usize) -> serde_json::Value {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let root = root();
                let mut host = game_economy_cli::core_scenario(
                    &root.join("content/scenarios/economy-core-personality.toml"),
                    &root.join("content"),
                )
                .unwrap();
                assert_eq!(
                    host.snapshot().personality.as_ref().unwrap().people.len(),
                    2
                );
                assert_eq!(
                    host.advance_continuous(3_600_000, &[]).unwrap().failure,
                    None
                );
                let state = host.snapshot();
                let child = state
                    .personality
                    .as_ref()
                    .unwrap()
                    .people
                    .iter()
                    .find(|row| row.person == 3)
                    .unwrap();
                assert_eq!((child.sociability, child.diligence), (500, 500));
                let saved = serde_json::to_string(&state).unwrap();
                assert_eq!(
                    game_economy::core_interval::CoreHost::resume(
                        serde_json::from_str(&saved).unwrap()
                    )
                    .unwrap()
                    .snapshot(),
                    state
                );
                serde_json::to_value(state).unwrap()
            })
    }
    assert_eq!(run(1), run(6));
}

#[test]
fn authored_aspiration_resolves_named_recipe_and_forms_a_goal() {
    let root = root();
    let mut host = game_economy_cli::core_scenario(
        &root.join("content/scenarios/economy-core-aspirations.toml"),
        &root.join("content"),
    )
    .unwrap();
    let recipe = host
        .snapshot()
        .core
        .physical
        .recipes
        .iter()
        .find(|r| r.name == "fixture-tool")
        .unwrap()
        .id;
    assert_eq!(
        host.snapshot().dreams.as_ref().unwrap().people[0].recipe,
        recipe
    );
    assert_eq!(
        host.advance_continuous(3_600_000, &[]).unwrap().failure,
        None
    );
    let state = host.snapshot();
    assert!(
        state
            .dreams
            .as_ref()
            .unwrap()
            .learning
            .iter()
            .any(|rule| rule.person == 3)
    );
    assert!(
        !state
            .dreams
            .as_ref()
            .unwrap()
            .people
            .iter()
            .any(|dream| dream.person == 3)
    );
    assert!(
        host.snapshot()
            .goals
            .as_ref()
            .unwrap()
            .people
            .iter()
            .any(|goal| goal.person == 1 && goal.recipe == recipe && goal.target == 2)
    );
}

#[test]
fn authored_shared_work_resolves_recipe_and_produces_real_output() {
    fn run(workers: usize) -> serde_json::Value {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
            let root = root();
            let mut host = game_economy_cli::core_scenario(&root.join("content/scenarios/economy-core-shared-work.toml"), &root.join("content")).unwrap();
            let result = host.advance_continuous(1_800_000, &[]).unwrap();
            assert_eq!(result.failure,None);
            assert!(result.events.iter().any(|event| matches!(event, game_economy::core_interval::CoreRunEvent::Assists(people) if people == &[1])));
            assert!(result.events.iter().any(|event| matches!(event, game_economy::core_interval::CoreRunEvent::Work(work) if work.finished == [1])));
            assert_eq!(host.advance_continuous(1_800_000, &[]).unwrap().failure,None);
            let state = host.snapshot();
            let tool = state.core.physical.goods.iter().find(|good| good.name == "tool").unwrap().id;
            assert_eq!(state.core.physical.custody.containers.iter().find(|row| row.place == (game_lots::Place::Stock {region:1,good:tool})).unwrap().used,1);
            serde_json::to_value(state).unwrap()
        })
    }
    assert_eq!(run(1), run(6));
}

#[test]
fn authored_succession_keeps_real_site_and_inputs_until_personal_takeover() {
    fn run(workers: usize) -> serde_json::Value {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
            let root = root();
            let mut host = game_economy_cli::core_scenario(&root.join("content/scenarios/economy-core-succession.toml"), &root.join("content")).unwrap();
            let first=host.advance_scheduled(900_000,&[]).unwrap(); assert_eq!(first.failure,None);
            assert!(first.events.iter().any(|event| matches!(event, game_economy::core_interval::CoreRunEvent::Assists(people) if people == &[1])));
            host.suspend_shared_work(&[(1,2)]).unwrap();
            let state=host.snapshot();
            assert!(matches!(state.core.physical.work[0].phase, game_lot_runtime::Phase::Suspended {..}));
            assert_eq!(state.sites.as_ref().unwrap().facilities.bindings.len(),1);
            let saved=serde_json::to_string(&state).unwrap();
            let mut restored=game_economy::core_interval::CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
            let chosen=host.take_over_work_autonomously(&[],1).unwrap();
            assert_eq!(chosen.len(),1); assert_eq!(chosen[0].worker,1);
            assert_eq!(restored.take_over_work_autonomously(&[],1).unwrap(),chosen);
            assert_eq!(host.snapshot(),restored.snapshot());
            serde_json::to_value(host.snapshot()).unwrap()
        })
    }
    assert_eq!(run(1), run(6));
}

#[test]
fn authored_personal_exchange_resolves_named_goods_and_real_custody() {
    fn run(workers: usize) -> serde_json::Value {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
            let root=root(); let mut host=game_economy_cli::core_scenario(&root.join("content/scenarios/economy-core-exchange.toml"),&root.join("content")).unwrap();
            let state=host.snapshot(); let grain=state.core.physical.goods.iter().find(|good|good.name=="grain").unwrap().id;
            let bread=state.core.physical.goods.iter().find(|good|good.name=="bread").unwrap().id;
            let offer=&state.market.as_ref().unwrap().offers[0]; assert_eq!((offer.give[0].good,offer.receive[0].good),(grain,bread));
            let result=host.advance_scheduled(0,&[]).unwrap(); assert_eq!(result.failure,None);
            assert!(result.events.iter().any(|event| matches!(event,game_economy::core_interval::CoreRunEvent::Exchanges(ids) if ids == &[1])));
            let state=host.snapshot();
            let a=state.core.physical.custody.containers.iter().find(|row|row.place==game_lots::Place::Person(1)).unwrap();
            let b=state.core.physical.custody.containers.iter().find(|row|row.place==game_lots::Place::Person(2)).unwrap();
            assert_eq!(a.lots.iter().filter(|lot|lot.good==bread).map(|lot|lot.quantity).sum::<u64>(),1);
            assert_eq!(b.lots.iter().filter(|lot|lot.good==grain).map(|lot|lot.quantity).sum::<u64>(),1);
            serde_json::to_value(state).unwrap()
        })
    }
    assert_eq!(run(1), run(6));
}

#[test]
fn authored_text_writing_consumes_supplies_and_creates_a_real_page() {
    fn run(workers: usize) -> serde_json::Value {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
            let root=root(); let mut host=game_economy_cli::core_scenario(&root.join("content/scenarios/economy-core-writing.toml"),&root.join("content")).unwrap();
            let result=host.advance_scheduled(0,&[]).unwrap(); assert_eq!(result.failure,None);
            assert!(result.events.iter().any(|event|matches!(event,game_economy::core_interval::CoreRunEvent::WrittenPages(ids) if ids == &[1])));
            assert!(result.events.iter().any(|event|matches!(event,game_economy::core_interval::CoreRunEvent::PracticedWriting(ids) if ids == &[1])));
            let state=host.snapshot(); let writing=state.writing.as_ref().unwrap();
            assert_eq!(writing.pages[0].text.as_deref(),Some("The river is cold."));
            assert!(writing.text_memories.iter().any(|memory| memory.person==2 && &*memory.text=="The river is cold."));
            let page=writing.page; let physical=state.core.physical.custody.containers.iter().find(|row|row.place==game_lots::Place::Person(2)).unwrap();
            assert!(physical.lots.iter().any(|lot|lot.good==page && lot.item==Some(1)));
            let saved=serde_json::to_string(&state).unwrap(); assert_eq!(game_economy::core_interval::CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap().snapshot(),state);
            serde_json::to_value(state).unwrap()
        })
    }
    assert_eq!(run(1), run(6));
}

#[test]
fn authored_written_claim_can_misstate_a_memory() {
    fn run(workers: usize) -> serde_json::Value {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
            let root = root();
            let mut host = game_economy_cli::core_scenario(&root.join("content/scenarios/economy-core-written-claim.toml"), &root.join("content")).unwrap();
            let result = host.advance_scheduled(0, &[]).unwrap();
            assert_eq!(result.failure, None);
            assert!(result.events.iter().any(|event| matches!(event, game_economy::core_interval::CoreRunEvent::WrittenPages(ids) if ids == &[1])));
            let state = host.snapshot();
            assert_eq!(state.writing.as_ref().unwrap().pages[0].claim.as_ref().unwrap().food, 13);
            assert_eq!(state.knowledge.as_ref().unwrap().minds[0].records[0].food, 20);
            serde_json::to_value(state).unwrap()
        })
    }
    assert_eq!(run(1), run(6));
}

#[test]
fn authored_local_carrying_moves_goods_through_personal_custody() {
    fn run(workers: usize) -> serde_json::Value {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(||{
            let root=root();let mut host=game_economy_cli::core_scenario(&root.join("content/scenarios/economy-core-local-carry.toml"),&root.join("content")).unwrap();
            let result=host.advance_scheduled(0,&[]).unwrap();assert_eq!(result.failure,None);
            assert!(result.events.iter().any(|event|matches!(event,game_economy::core_interval::CoreRunEvent::LocalCarries(rows) if rows.iter().any(|row|row.person==1 && row.moved))));
            let state=host.snapshot();let local=state.local_transport.as_ref().unwrap();assert_eq!(local.events.len(),1);
            let person=state.core.physical.custody.containers.iter().find(|row|row.place==game_lots::Place::Person(1)).unwrap();
            assert_eq!(person.lots.iter().map(|lot|lot.quantity).sum::<u64>(),2);
            let mut restored=game_economy::core_interval::CoreHost::resume(serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap()).unwrap();
            assert_eq!(host.advance_continuous(900000,&[]).unwrap(),restored.advance_continuous(900000,&[]).unwrap());
            let state=host.snapshot();assert_eq!(state,restored.snapshot());
            let store=state.core.physical.custody.containers.iter().find(|row|row.place==game_lots::Place::Inventory(1)).unwrap();
            assert_eq!(store.lots.iter().map(|lot|lot.quantity).sum::<u64>(),2);
            serde_json::to_value(state).unwrap()
        })
    }
    assert_eq!(run(1), run(6));
}

#[test]
fn authored_builder_creates_a_site_from_physical_materials() {
    fn run(workers: usize) -> serde_json::Value {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(||{
            let root=root();let mut host=game_economy_cli::core_scenario(&root.join("content/scenarios/economy-core-construction.toml"),&root.join("content")).unwrap();
            let result=host.advance_scheduled(0,&[]).unwrap();assert_eq!(result.failure,None);
            assert!(result.events.iter().any(|event|matches!(event,game_economy::core_interval::CoreRunEvent::Construction(rows) if rows.iter().any(|row|row.person==1 && row.site==Some(1)))));
            let state=host.snapshot();assert_eq!(state.sites.as_ref().unwrap().facilities.sites.len(),1);
            let site=state.core.physical.custody.containers.iter().find(|row|row.place==game_lots::Place::Site(1)).unwrap();assert_eq!(site.lots.iter().map(|lot|lot.quantity).sum::<u64>(),1);
            assert_eq!(state.construction.as_ref().unwrap().materials.len(),1);
            assert_eq!(game_economy::core_interval::CoreHost::resume(serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap()).unwrap().snapshot(),state);
            serde_json::to_value(state).unwrap()
        })
    }
    assert_eq!(run(1), run(6));
}

#[test]
fn authored_hauler_picks_up_travels_and_delivers() {
    fn run(workers: usize) -> serde_json::Value {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(||{
            let root=root();let mut host=game_economy_cli::core_scenario(&root.join("content/scenarios/economy-core-hauling.toml"),&root.join("content")).unwrap();
            let first=host.advance_continuous(0,&[]).unwrap();assert_eq!(first.failure,None);
            assert!(first.events.iter().any(|event|matches!(event,game_economy::core_interval::CoreRunEvent::HaulingTransfers(rows) if rows.iter().any(|row|row.accepted))));
            let state=host.snapshot();let mut restored=game_economy::core_interval::CoreHost::resume(serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap()).unwrap();
            let result=host.advance_continuous(3000,&[]).unwrap();let resumed=restored.advance_continuous(3000,&[]).unwrap();assert_eq!(result,resumed);assert_eq!(result.failure,None);
            let state=host.snapshot();assert_eq!(state,restored.snapshot());let grain=state.core.physical.goods.iter().find(|good|good.name=="grain").unwrap().id;
            let destination=state.core.physical.custody.containers.iter().find(|row|row.place==game_lots::Place::Stock{region:2,good:grain}).unwrap();assert_eq!(destination.lots.iter().map(|lot|lot.quantity).sum::<u64>(),2);
            serde_json::to_value(state).unwrap()
        })
    }
    assert_eq!(run(1), run(6));
}

#[test]
fn authored_hauling_request_leaves_the_carrier_free_to_choose() {
    fn run(workers: usize) -> serde_json::Value {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(||{
            let root=root();let mut host=game_economy_cli::core_scenario(&root.join("content/scenarios/economy-core-hauling-request.toml"),&root.join("content")).unwrap();
            let first=host.advance_continuous(0,&[]).unwrap();assert_eq!(first.failure,None);
            assert!(first.events.iter().any(|event|matches!(event,game_economy::core_interval::CoreRunEvent::HaulingRequests(people) if people==&[2])));
            assert!(first.events.iter().any(|event|matches!(event,game_economy::core_interval::CoreRunEvent::HaulingTransfers(rows) if rows.iter().any(|row|row.accepted))));
            let saved=host.snapshot();let mut restored=game_economy::core_interval::CoreHost::resume(serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap()).unwrap();
            assert_eq!(host.advance_continuous(3000,&[]).unwrap(),restored.advance_continuous(3000,&[]).unwrap());let state=host.snapshot();assert_eq!(state,restored.snapshot());
            let grain=state.core.physical.goods.iter().find(|good|good.name=="grain").unwrap().id;let destination=state.core.physical.custody.containers.iter().find(|row|row.place==game_lots::Place::Stock{region:2,good:grain}).unwrap();
            assert_eq!(destination.lots.iter().map(|lot|lot.quantity).sum::<u64>(),2);serde_json::to_value(state).unwrap()
        })
    }
    assert_eq!(run(1), run(6));
}
