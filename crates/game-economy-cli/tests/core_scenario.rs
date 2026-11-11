use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn authored_mechanics_loads_content_and_uses_identified_parts() {
    let root = root();
    let path = root.join("content/scenarios/economy-core-mechanics.toml");
    assert!(game_economy_cli::scenario(&path, &root.join("content")).is_err());
    let mut host = game_economy_cli::core_scenario(&path, &root.join("content")).unwrap();
    let request = game_mechanics::Experiment {
        actor: 1,
        apparatus: 1,
        output_part: 1002,
        input_force: 100,
        input_distance: 100,
        load_force: 20,
    };
    assert_eq!(
        host.experiment_mechanics(&[request], &[]).unwrap()[0].outcome,
        game_mechanics::Outcome::Moved
    );
    let saved = serde_json::to_string(&host.snapshot()).unwrap();
    let restored =
        game_economy::core_interval::CoreHost::resume(serde_json::from_str(&saved).unwrap())
            .unwrap();
    assert_eq!(host.snapshot(), restored.snapshot());
}

#[test]
fn work_intentions_compete_with_carrying_and_share_scarce_inputs() {
    use game_economy::core_interval::{CoreChoiceOutcome, CoreHost, CoreRunEvent, DecisionPhase};
    use game_lots::{Container, Lot, Place};
    let root = root();
    let host = game_economy_cli::core_scenario(
        &root.join("content/scenarios/economy-core-personal-choices.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut snapshot = host.snapshot();
    let grain = snapshot
        .core
        .physical
        .goods
        .iter()
        .find(|good| good.name == "grain")
        .unwrap()
        .id;
    let output = snapshot
        .core
        .physical
        .goods
        .iter()
        .find(|good| good.id != grain)
        .unwrap()
        .id;
    let recipe = snapshot
        .core
        .physical
        .recipes
        .iter()
        .map(|recipe| recipe.id)
        .max()
        .unwrap_or(0)
        + 1;
    snapshot
        .core
        .physical
        .recipes
        .push(game_production::Recipe {
            id: recipe,
            name: "choice work".into(),
            ticks: 2,
            inputs: vec![game_production::Amount {
                good: grain,
                quantity: 2,
            }],
            outputs: vec![game_production::Amount {
                good: output,
                quantity: 1,
            }],
        });
    for person in &mut snapshot.core.people {
        person.willing_to_work = true;
    }
    snapshot.autowork = Some(game_autowork::Autowork {
        next_id: 1,
        policies: (1..=3)
            .map(|person| game_autowork::Policy {
                person,
                recipe,
                enabled: true,
                disregard_claims: false,
            })
            .collect(),
        requests: None,
    });
    let profiles = std::sync::Arc::make_mut(
        snapshot
            .schedule
            .as_mut()
            .unwrap()
            .personal_choices
            .as_mut()
            .unwrap(),
    );
    for person in profiles.iter_mut() {
        person.preferences.push(game_personal_choice::Preference {
            domain: 4,
            enabled: true,
            bias: 100,
            weight: 1000,
        });
    }
    profiles[2].idle = 1000;
    let stock = snapshot
        .core
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|container| {
            container.place
                == Place::Stock {
                    region: 1,
                    good: grain,
                }
        })
        .unwrap();
    *stock = Container::new(
        stock.place,
        100,
        vec![Lot {
            item: None,
            good: grain,
            quantity: 2,
            expires_at: None,
        }],
    )
    .unwrap();
    let output_place = Place::Stock {
        region: 1,
        good: output,
    };
    if !snapshot
        .core
        .physical
        .custody
        .containers
        .iter()
        .any(|container| container.place == output_place)
    {
        snapshot
            .core
            .physical
            .custody
            .containers
            .push(Container::new(output_place, 100, vec![]).unwrap());
        snapshot
            .core
            .physical
            .custody
            .containers
            .sort_by_key(|container| container.place);
    }
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = CoreHost::resume(snapshot.clone()).unwrap();
                let (intentions, _) = host.preview_personal_choices(&[]).unwrap();
                assert_eq!(
                    intentions
                        .iter()
                        .filter(|intent| intent.key.domain == 4)
                        .count(),
                    2
                );
                let mut pending = snapshot.clone();
                pending.schedule.as_mut().unwrap().phase = DecisionPhase::ChosenWork {
                    meal_people: vec![],
                    intents: std::sync::Arc::new(intentions),
                    construction_after: true,
                    carrying_after: true,
                    maintenance_after: false,
                };
                let mut stale = pending.clone();
                stale.autowork.as_mut().unwrap().policies[0].enabled = false;
                let mut stale = CoreHost::resume(stale).unwrap();
                let stale_run = stale.advance_continuous(1, &[]).unwrap();
                assert_eq!(stale_run.failure, None);
                assert!(stale_run.events.iter().any(|event| matches!(event,
                    CoreRunEvent::PersonalChoices(rows) if rows.iter().any(|row|
                        row.actor == 1 && row.outcome == CoreChoiceOutcome::Stale))));
                assert!(
                    stale
                        .snapshot()
                        .core
                        .physical
                        .work
                        .iter()
                        .all(|job| job.job.worker != 1)
                );
                let saved = serde_json::to_string(&pending).unwrap();
                let mut resumed = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                assert_eq!(resumed.advance_continuous(1, &[]).unwrap().failure, None);
                let run = host.advance_continuous(1, &[]).unwrap();
                assert_eq!(run.failure, None);
                let decisions: Vec<_> = run
                    .events
                    .iter()
                    .filter_map(|event| match event {
                        CoreRunEvent::PersonalChoices(rows) => Some(rows),
                        _ => None,
                    })
                    .flatten()
                    .filter(|event| event.key.is_some_and(|key| key.domain == 4))
                    .collect();
                assert_eq!(decisions.len(), 2);
                assert_eq!(
                    decisions
                        .iter()
                        .filter(|event| event.outcome == CoreChoiceOutcome::Accepted)
                        .count(),
                    1
                );
                assert_eq!(
                    decisions
                        .iter()
                        .filter(|event| event.outcome == CoreChoiceOutcome::Denied)
                        .count(),
                    1
                );
                assert_eq!(host.snapshot(), resumed.snapshot());
                host.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
    let mut alternative = snapshot;
    let stock = alternative
        .core
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|container| {
            container.place
                == Place::Stock {
                    region: 1,
                    good: grain,
                }
        })
        .unwrap();
    *stock = Container::new(
        stock.place,
        100,
        vec![Lot {
            item: None,
            good: grain,
            quantity: 20,
            expires_at: None,
        }],
    )
    .unwrap();
    let people = std::sync::Arc::make_mut(
        alternative
            .schedule
            .as_mut()
            .unwrap()
            .personal_choices
            .as_mut()
            .unwrap(),
    );
    people[1]
        .preferences
        .iter_mut()
        .find(|preference| preference.domain == 2)
        .unwrap()
        .bias = 200;
    let mut host = CoreHost::resume(alternative).unwrap();
    let (intentions, _) = host.preview_personal_choices(&[]).unwrap();
    assert_eq!(
        intentions
            .iter()
            .find(|intent| intent.actor == 1)
            .unwrap()
            .key
            .domain,
        4
    );
    assert_eq!(
        intentions
            .iter()
            .find(|intent| intent.actor == 2)
            .unwrap()
            .key
            .domain,
        2
    );
    let result = host.advance_continuous(1, &[]).unwrap();
    assert_eq!(result.failure, None);
    assert!(
        result
            .events
            .iter()
            .any(|event| matches!(event, CoreRunEvent::LocalCarries(rows)
        if rows.iter().any(|row| row.person == 2 && row.moved)))
    );
    assert!(
        host.snapshot()
            .core
            .physical
            .work
            .iter()
            .any(|job| job.job.worker == 1)
    );
    assert!(
        host.snapshot()
            .core
            .physical
            .work
            .iter()
            .all(|job| job.job.worker != 2)
    );
}

