use super::*;

#[test]
fn opposed_actions_use_each_observers_signed_reactions_and_per_outcome_overrides() {
    let mut rules = rules();
    rules.affinity = Some(AffinityRules {
        limit: 10,
        encounter_change: 0,
        pressured_encounter_change: 0,
        help_change: 0,
        harm_change_per_damage: 0,
    });
    let reaction = std::sync::Arc::new(OpposedActionReaction {
        blocked: Some(SubjectiveReaction {
            trust: 1,
            affinity: -2,
            fear: 3,
            obligation: 0,
        }),
        overcame: Some(SubjectiveReaction {
            trust: 2,
            affinity: 1,
            fear: 0,
            obligation: 0,
        }),
        ..Default::default()
    });
    let mut sensitivities: Vec<_> = (1..=4096)
        .map(|person| Sensitivity {
            person,
            opposed_action_reaction: Some(reaction.clone()),
            ..Default::default()
        })
        .collect();
    sensitivities[0].relationship_cases.push(RelationshipCase {
        target: 9000,
        opposed_action_reaction: Some(std::sync::Arc::new(OpposedActionReaction {
            blocked: Some(SubjectiveReaction {
                trust: -1,
                affinity: 5,
                fear: 0,
                obligation: 0,
            }),
            ..Default::default()
        })),
        ..Default::default()
    });
    sensitivities[1].relationship_cases.push(RelationshipCase {
        target: 9000,
        opposed_action_reaction: Some(std::sync::Arc::new(OpposedActionReaction {
            blocked: Some(SubjectiveReaction::default()),
            ..Default::default()
        })),
        ..Default::default()
    });
    let initial = Opinions {
        rules,
        sensitivities: sensitivities.into(),
        records: vec![],
        events: vec![],
    };
    let mut experiences: Vec<_> = (1..=4096)
        .map(|observer| OpposedActionExperience {
            observer,
            other: 9000,
            outcome: OpposedActionOutcome::Blocked,
        })
        .collect();
    experiences.push(OpposedActionExperience {
        observer: 2,
        other: 9000,
        outcome: OpposedActionOutcome::Overcame,
    });
    experiences.push(OpposedActionExperience {
        observer: 4097,
        other: 9000,
        outcome: OpposedActionOutcome::Blocked,
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut owner = initial.clone();
                owner.learn_opposed_actions(&experiences, 1);
                owner
            })
    };
    let result = run(1);
    assert_eq!(result, run(4));
    let index = result.index();
    assert_eq!(
        (
            index.trust(1, 9000),
            index.affinity(1, 9000),
            index.feelings(1, 9000)
        ),
        (-1, 5, (0, 0))
    );
    assert_eq!(
        (
            index.trust(2, 9000),
            index.affinity(2, 9000),
            index.feelings(2, 9000)
        ),
        (2, 1, (0, 0))
    );
    assert_eq!(
        (
            index.trust(3, 9000),
            index.affinity(3, 9000),
            index.feelings(3, 9000)
        ),
        (1, -2, (3, 0))
    );
    assert!(!result.records.iter().any(|row| row.observer == 4097));
}

#[test]
fn confinement_experiences_use_personal_signed_reactions_and_case_overrides() {
    let mut rules = rules();
    rules.affinity = Some(AffinityRules {
        limit: 10,
        encounter_change: 0,
        pressured_encounter_change: 0,
        help_change: 0,
        harm_change_per_damage: 0,
    });
    let reaction = ConfinementReaction {
        captured: SubjectiveReaction {
            trust: 1,
            affinity: -2,
            fear: 3,
            obligation: 4,
        },
        ..Default::default()
    };
    let mut sensitivities: Vec<_> = (1..=4096)
        .map(|person| Sensitivity {
            person,
            opposed_action_reaction: None,
            confinement_reaction: Some(reaction),
            ..Default::default()
        })
        .collect();
    sensitivities[0].relationship_cases.push(RelationshipCase {
        target: 9000,
        opposed_action_reaction: None,
        confinement_reaction: Some(ConfinementReaction {
            captured: SubjectiveReaction {
                trust: -1,
                affinity: 5,
                fear: 0,
                obligation: 0,
            },
            ..Default::default()
        }),
        ..Default::default()
    });
    let initial = Opinions {
        rules,
        sensitivities: sensitivities.into(),
        records: vec![],
        events: vec![],
    };
    let experiences: Vec<_> = (1..=4096)
        .map(|observer| ConfinementExperience {
            observer,
            actor: 9000,
            kind: ConfinementExperienceKind::Captured,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut owner = initial.clone();
                owner.learn_confinement(&experiences, 1);
                owner
            })
    };
    let result = run(1);
    assert_eq!(result, run(4));
    let index = result.index();
    assert_eq!(
        (
            index.trust(1, 9000),
            index.affinity(1, 9000),
            index.feelings(1, 9000)
        ),
        (-1, 5, (0, 0))
    );
    assert_eq!(
        (
            index.trust(2, 9000),
            index.affinity(2, 9000),
            index.feelings(2, 9000)
        ),
        (1, -2, (3, 4))
    );
}

