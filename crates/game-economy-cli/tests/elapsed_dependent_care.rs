use game_economy::core_interval::{CoreHost, CoreRunEvent};

#[test]
fn an_unrelated_caregiver_acts_on_their_own_recent_interpretation() {
    let content = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let scenario = content.join("scenarios/economy-care-signals.toml");
    let simulate = |workers, durations: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = game_economy_cli::core_scenario(&scenario, &content).unwrap();
                let mut visits = Vec::new();
                for &duration in durations {
                    let advanced = host.advance_continuous(duration, &[]).unwrap();
                    assert_eq!(advanced.failure, None);
                    visits.extend(advanced.events.into_iter().filter_map(|event| {
                        if let CoreRunEvent::Care(visit) = event {
                            Some(visit)
                        } else {
                            None
                        }
                    }));
                    let save = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&save).unwrap()).unwrap();
                }
                assert_eq!(visits.len(), 2);
                let first = &visits[0];
                assert_eq!(first.at_millis, 137);
                assert_eq!(first.events[0].child, 3);
                assert_eq!(first.events[0].caregiver, Some(1));
                let signal = first.events[0].signal.unwrap();
                assert_eq!(
                    (signal.form, signal.meaning, signal.noticed_at),
                    (1, 1, 100)
                );
                assert_eq!(first.events[1].child, 4);
                assert!(first.events[1].caregiver.is_none());
                assert!(first.transfers.iter().any(|row| row.accepted));
                let next = &visits[1];
                assert_eq!(next.at_millis, 274);
                assert!(next.events.iter().all(|row| row.caregiver.is_none()));
                assert!(next.transfers.is_empty());
                (host.snapshot(), visits)
            })
    };
    assert_eq!(simulate(1, &[274]), simulate(6, &[101, 31, 97, 45]));
}

#[test]
fn a_child_can_learn_familiarity_with_an_unrelated_caregiver() {
    let content = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let scenario = content.join("scenarios/economy-care-familiarity.toml");
    let opening = game_economy_cli::core_scenario(&scenario, &content)
        .unwrap()
        .snapshot();
    let simulate = |workers, durations: &[u64], visits: bool, weight| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = opening.clone();
                initial.family.as_mut().unwrap().members[3]
                    .mate_preference
                    .as_mut()
                    .unwrap()
                    .childhood_weight = weight;
                if !visits {
                    initial.dependent_care.as_mut().unwrap().preferences[0].value = 0;
                }
                let mut host = CoreHost::resume(initial).unwrap();
                let mut recognized = Vec::new();
                for &duration in durations {
                    let advanced = host.advance_continuous(duration, &[]).unwrap();
                    assert_eq!(advanced.failure, None);
                    recognized.extend(
                        advanced
                            .events
                            .into_iter()
                            .filter_map(|event| match event {
                                CoreRunEvent::Care(visit) => Some(visit.recognized),
                                _ => None,
                            })
                            .flatten(),
                    );
                    let save = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&save).unwrap()).unwrap();
                }
                let state = host.snapshot();
                assert_eq!(host.core().simulated_millis(), 1100);
                let family = state.family.as_ref().unwrap();
                let familiarity = family.familiarity.as_ref().unwrap();
                assert_eq!(familiarity.recognizes(4, 3), visits);
                assert!(!familiarity.recognizes(3, 4));
                assert!(!familiarity.recognizes(4, 1));
                assert!(!familiarity.recognizes(4, 2));
                assert_eq!(
                    family.members[3].partner,
                    (!visits || weight >= 0).then_some(3)
                );
                if visits {
                    assert_eq!(recognized.len(), 1);
                    let recognition = recognized[0];
                    assert_eq!(
                        (
                            recognition.child,
                            recognition.caregiver,
                            recognition.at_millis
                        ),
                        (4, 3, 200)
                    );
                    let mut forged = state.clone();
                    std::sync::Arc::make_mut(
                        forged
                            .family
                            .as_mut()
                            .unwrap()
                            .familiarity
                            .as_mut()
                            .unwrap(),
                    )
                    .learners[0]
                        .contacts[0]
                        .recognized_at = Some(1101);
                    assert!(CoreHost::resume(forged).is_err());
                } else {
                    assert!(recognized.is_empty());
                }
                (state, recognized)
            })
    };
    assert_eq!(
        simulate(1, &[1100], true, -10),
        simulate(6, &[137, 63, 399, 501], true, -10)
    );
    simulate(4, &[1100], false, -10);
    simulate(4, &[1100], true, 10);
}

#[test]
fn children_interpret_attributed_care_differently_without_assigning_unobserved_blame() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let opening = game_economy_cli::core_scenario(
        &root.join("content/scenarios/economy-care-relationships.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    let simulate = |workers, durations: &[u64], available| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = opening.clone();
                for policy in &mut initial.dependent_care.as_mut().unwrap().policies {
                    policy.willing = available;
                }
                let mut host = CoreHost::resume(initial).unwrap();
                let mut visits = Vec::new();
                for &duration in durations {
                    let advanced = host.advance_continuous(duration, &[]).unwrap();
                    assert_eq!(advanced.failure, None);
                    visits.extend(advanced.events.into_iter().filter_map(|event| {
                        if let CoreRunEvent::Care(visit) = event {
                            Some(visit)
                        } else {
                            None
                        }
                    }));
                    let save = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&save).unwrap()).unwrap();
                }
                assert_eq!(visits.len(), 1);
                let visit = &visits[0];
                assert_eq!(visit.at_millis, 100);
                assert_eq!(visit.events.len(), 2);
                let opinions = host.snapshot().opinions.unwrap();
                for event in &visit.events {
                    if available {
                        let caregiver = event.caregiver.unwrap();
                        let (trust, affinity, fear, obligation) =
                            opinions.index().subjective(event.child, caregiver);
                        assert_eq!(
                            (trust, affinity, fear, obligation),
                            if event.child == 3 {
                                (1, 3, 0, 0)
                            } else {
                                (-2, -4, 2, 0)
                            }
                        );
                    } else {
                        assert!(event.caregiver.is_none());
                    }
                }
                if !available {
                    assert!(opinions.records.is_empty());
                }
                (visits, opinions.records)
            })
    };
    assert_eq!(simulate(1, &[100], true), simulate(6, &[59, 21, 20], true));
    simulate(1, &[100], false);
}

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

[opinions.rules]
minimum = -10
maximum = 10
initial = 0
fulfilled_gain = 0
default_loss = 0
gratitude_gain = 2
newborn_obligation_development = { until_age = 10, minimum = 0, maximum = 10, cared_change = -2, unattended_change = 3, cared_promise_change = 0, unattended_promise_change = 2 }
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
                let opinions = state.opinions.as_ref().unwrap();
                let child = opinions
                    .sensitivities
                    .iter()
                    .find(|row| row.person == 3)
                    .unwrap();
                assert_eq!(child.gratitude_gain, Some(if willing { 0 } else { 5 }));
                assert_eq!(
                    child.promise_obligation_gain,
                    Some(if willing { 0 } else { 2 })
                );
                assert!(opinions.records.iter().all(|row| row.observer != 3));
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
    assert_eq!(
        result
            .events
            .iter()
            .filter_map(|event| match event {
                CoreRunEvent::Care(visit) => Some(visit.at_millis),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [500, 1000]
    );
}