#[test]
fn people_independently_choose_repairs_before_site_contention() {
    use game_economy::core_interval::{CoreChoiceOutcome, CoreHost, CoreRunEvent, DecisionPhase};
    use game_economy_building::maintenance::{Condition, Maintenance, Policy, Rule};
    use game_economy_work::sites::Site;
    use game_lots::{Container, Lot, Place};

    let root = root();
    let host = game_economy_cli::core_scenario(
        &root.join("content/scenarios/economy-core-personal-choices.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut snapshot = host.snapshot();
    let good = snapshot
        .core
        .physical
        .goods
        .iter()
        .find(|good| good.name == "grain")
        .unwrap()
        .id;
    for id in 1..=2 {
        let bag = snapshot
            .core
            .physical
            .custody
            .containers
            .iter_mut()
            .find(|container| container.place == Place::Person(id))
            .unwrap();
        *bag = Container::new(
            Place::Person(id),
            10,
            vec![Lot {
                item: None,
                good,
                quantity: 2,
                expires_at: None,
            }],
        )
        .unwrap();
    }
    let sites = snapshot.sites.as_mut().unwrap();
    sites.facilities.next_id = 2;
    sites.facilities.sites.push(Site {
        id: 1,
        region: 1,
        kind: "mill".into(),
        capacity: 1,
    });
    sites.maintenance = Some(Maintenance {
        rules: vec![Rule {
            kind: "mill".into(),
            wear_limit: 10,
            wear_per_slot: 1,
            repair_good: good,
            repair_quantity: 1,
        }],
        conditions: vec![Condition { site: 1, wear: 5 }],
        policies: (1..=2)
            .map(|person| Policy {
                person,
                site: 1,
                willing: true,
                pressured_by: None,
                pressure_threshold: None,
                disregard_claims: false,
            })
            .collect(),
        events: vec![],
    });
    let profiles = std::sync::Arc::make_mut(
        snapshot
            .schedule
            .as_mut()
            .unwrap()
            .personal_choices
            .as_mut()
            .unwrap(),
    );
    for profile in &mut profiles[..2] {
        profile.preferences.push(game_personal_choice::Preference {
            domain: 3,
            enabled: true,
            bias: 100,
            weight: 1000,
        });
    }
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = CoreHost::resume(snapshot.clone()).unwrap();
                let (intentions, _) = host.preview_personal_choices(&[]).unwrap();
                assert_eq!(
                    intentions
                        .iter()
                        .filter(|intent| intent.key.domain == 3)
                        .count(),
                    2
                );
                let mut pending = snapshot.clone();
                pending.schedule.as_mut().unwrap().phase = DecisionPhase::ChosenMaintenance {
                    meal_people: vec![],
                    intents: std::sync::Arc::new(intentions),
                    construction_after: true,
                    carrying_after: true,
                    work_after: false,
                };
                let saved = serde_json::to_string(&pending).unwrap();
                let mut resumed = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                assert_eq!(resumed.advance_continuous(1, &[]).unwrap().failure, None);
                let result = host.advance_continuous(1, &[]).unwrap();
                assert_eq!(result.failure, None);
                let choices: Vec<_> = result
                    .events
                    .iter()
                    .filter_map(|event| match event {
                        CoreRunEvent::PersonalChoices(rows) => Some(rows),
                        _ => None,
                    })
                    .flatten()
                    .filter(|row| row.key.is_some_and(|key| key.domain == 3))
                    .collect();
                assert_eq!(choices.len(), 2);
                assert_eq!(
                    choices
                        .iter()
                        .filter(|row| row.outcome == CoreChoiceOutcome::Accepted)
                        .count(),
                    1
                );
                assert_eq!(
                    choices
                        .iter()
                        .filter(|row| row.outcome == CoreChoiceOutcome::Denied)
                        .count(),
                    1
                );
                assert_eq!(
                    host.snapshot()
                        .sites
                        .as_ref()
                        .unwrap()
                        .maintenance
                        .as_ref()
                        .unwrap()
                        .conditions[0]
                        .wear,
                    0
                );
                assert_eq!(host.snapshot(), resumed.snapshot());
                host.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
    let mut refusal = snapshot;
    let profiles = std::sync::Arc::make_mut(
        refusal
            .schedule
            .as_mut()
            .unwrap()
            .personal_choices
            .as_mut()
            .unwrap(),
    );
    for person in &mut profiles[..2] {
        person.preferences.last_mut().unwrap().enabled = false;
    }
    let mut host = CoreHost::resume(refusal).unwrap();
    assert_eq!(host.advance_continuous(1, &[]).unwrap().failure, None);
    assert_eq!(
        host.snapshot()
            .sites
            .unwrap()
            .maintenance
            .unwrap()
            .conditions[0]
            .wear,
        5
    );
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
fn hourly_history_food_supply_supports_founders_and_is_worker_deterministic() {
    fn run(workers: usize) -> serde_json::Value {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let root = root();
                let mut host = game_economy_cli::core_scenario(
                    &root.join("content/scenarios/economy-history-hourly.toml"),
                    &root.join("content"),
                )
                .unwrap();
                // The earlier hourly configuration exhausted food and began
                // losing people before hour 1,000, after its initial stocks ran out.
                for _ in 0..1_200 {
                    let result = host.advance_continuous(3_600_000, &[]).unwrap();
                    assert_eq!(result.failure, None);
                    assert_eq!(result.boundaries, 1);
                }
                let snapshot = host.snapshot();
                assert!(snapshot.core.people.iter().filter(|p| p.health > 0).count() >= 55);
                // Hunting is enabled, so some founder animals may be killed.
                assert!(
                    snapshot
                        .fauna
                        .as_ref()
                        .unwrap()
                        .animals
                        .iter()
                        .filter(|a| a.health > 0)
                        .count()
                        >= 10
                );
                serde_json::to_value(snapshot).unwrap()
            })
    }
    assert_eq!(run(1), run(4));
}

#[test]
fn archiving_opinion_events_preserves_decisions_and_every_event() {
    rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap()
        .install(|| {
            let root = root();
            let scenario = root.join("content/scenarios/economy-history-hourly.toml");
            let mut retained =
                game_economy_cli::core_scenario(&scenario, &root.join("content")).unwrap();
            let mut archived =
                game_economy_cli::core_scenario(&scenario, &root.join("content")).unwrap();
            let mut events = Vec::new();
            for _ in 0..168 {
                assert!(
                    retained
                        .advance_continuous(3_600_000, &[])
                        .unwrap()
                        .failure
                        .is_none()
                );
                assert!(
                    archived
                        .advance_continuous(3_600_000, &[])
                        .unwrap()
                        .failure
                        .is_none()
                );
                events.extend(archived.take_opinion_events());
            }
            let mut original = retained.snapshot();
            let mut expected = std::mem::take(&mut original.opinions.as_mut().unwrap().events);
            assert!(!events.is_empty());
            let key = |event: &game_opinions::Event| {
                (
                    event.tick,
                    event.observer,
                    event.target,
                    event.before,
                    event.after,
                )
            };
            events.sort_by_key(key);
            expected.sort_by_key(key);
            assert_eq!(events, expected);
            assert_eq!(original, archived.snapshot());
        });
}

#[test]
fn authored_language_runs_in_meetings_and_survives_resume() {
    fn run(workers: usize) -> serde_json::Value {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let root = root();
                let path = root.join("content/scenarios/economy-history-hourly.toml");
                assert!(game_economy_cli::scenario(&path, &root.join("content")).is_err());
                let mut host =
                    game_economy_cli::core_scenario(&path, &root.join("content")).unwrap();
                for _ in 0..20 {
                    assert!(
                        host.advance_continuous(3_600_000, &[])
                            .unwrap()
                            .failure
                            .is_none()
                    );
                }
                let snapshot = host.snapshot();
                let language = snapshot.social.as_ref().unwrap().language.as_ref().unwrap();
                assert!(language.minds.iter().any(|mind| !mind.words.is_empty()));
                let saved = serde_json::to_string(&snapshot).unwrap();
                let mut resumed = game_economy::core_interval::CoreHost::resume(
                    serde_json::from_str(&saved).unwrap(),
                )
                .unwrap();
                for _ in 0..5 {
                    assert!(
                        host.advance_continuous(3_600_000, &[])
                            .unwrap()
                            .failure
                            .is_none()
                    );
                    assert!(
                        resumed
                            .advance_continuous(3_600_000, &[])
                            .unwrap()
                            .failure
                            .is_none()
                    );
                }
                assert_eq!(host.snapshot(), resumed.snapshot());
                serde_json::to_value(host.snapshot()).unwrap()
            })
    }
    assert_eq!(run(1), run(4));
}