#[test]
fn simultaneous_personal_changes_are_signed_bounded_and_worker_invariant() {
    let mut rules = rules();
    rules.affinity = Some(AffinityRules {
        limit: 10,
        encounter_change: 0,
        pressured_encounter_change: 0,
        help_change: 0,
        harm_change_per_damage: 0,
    });
    let initial = Opinions {
        rules,
        records: vec![],
        events: vec![],
        sensitivities: vec![].into(),
    };
    let mut changes: Vec<_> = (1..=3000)
        .rev()
        .map(|observer| PersonalChange {
            observer,
            target: 9000,
            trust: -2,
            affinity: 3,
            fear: if observer == 1 { 3 } else { 0 },
            obligation: if observer == 1 { -4 } else { 0 },
        })
        .collect();
    changes.extend([
        PersonalChange {
            observer: 1,
            target: 9000,
            trust: 5,
            affinity: -10,
            fear: -5,
            obligation: 9,
        },
        PersonalChange {
            observer: 2,
            target: 9000,
            trust: 2,
            affinity: -3,
            fear: 0,
            obligation: 0,
        },
        PersonalChange {
            observer: 4000,
            target: 9000,
            trust: 0,
            affinity: 0,
            fear: -1,
            obligation: -1,
        },
    ]);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut result = initial.clone();
                result.apply_personal_changes(&changes, 5);
                result
            })
    };
    let result = run(1);
    assert_eq!(result, run(4));
    let index = result.index();
    assert_eq!((index.trust(1, 9000), index.affinity(1, 9000)), (3, -7));
    assert_eq!(index.feelings(1, 9000), (0, 5));
    assert_eq!((index.trust(2, 9000), index.affinity(2, 9000)), (0, 0));
    assert_eq!((index.trust(3, 9000), index.affinity(3, 9000)), (-2, 3));
    assert_eq!(result.records.len(), 2999);
    assert!(result.events.iter().all(|event| event.tick == 5));
}
#[test]
fn personal_dispositions_share_until_a_real_change_then_detach() {
    use std::sync::Arc;
    let development = ObligationDevelopment {
        until_age: 3,
        minimum: 0,
        maximum: 10,
        cared_change: 0,
        unattended_change: 0,
        cared_promise_change: Some(0),
        unattended_promise_change: None,
    };
    let mut configured = rules();
    configured.newborn_obligation_development = Some(development.clone());
    let mut state = Opinions {
        rules: configured,
        records: vec![],
        events: vec![],
        sensitivities: vec![Sensitivity {
            person: 1,
            obligation_development: Some(development),
            relationship_cases: vec![RelationshipCase {
                target: 2,
                help: Some(1),
                ..Default::default()
            }],
            ..Default::default()
        }]
        .into(),
    };
    let original = state.clone();
    assert!(Arc::ptr_eq(&state.sensitivities, &original.sensitivities));
    state.develop_obligation(&[(1, 3, true), (9, 0, true)]);
    state.enroll_newborn_development(&[1]);
    assert!(Arc::ptr_eq(&state.sensitivities, &original.sensitivities));
    state.develop_obligation(&[(1, 1, true)]);
    assert!(!Arc::ptr_eq(&state.sensitivities, &original.sensitivities));
    assert_eq!(original.sensitivities[0].gratitude_gain, None);
    assert_eq!(state.sensitivities[0].gratitude_gain, Some(0));
    assert_eq!(state.sensitivities[0].promise_obligation_gain, Some(0));
    let unchanged = state.clone();
    state.develop_obligation(&[(1, 1, true)]);
    assert!(Arc::ptr_eq(&state.sensitivities, &unchanged.sensitivities));
    state.sensitivities_mut()[0].relationship_cases[0].help = Some(-2);
    assert_eq!(
        unchanged.sensitivities[0].relationship_cases[0].help,
        Some(1)
    );
    state.enroll_newborn_development(&[3]);
    assert!(state.sensitivities.iter().any(|s| s.person == 3));
    assert!(unchanged.sensitivities.iter().all(|s| s.person != 3));
    let before_reset = state.clone();
    state.reset_for_generation();
    assert!(state.sensitivities.is_empty());
    assert!(before_reset.sensitivities.iter().any(|s| s.person == 3));
}
#[test]
fn report_judgments_handle_dense_sparse_and_reordered_people() {
    let mut state = Opinions {
        rules: rules(),
        sensitivities: vec![].into(),
        records: vec![],
        events: vec![],
    };
    state.rules.minimum_report_trust = Some(1);
    let policy = |person, reception| Sensitivity {
        person,
        report_reception: Some(reception),
        ..Default::default()
    };
    for people in [[3, 1, 2], [30, 10, 20]] {
        state.sensitivities = vec![
            policy(people[0], ReportReception::Never),
            policy(people[1], ReportReception::Always),
            policy(people[2], ReportReception::TrustAtLeast(0)),
        ]
        .into();
        let filter = state.report_filter().unwrap();
        assert!(!filter.accepts(people[0], 99));
        assert!(filter.accepts(people[1], 99));
        assert!(filter.accepts(people[2], 99));
        assert!(!filter.accepts(100, 99));
    }
    state.sensitivities_mut().clear();
    state.rules.minimum_report_trust = None;
    assert!(state.report_filter().is_none());
}
#[test]
fn each_person_interprets_an_observed_discrepancy_in_their_own_way() {
    let mut rules = rules();
    rules.affinity = Some(AffinityRules {
        limit: 10,
        encounter_change: 0,
        pressured_encounter_change: 0,
        help_change: 0,
        harm_change_per_damage: 0,
    });
    let mut state = Opinions {
        rules,
        records: vec![],
        events: vec![],
        sensitivities: vec![Sensitivity {
            person: 1,
            affinity_report_discrepancy: Some(-2),
            relationship_cases: vec![RelationshipCase {
                target: 2,
                report_discrepancy: Some(3),
                ..Default::default()
            }],
            ..Default::default()
        }]
        .into(),
    };
    state.learn_report_discrepancies(&[(1, 2), (1, 3), (4, 2)], 1);
    assert_eq!(state.index().affinity(1, 2), 3);
    assert_eq!(state.index().affinity(1, 3), -2);
    assert_eq!(state.index().affinity(4, 2), 0);
}
#[test]
fn report_discrepancies_change_only_the_listeners_chosen_trust() {
    let mut state = Opinions {
        rules: rules(),
        records: vec![],
        events: vec![],
        sensitivities: vec![Sensitivity {
            person: 1,
            trust_report_discrepancy: Some(-2),
            relationship_cases: vec![
                RelationshipCase {
                    target: 2,
                    report_discrepancy_trust: Some(3),
                    ..Default::default()
                },
                RelationshipCase {
                    target: 4,
                    report_discrepancy_trust: Some(0),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }]
        .into(),
    };
    assert!(state.tracks_report_discrepancy());
    state.learn_report_discrepancies(&[(1, 2), (1, 3), (1, 4), (4, 2)], 7);
    assert_eq!(state.index().trust(1, 2), 3);
    assert_eq!(state.index().trust(1, 3), -2);
    assert_eq!(state.index().trust(4, 2), 0);
    assert_eq!(state.index().trust(1, 4), 0);
    assert_eq!(state.index().affinity(1, 2), 0);
    assert_eq!(state.index().obligation(1, 2), 0);
    assert_eq!(state.events.len(), 2);
    assert!(state.events.iter().all(|e| e.tick == 7));
    state.learn_report_discrepancies(&[(1, 2), (1, 3), (1, 3)], 8);
    assert_eq!(state.index().trust(1, 2), 5);
    assert_eq!(state.index().trust(1, 3), -5);
}

#[test]
fn report_trust_merge_handles_sparse_unordered_existing_relationships_across_chunks() {
    let count = 2050u64;
    let initial = Opinions {
        rules: rules(),
        events: vec![],
        sensitivities: (1..=count)
            .rev()
            .map(|person| Sensitivity {
                person,
                trust_report_discrepancy: Some(if person % 2 == 0 { 3 } else { -2 }),
                ..Default::default()
            })
            .collect::<Vec<_>>()
            .into(),
        records: (1..=count)
            .rev()
            .filter(|person| person % 2 == 0)
            .map(|observer| Opinion {
                observer,
                target: 9000,
                trust: 1,
                fear: 0,
                obligation: 0,
                affinity: 0,
            })
            .collect(),
    };
    let observations: Vec<_> = (1..=count).rev().map(|person| (person, 9000)).collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = initial.clone();
                state.learn_report_discrepancies(&observations, 1);
                state
            })
    };
    let result = run(1);
    assert_eq!(result, run(4));
    assert_eq!(result.events.len(), count as usize);
    assert!(result.events.iter().all(|event| {
        if event.observer % 2 == 0 {
            (event.before, event.after) == (1, 4)
        } else {
            (event.before, event.after) == (0, -2)
        }
    }));
    assert_eq!(result.records.len(), count as usize);
}

#[test]
fn unordered_sparse_discrepancies_match_ordered_inputs() {
    let mut rules = rules();
    rules.affinity = Some(AffinityRules {
        limit: 10,
        encounter_change: 0,
        pressured_encounter_change: 0,
        help_change: 0,
        harm_change_per_damage: 0,
    });
    let mut ordered = Opinions {
        rules,
        records: vec![],
        events: vec![],
        sensitivities: vec![
            Sensitivity {
                person: 10,
                affinity_report_discrepancy: Some(2),
                ..Default::default()
            },
            Sensitivity {
                person: 30,
                affinity_report_discrepancy: Some(-1),
                ..Default::default()
            },
        ]
        .into(),
    };
    let mut reordered = ordered.clone();
    reordered.sensitivities_mut().reverse();
    ordered.learn_report_discrepancies(&[(10, 20), (10, 20), (30, 20)], 1);
    reordered.learn_report_discrepancies(&[(30, 20), (10, 20), (10, 20)], 1);
    assert_eq!(ordered.records, reordered.records);
    assert_eq!(ordered.index().affinity(10, 20), 4);
    assert_eq!(ordered.index().affinity(30, 20), -1);
}
#[test]
fn witnessed_takings_are_combined_per_pair_without_losing_other_feelings() {
    let mut state = Opinions {
        rules: rules(),
        sensitivities: vec![].into(),
        events: vec![],
        records: vec![Opinion {
            observer: 1,
            target: 2,
            trust: 4,
            fear: 7,
            obligation: 3,
            affinity: 0,
        }],
    };
    state.rules.theft_loss = 2;
    state.witness_thefts(&[(1, 2), (3, 2), (1, 2)], 1);
    assert_eq!(
        (
            state.index().trust(1, 2),
            state.index().fear(1, 2),
            state.index().obligation(1, 2)
        ),
        (0, 7, 3)
    );
    assert_eq!(state.index().trust(3, 2), -2);
    assert_eq!(state.events.len(), 2);
}