#[test]
fn people_choose_between_competing_physical_actions_or_idleness() {
    use game_economy::core_interval::{CoreChoiceOutcome, CoreRunEvent};
    fn run(workers: usize) -> serde_json::Value {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let root = root();
                let path = root.join("content/scenarios/economy-core-personal-choices.toml");
                assert!(game_economy_cli::scenario(&path, &root.join("content")).is_err());
                let mut host =
                    game_economy_cli::core_scenario(&path, &root.join("content")).unwrap();
                let (selected, _) = host.preview_personal_choices(&[]).unwrap();
                let carrying = selected.iter().find(|intent| intent.actor == 2).unwrap().movement.unwrap();
                let result = host.advance_continuous(900_000, &[]).unwrap();
                assert_eq!(result.failure, None);
                assert!(result.events.iter().any(|event| matches!(event, CoreRunEvent::LocalCarries(rows) if rows.iter().any(|row| row.tick == 0 && row.person == 2 && row.from == carrying.from && Some(row.to) == carrying.to && row.good == carrying.good && row.quantity == carrying.quantity && row.moved))));
                assert!(!result.events.iter().any(|event| matches!(event, CoreRunEvent::Construction(rows) if rows.iter().any(|row| row.tick == 0 && row.person == 3))));
                let choices: Vec<_> = result
                    .events
                    .iter()
                    .filter_map(|event| match event {
                        CoreRunEvent::PersonalChoices(rows) => Some(rows.as_slice()),
                        _ => None,
                    })
                    .flatten()
                    .collect();
                assert!(choices.iter().any(|event| event.actor == 1
                    && event.key.is_some_and(|key| key.domain == 1)
                    && event.outcome == CoreChoiceOutcome::Accepted));
                assert!(choices.iter().any(|event| event.actor == 2
                    && event.key.is_some_and(|key| key.domain == 2)
                    && event.outcome == CoreChoiceOutcome::Accepted));
                assert!(choices.iter().any(|event| event.actor == 3
                    && event.key.is_none()
                    && event.outcome == CoreChoiceOutcome::NoLocalAction));
                let snapshot = host.snapshot();
                assert_eq!(snapshot.sites.as_ref().unwrap().facilities.sites.len(), 1);
                assert!(
                    snapshot
                        .schedule
                        .as_ref()
                        .unwrap()
                        .personal_choices
                        .is_some()
                );
                let mut resumed = game_economy::core_interval::CoreHost::resume(
                    serde_json::from_str(&serde_json::to_string(&snapshot).unwrap()).unwrap(),
                )
                .unwrap();
                assert_eq!(host.advance_continuous(900_000, &[]).unwrap().failure, None);
                assert_eq!(
                    resumed.advance_continuous(900_000, &[]).unwrap().failure,
                    None
                );
                assert_eq!(host.snapshot(), resumed.snapshot());
                serde_json::to_value(host.snapshot()).unwrap()
            })
    }
    assert_eq!(run(1), run(4));
}