#[test]
fn people_can_interpret_the_same_kind_of_taking_differently() {
    let mut state = Opinions {
        rules: rules(),
        events: vec![],
        records: vec![],
        sensitivities: vec![
            Sensitivity {
                person: 1,
                report_reception: None,
                affinity_report_discrepancy: None,
                trust_report_discrepancy: None,
                trust_promise_fulfilled: None,
                trust_promise_unfulfilled: None,
                fear_per_damage: None,
                gratitude_gain: None,
                obligation_relief: None,
                theft_loss: Some(0),
                confrontation: None,
                affinity_help: None,
                affinity_harm_per_damage: None,
                affinity_encounter: None,
                affinity_pressured_encounter: None,
                affinity_site_use: None,
                affinity_site_destruction: None,
                affinity_site_salvage: None,
                affinity_work_success: None,
                affinity_work_refusal: None,
                opposed_action_reaction: None,
                confinement_reaction: None,
                relationship_cases: vec![],
                obligation_development: None,
                promise_obligation_gain: None,
            },
            Sensitivity {
                person: 3,
                report_reception: None,
                affinity_report_discrepancy: None,
                trust_report_discrepancy: None,
                trust_promise_fulfilled: None,
                trust_promise_unfulfilled: None,
                fear_per_damage: None,
                gratitude_gain: None,
                obligation_relief: None,
                theft_loss: Some(3),
                confrontation: None,
                affinity_help: None,
                affinity_harm_per_damage: None,
                affinity_encounter: None,
                affinity_pressured_encounter: None,
                affinity_site_use: None,
                affinity_site_destruction: None,
                affinity_site_salvage: None,
                affinity_work_success: None,
                affinity_work_refusal: None,
                opposed_action_reaction: None,
                confinement_reaction: None,
                relationship_cases: vec![],
                obligation_development: None,
                promise_obligation_gain: None,
            },
        ]
        .into(),
    };
    state.witness_thefts(&[(1, 2), (3, 2)], 1);
    assert_eq!(state.index().trust(1, 2), 0);
    assert_eq!(state.index().trust(3, 2), -3);
    assert_eq!(state.events.len(), 1);
}

#[test]
fn individual_reciprocity_relief_preserves_other_feelings_and_creates_no_empty_memory() {
    let mut state = Opinions {
        rules: rules(),
        events: vec![],
        sensitivities: vec![Sensitivity {
            person: 1,
            report_reception: None,
            affinity_report_discrepancy: None,
            trust_report_discrepancy: None,
            trust_promise_fulfilled: None,
            trust_promise_unfulfilled: None,
            fear_per_damage: None,
            gratitude_gain: None,
            obligation_relief: Some(2),
            theft_loss: None,
            confrontation: None,
            affinity_help: None,
            affinity_harm_per_damage: None,
            affinity_encounter: None,
            affinity_pressured_encounter: None,
            affinity_site_use: None,
            affinity_site_destruction: None,
            affinity_site_salvage: None,
            affinity_work_success: None,
            affinity_work_refusal: None,
            opposed_action_reaction: None,
            confinement_reaction: None,
            relationship_cases: vec![],
            obligation_development: None,
            promise_obligation_gain: None,
        }]
        .into(),
        records: vec![Opinion {
            observer: 1,
            target: 2,
            trust: -3,
            fear: 5,
            obligation: 3,
            affinity: 0,
        }],
    };
    state.learn_help(&[(2, 1)]);
    assert_eq!(
        (
            state.index().obligation(1, 2),
            state.index().fear(1, 2),
            state.index().trust(1, 2)
        ),
        (1, 5, -3)
    );
    state.learn_help(&[(2, 1), (2, 1)]);
    assert_eq!(state.index().obligation(1, 2), 0);
    assert_eq!(state.records.len(), 1);
}

#[test]
fn simultaneous_gifts_do_not_relieve_obligation_created_in_the_same_batch() {
    let mut state = Opinions {
        rules: rules(),
        sensitivities: vec![].into(),
        events: vec![],
        records: vec![Opinion {
            observer: 1,
            target: 2,
            trust: 0,
            fear: 0,
            obligation: 1,
            affinity: 0,
        }],
    };
    state.rules.gratitude_gain = 3;
    state.rules.obligation_relief = 5;
    state.learn_help(&[(1, 2), (2, 1)]);
    assert_eq!(state.index().obligation(1, 2), 3);
    assert_eq!(state.index().obligation(2, 1), 3);
}
#[test]
fn each_person_can_respond_differently_to_the_same_experience() {
    let mut state = Opinions {
        rules: rules(),
        sensitivities: vec![
            Sensitivity {
                person: 2,
                report_reception: None,
                affinity_report_discrepancy: None,
                trust_report_discrepancy: None,
                trust_promise_fulfilled: None,
                trust_promise_unfulfilled: None,
                fear_per_damage: Some(0),
                gratitude_gain: Some(0),
                obligation_relief: None,
                theft_loss: None,
                confrontation: None,
                affinity_help: None,
                affinity_harm_per_damage: None,
                affinity_encounter: None,
                affinity_pressured_encounter: None,
                affinity_site_use: None,
                affinity_site_destruction: None,
                affinity_site_salvage: None,
                affinity_work_success: None,
                affinity_work_refusal: None,
                opposed_action_reaction: None,
                confinement_reaction: None,
                relationship_cases: vec![],
                obligation_development: None,
                promise_obligation_gain: None,
            },
            Sensitivity {
                person: 3,
                report_reception: None,
                affinity_report_discrepancy: None,
                trust_report_discrepancy: None,
                trust_promise_fulfilled: None,
                trust_promise_unfulfilled: None,
                fear_per_damage: Some(2),
                gratitude_gain: Some(3),
                obligation_relief: None,
                theft_loss: None,
                confrontation: None,
                affinity_help: None,
                affinity_harm_per_damage: None,
                affinity_encounter: None,
                affinity_pressured_encounter: None,
                affinity_site_use: None,
                affinity_site_destruction: None,
                affinity_site_salvage: None,
                affinity_work_success: None,
                affinity_work_refusal: None,
                opposed_action_reaction: None,
                confinement_reaction: None,
                relationship_cases: vec![],
                obligation_development: None,
                promise_obligation_gain: None,
            },
        ]
        .into(),
        records: vec![],
        events: vec![],
    };
    state.learn_harm(&[(2, 1, 2), (3, 1, 2)]);
    state.learn_help(&[(2, 1), (3, 1)]);
    assert_eq!(state.index().fear(2, 1), 0);
    assert_eq!(state.index().obligation(2, 1), 0);
    assert_eq!(state.index().fear(3, 1), 4);
    assert_eq!(state.index().obligation(3, 1), 3);
    assert!(!state.records.iter().any(|row| row.observer == 2));
}
#[test]
fn directly_suffered_harm_and_later_trust_updates_preserve_separate_feelings() {
    let mut state = Opinions {
        sensitivities: Vec::new().into(),
        rules: rules(),
        records: vec![Opinion {
            observer: 2,
            target: 1,
            trust: 4,
            fear: 7,
            obligation: 0,
            affinity: 0,
        }],
        events: vec![],
    };
    state.rules.fear_per_damage = 2;
    state.rules.gratitude_gain = 3;
    state.learn_harm(&[(2, 1, 3), (2, 1, 1), (3, 1, 2)]);
    state.learn_help(&[(2, 1), (2, 1)]);
    assert_eq!(state.index().fear(2, 1), 15);
    assert_eq!(state.index().fear(3, 1), 4);
    assert_eq!(state.index().trust(2, 1), 4);
    state.advance_validated(
        &[contract(1, 1, 2)],
        &[AgreementEvent {
            tick: 1,
            contract: 1,
            actor: 1,
            kind: EventKind::Fulfilled,
            quantity: 1,
        }],
        1,
    );
    assert_eq!(state.index().fear(2, 1), 15);
    assert_eq!(state.index().trust(2, 1), 5);
    assert_eq!(state.index().obligation(2, 1), 6);
}
use game_parallel_agreements::{Event as AgreementEvent, EventKind, Status};
fn contract(id: u64, debtor: u64, creditor: u64) -> Contract {
    Contract {
        id,
        debtor,
        creditor,
        good: 1,
        quantity: 1,
        delivered: 1,
        due: 1,
        status: Status::Fulfilled,
    }
}
fn rules() -> Rules {
    Rules {
        minimum: -5,
        maximum: 5,
        initial: 0,
        fulfilled_gain: 3,
        default_loss: 4,
        theft_loss: 0,
        encounter_gain: 0,
        minimum_report_trust: None,
        affinity: None,
        newborn_obligation_development: None,
        fear_per_damage: 0,
        fear_decay_per_tick: 0,
        gratitude_gain: 0,
        obligation_relief: 0,
    }
}
#[test]
fn experienced_outcomes_update_only_the_creditors_directed_trust() {
    let mut state = Opinions {
        sensitivities: Vec::new().into(),
        rules: rules(),
        records: vec![Opinion {
            observer: 1,
            target: 2,
            trust: 4,
            fear: 0,
            obligation: 0,
            affinity: 0,
        }],
        events: vec![],
    };
    let contracts = vec![contract(1, 1, 2)];
    state.advance_validated(
        &contracts,
        &[AgreementEvent {
            tick: 1,
            contract: 1,
            actor: 1,
            kind: EventKind::Fulfilled,
            quantity: 1,
        }],
        1,
    );
    assert_eq!(state.index().trust(2, 1), 3);
    assert_eq!(state.index().trust(1, 2), 4);
    state.advance_validated(
        &contracts,
        &[AgreementEvent {
            tick: 2,
            contract: 1,
            actor: 1,
            kind: EventKind::Defaulted,
            quantity: 1,
        }],
        2,
    );
    assert_eq!(state.index().trust(2, 1), -1);
    assert_eq!(state.index().trust(1, 2), 4);
    state.validate(&HashSet::from([1, 2]), 2).unwrap();
}
#[test]
fn promise_outcomes_use_each_observers_signed_interpretation() {
    let initial = Opinions {
        rules: rules(),
        records: vec![],
        events: vec![],
        sensitivities: vec![
            Sensitivity {
                person: 2,
                trust_promise_fulfilled: Some(-2),
                trust_promise_unfulfilled: Some(3),
                relationship_cases: vec![RelationshipCase {
                    target: 4,
                    promise_fulfilled_trust: Some(4),
                    promise_unfulfilled_trust: Some(-1),
                    ..Default::default()
                }],
                ..Default::default()
            },
            Sensitivity {
                person: 3,
                trust_promise_fulfilled: Some(0),
                trust_promise_unfulfilled: Some(0),
                ..Default::default()
            },
        ]
        .into(),
    };
    let contracts = vec![
        contract(1, 1, 2),
        contract(2, 1, 3),
        contract(3, 4, 2),
        contract(4, 5, 6),
    ];
    let events = |kind| {
        contracts
            .iter()
            .rev()
            .map(|c| AgreementEvent {
                tick: 1,
                contract: c.id,
                actor: c.debtor,
                kind,
                quantity: 1,
            })
            .collect::<Vec<_>>()
    };
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = initial.clone();
                state.advance_validated(&contracts, &events(EventKind::Fulfilled), 1);
                let index = state.index();
                assert_eq!(
                    (
                        index.trust(2, 1),
                        index.trust(3, 1),
                        index.trust(2, 4),
                        index.trust(6, 5)
                    ),
                    (-2, 0, 4, 3)
                );
                assert!(!state.records.iter().any(|row| row.observer == 3));
                let mut restored: Opinions =
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
                state.advance_validated(&contracts, &events(EventKind::Defaulted), 2);
                restored.advance_validated(&contracts, &events(EventKind::Defaulted), 2);
                assert_eq!(state, restored);
                let index = state.index();
                assert_eq!(
                    (
                        index.trust(2, 1),
                        index.trust(3, 1),
                        index.trust(2, 4),
                        index.trust(6, 5)
                    ),
                    (1, 0, 3, -1)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn multiple_experiences_are_combined_and_clamped_deterministically() {
    let contracts: Vec<_> = (1..=8000).map(|id| contract(id, id, 9000)).collect();
    let events: Vec<_> = contracts
        .iter()
        .map(|c| AgreementEvent {
            tick: 1,
            contract: c.id,
            actor: c.debtor,
            kind: EventKind::Fulfilled,
            quantity: 1,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = Opinions {
                    sensitivities: Vec::new().into(),
                    rules: rules(),
                    records: vec![],
                    events: vec![],
                };
                state.advance_validated(&contracts, &events, 1);
                state
            })
    };
    assert_eq!(run(1), run(4));
    let state = run(4);
    assert_eq!(state.records.len(), 8000);
    assert_eq!(state.index().trust(9000, 1), 3);
}

#[test]
fn shuffled_contracts_and_multiple_outcomes_merge_against_prior_personal_trust() {
    let mut state = Opinions {
        sensitivities: Vec::new().into(),
        rules: rules(),
        records: vec![
            Opinion {
                observer: 3,
                target: 1,
                trust: 4,
                fear: 0,
                obligation: 0,
                affinity: 0,
            },
            Opinion {
                observer: 2,
                target: 1,
                trust: 2,
                fear: 0,
                obligation: 0,
                affinity: 0,
            },
        ],
        events: vec![],
    };
    let contracts = vec![contract(3, 1, 3), contract(1, 1, 2), contract(2, 1, 2)];
    let events = vec![
        AgreementEvent {
            tick: 1,
            contract: 2,
            actor: 1,
            kind: EventKind::Defaulted,
            quantity: 1,
        },
        AgreementEvent {
            tick: 1,
            contract: 3,
            actor: 1,
            kind: EventKind::Fulfilled,
            quantity: 1,
        },
        AgreementEvent {
            tick: 1,
            contract: 1,
            actor: 1,
            kind: EventKind::Fulfilled,
            quantity: 1,
        },
    ];
    state.advance_validated(&contracts, &events, 1);
    assert_eq!(state.index().trust(2, 1), 1);
    assert_eq!(state.index().trust(3, 1), 5);
    assert_eq!(
        state
            .events
            .iter()
            .map(|event| (event.observer, event.target))
            .collect::<Vec<_>>(),
        vec![(2, 1), (3, 1)]
    );
}

#[test]
fn concentrated_experiences_reduce_without_losing_events_or_exceeding_bounds() {
    let contracts: Vec<_> = (1..=10000).rev().map(|id| contract(id, 1, 2)).collect();
    let events: Vec<_> = contracts
        .iter()
        .map(|c| AgreementEvent {
            tick: 1,
            contract: c.id,
            actor: 1,
            kind: EventKind::Fulfilled,
            quantity: 1,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = Opinions {
                    sensitivities: Vec::new().into(),
                    rules: rules(),
                    records: vec![],
                    events: vec![],
                };
                state.advance_validated(&contracts, &events, 1);
                state
            })
    };
    let one = run(1);
    assert_eq!(one, run(4));
    assert_eq!(one.records.len(), 1);
    assert_eq!(one.records[0].trust, 5);
    assert_eq!(one.events.len(), 1);
}