#[test]
fn stale_saved_carrying_intention_is_reported_without_substituting_an_action() {
    use game_economy::core_interval::{
        CoreChoiceIntent, CoreChoiceOutcome, CoreRunEvent, DecisionPhase,
    };
    let root = root();
    let host = game_economy_cli::core_scenario(
        &root.join("content/scenarios/economy-core-personal-choices.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut snapshot = host.snapshot();
    let grain = snapshot
        .core
        .physical
        .goods
        .iter()
        .find(|good| good.name == "grain")
        .unwrap()
        .id;
    snapshot.schedule.as_mut().unwrap().phase = DecisionPhase::ChosenCarrying {
        construction_after: false,
        maintenance_after: false,
        work_after: false,
        meal_people: vec![],
        intents: std::sync::Arc::new(vec![CoreChoiceIntent {
            actor: 2,
            key: game_personal_choice::Key {
                domain: 2,
                action: u64::from(grain),
            },
            movement: Some(game_lots::Movement {
                id: 0,
                from: game_lots::Place::Stock {
                    region: 1,
                    good: grain,
                },
                to: Some(game_lots::Place::Person(2)),
                good: grain,
                quantity: 3,
                item: None,
            }),
            second_movement: None,
            work: None,
        }]),
    };
    let mut forbidden = snapshot.clone();
    if let DecisionPhase::ChosenCarrying { intents, .. } =
        &mut forbidden.schedule.as_mut().unwrap().phase
    {
        let intents = std::sync::Arc::make_mut(intents);
        intents[0].actor = 3;
        intents[0].movement.as_mut().unwrap().to = Some(game_lots::Place::Person(3));
    }
    assert!(game_economy::core_interval::CoreHost::resume(forbidden).is_err());
    let mut host = game_economy::core_interval::CoreHost::resume(
        serde_json::from_str(&serde_json::to_string(&snapshot).unwrap()).unwrap(),
    )
    .unwrap();
    let result = host.advance_scheduled(1, &[]).unwrap();
    assert_eq!(result.failure, None);
    assert!(result.events.iter().any(|event| matches!(event, CoreRunEvent::PersonalChoices(rows) if rows.iter().any(|row| row.actor == 2 && row.outcome == CoreChoiceOutcome::Stale))));
    assert!(!result.events.iter().any(|event| matches!(event, CoreRunEvent::LocalCarries(rows) if rows.iter().any(|row| row.person == 2))));
}

#[test]
fn saved_choice_round_keeps_exact_intentions_across_workers() {
    use game_economy::core_interval::DecisionPhase;
    let root = root();
    let mut host = game_economy_cli::core_scenario(
        &root.join("content/scenarios/economy-core-personal-choices.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut profiles = host
        .snapshot()
        .schedule
        .unwrap()
        .personal_choices
        .unwrap()
        .as_ref()
        .clone();
    profiles[2].idle = 2;
    profiles[2].act_on_tie = true;
    host.enable_personal_choices(profiles).unwrap();
    let (intents, _) = host.preview_personal_choices(&[]).unwrap();
    assert_eq!(intents.len(), 3);
    let mut snapshot = host.snapshot();
    snapshot.schedule.as_mut().unwrap().phase = DecisionPhase::ChosenConstruction {
        meal_people: vec![],
        intents: std::sync::Arc::new(intents),
        carrying_after: true,
        maintenance_after: false,
        work_after: false,
    };
    let saved = serde_json::to_string(&snapshot).unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = game_economy::core_interval::CoreHost::resume(
                    serde_json::from_str(&saved).unwrap(),
                )
                .unwrap();
                let result = host.advance_continuous(900_000, &[]).unwrap();
                assert_eq!(result.failure, None);
                serde_json::to_value(host.snapshot()).unwrap()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn contested_resources_make_later_intentions_stale_without_substitution() {
    use game_economy::core_interval::{CoreChoiceOutcome, CoreRunEvent};
    rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap()
        .install(|| {
            let root = root();
            let source = std::fs::read_to_string(
                root.join("content/scenarios/economy-core-personal-choices.toml"),
            )
            .unwrap();
            let token = format!(
                "legacy-choice-contest-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(format!("{token}.toml"));
            let mut winners = std::collections::BTreeSet::new();
            for seed in 1..=20 {
                std::fs::write(
                    &path,
                    source
                        .replace("seed = 7", &format!("seed = {seed}"))
                        .replace("stock = { grain = 20 }", "stock = { grain = 3 }"),
                )
                .unwrap();
                let mut host =
                    game_economy_cli::core_scenario(&path, &root.join("content")).unwrap();
                let result = host.advance_continuous(900_000, &[]).unwrap();
                assert_eq!(result.failure, None);
                let choices: Vec<_> = result
                    .events
                    .iter()
                    .filter_map(|event| match event {
                        CoreRunEvent::PersonalChoices(rows) => Some(rows.as_slice()),
                        _ => None,
                    })
                    .flatten()
                    .filter(|event| event.tick == 0 && (event.actor == 1 || event.actor == 2))
                    .collect();
                assert_eq!(
                    choices
                        .iter()
                        .filter(|event| event.outcome == CoreChoiceOutcome::Accepted)
                        .count(),
                    1,
                    "seed {seed}: {choices:?}"
                );
                assert_eq!(
                    choices
                        .iter()
                        .filter(|event| event.outcome == CoreChoiceOutcome::Stale)
                        .count(),
                    1,
                    "seed {seed}: {choices:?}"
                );
                winners.insert(
                    choices
                        .iter()
                        .find(|event| event.outcome == CoreChoiceOutcome::Accepted)
                        .unwrap()
                        .key
                        .unwrap()
                        .domain,
                );
                if winners.len() == 2 {
                    break;
                }
            }
            std::fs::remove_file(path).unwrap();
            assert_eq!(winners.len(), 2);
        });
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
                language: None,
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