#[test]
fn affinity_is_directed_and_reflects_help_harm_and_unpressured_encounters() {
    let mut rules = rules();
    rules.encounter_gain = 1;
    rules.affinity = Some(AffinityRules {
        limit: 5,
        encounter_change: 1,
        help_change: 2,
        harm_change_per_damage: -1,

        pressured_encounter_change: 0,
    });
    let mut state = Opinions {
        rules,
        sensitivities: vec![].into(),
        records: vec![],
        events: vec![],
    };
    state.learn_help(&[(1, 2)]);
    assert_eq!(state.index().affinity(1, 2), 2);
    assert_eq!(state.index().affinity(2, 1), 0);
    state.learn_harm(&[(1, 2, 4)]);
    assert_eq!(state.index().affinity(1, 2), -2);
    let meetings = [game_parallel_social::Encounter {
        tick: 1,
        region: 1,
        a: 1,
        b: 2,
        pressured: None,
    }];
    state.advance_with_social(&[], &[], &meetings, 1);
    assert_eq!(state.index().affinity(1, 2), -1);
    assert_eq!(state.index().affinity(2, 1), 1);
    assert_eq!(state.index().trust(1, 2), 1);
    let pressured = [game_parallel_social::Encounter {
        tick: 2,
        region: 1,
        a: 1,
        b: 2,
        pressured: Some(1),
    }];
    state.advance_with_social(&[], &[], &pressured, 2);
    assert_eq!(state.index().affinity(1, 2), -1);
    assert_eq!(state.index().affinity(2, 1), 2);
    state.learn_help(&[(1, 2); 10]);
    assert_eq!(state.index().affinity(1, 2), 5);
}

#[test]
fn individuals_can_interpret_the_same_experience_in_opposite_ways() {
    let mut rules = rules();
    rules.affinity = Some(AffinityRules {
        limit: 20,
        encounter_change: 0,
        pressured_encounter_change: 0,
        help_change: 0,
        harm_change_per_damage: 0,
    });
    let personal = |person, help, harm, encounter, pressured| Sensitivity {
        person,
        report_reception: None,
        affinity_report_discrepancy: None,
        trust_report_discrepancy: None,
        trust_promise_fulfilled: None,
        trust_promise_unfulfilled: None,
        fear_per_damage: None,
        gratitude_gain: None,
        obligation_relief: None,
        theft_loss: None,
        confrontation: None,
        affinity_help: Some(help),
        affinity_harm_per_damage: Some(harm),
        affinity_encounter: Some(encounter),
        affinity_pressured_encounter: Some(pressured),
        affinity_site_use: None,
        affinity_site_destruction: None,
        affinity_site_salvage: None,
        affinity_work_success: None,
        affinity_work_refusal: None,
        opposed_action_reaction: None,
        confinement_reaction: None,
        relationship_cases: vec![],
        obligation_development: None,
        promise_obligation_gain: None,
    };
    let mut state = Opinions {
        rules,
        records: vec![],
        events: vec![],
        sensitivities: vec![personal(1, -2, 3, -1, 2), personal(3, 2, -2, 0, -3)].into(),
    };
    state.learn_help(&[(1, 2), (3, 2)]);
    state.learn_harm(&[(1, 2, 2), (3, 2, 2)]);
    assert_eq!(state.index().affinity(1, 2), 4);
    assert_eq!(state.index().affinity(3, 2), -2);
    let events = [
        game_parallel_social::Encounter {
            tick: 1,
            region: 1,
            a: 1,
            b: 2,
            pressured: Some(1),
        },
        game_parallel_social::Encounter {
            tick: 1,
            region: 1,
            a: 2,
            b: 3,
            pressured: Some(3),
        },
    ];
    state.advance_with_social(&[], &[], &events, 1);
    assert_eq!(state.index().affinity(1, 2), 6);
    assert_eq!(state.index().affinity(3, 2), -5);
    assert_eq!(state.index().affinity(2, 1), 0);
}

#[test]
fn a_person_can_interpret_the_same_harm_differently_by_relationship() {
    let mut rules = rules();
    rules.affinity = Some(AffinityRules {
        limit: 20,
        encounter_change: 0,
        pressured_encounter_change: 0,
        help_change: 0,
        harm_change_per_damage: 0,
    });
    let sensitivity = Sensitivity {
        person: 1,
        report_reception: None,
        affinity_report_discrepancy: None,
        trust_report_discrepancy: None,
        trust_promise_fulfilled: None,
        trust_promise_unfulfilled: None,
        fear_per_damage: None,
        gratitude_gain: Some(0),
        obligation_relief: None,
        theft_loss: None,
        confrontation: None,
        affinity_help: None,
        affinity_harm_per_damage: Some(3),
        affinity_encounter: None,
        affinity_pressured_encounter: None,
        affinity_site_use: None,
        affinity_site_destruction: None,
        affinity_site_salvage: None,
        affinity_work_success: None,
        affinity_work_refusal: None,
        opposed_action_reaction: None,
        confinement_reaction: None,
        relationship_cases: vec![RelationshipCase {
            target: 4,
            report_discrepancy: None,
            report_discrepancy_trust: None,
            promise_fulfilled_trust: None,
            promise_unfulfilled_trust: None,
            help: Some(-2),
            harm_per_damage: Some(-2),
            encounter: None,
            pressured_encounter: None,
            site_use: None,
            site_destruction: None,
            site_salvage: None,
            work_success: None,
            work_refusal: None,
            opposed_action_reaction: None,
            confinement_reaction: None,
            obligation_gain: None,
            obligation_relief: None,
            promise_obligation_gain: None,
        }],
        promise_obligation_gain: None,
        obligation_development: None,
    };
    let mut state = Opinions {
        rules,
        sensitivities: vec![sensitivity].into(),
        records: vec![],
        events: vec![],
    };
    state.validate(&[1, 2, 4].into_iter().collect(), 0).unwrap();
    state.learn_harm(&[(1, 2, 2), (1, 4, 2)]);
    state.learn_help(&[(1, 4)]);
    assert_eq!(state.index().affinity(1, 2), 6);
    assert_eq!(state.index().affinity(1, 4), -6);
    assert_eq!(state.index().obligation(1, 4), 0);
    let mut invalid = state.clone();
    let duplicate = invalid.sensitivities[0].relationship_cases[0].clone();
    invalid.sensitivities_mut()[0]
        .relationship_cases
        .push(duplicate);
    assert!(
        invalid
            .validate(&[1, 2, 4].into_iter().collect(), 0)
            .is_err()
    );
}

#[test]
fn obligation_from_help_depends_on_the_person_and_relationship() {
    let sensitivity = Sensitivity {
        person: 1,
        report_reception: None,
        affinity_report_discrepancy: None,
        trust_report_discrepancy: None,
        trust_promise_fulfilled: None,
        trust_promise_unfulfilled: None,
        fear_per_damage: None,
        gratitude_gain: Some(0),
        obligation_relief: Some(0),
        theft_loss: None,
        confrontation: None,
        affinity_help: None,
        affinity_harm_per_damage: None,
        affinity_encounter: None,
        affinity_pressured_encounter: None,
        affinity_site_use: None,
        affinity_site_destruction: None,
        affinity_site_salvage: None,
        affinity_work_success: None,
        affinity_work_refusal: None,
        opposed_action_reaction: None,
        confinement_reaction: None,
        relationship_cases: vec![RelationshipCase {
            target: 4,
            report_discrepancy: None,
            report_discrepancy_trust: None,
            promise_fulfilled_trust: None,
            promise_unfulfilled_trust: None,
            help: None,
            harm_per_damage: None,
            encounter: None,
            pressured_encounter: None,
            site_use: None,
            site_destruction: None,
            site_salvage: None,
            work_success: None,
            work_refusal: None,
            opposed_action_reaction: None,
            confinement_reaction: None,
            obligation_gain: Some(4),
            obligation_relief: Some(2),
            promise_obligation_gain: None,
        }],
        promise_obligation_gain: None,
        obligation_development: None,
    };
    let mut state = Opinions {
        rules: rules(),
        sensitivities: vec![sensitivity].into(),
        records: vec![],
        events: vec![],
    };
    state.learn_help(&[(1, 2), (1, 4)]);
    assert_eq!(state.index().obligation(1, 2), 0);
    assert_eq!(state.index().obligation(1, 4), 4);
    state.learn_help(&[(4, 1)]);
    assert_eq!(state.index().obligation(1, 4), 2);
    assert_eq!(state.index().obligation(4, 1), 0);
}
