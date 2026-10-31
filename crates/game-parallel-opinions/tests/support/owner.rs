use super::*;
#[test]
fn witnessed_takings_are_combined_per_pair_without_losing_other_feelings() {
    let mut state = Opinions {
        rules: rules(),
        sensitivities: vec![],
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
                relationship_cases: vec![],
                obligation_development: None,
                promise_obligation_gain: None,
            },
            Sensitivity {
                person: 3,
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
                relationship_cases: vec![],
                obligation_development: None,
                promise_obligation_gain: None,
            },
        ],
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
            relationship_cases: vec![],
            obligation_development: None,
            promise_obligation_gain: None,
        }],
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
        sensitivities: vec![],
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
                relationship_cases: vec![],
                obligation_development: None,
                promise_obligation_gain: None,
            },
            Sensitivity {
                person: 3,
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
                relationship_cases: vec![],
                obligation_development: None,
                promise_obligation_gain: None,
            },
        ],
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
        sensitivities: Vec::new(),
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
        sensitivities: Vec::new(),
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
                    sensitivities: Vec::new(),
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
        sensitivities: Vec::new(),
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
                    sensitivities: Vec::new(),
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
        sensitivities: vec![],
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
        relationship_cases: vec![],
        obligation_development: None,
        promise_obligation_gain: None,
    };
    let mut state = Opinions {
        rules,
        records: vec![],
        events: vec![],
        sensitivities: vec![personal(1, -2, 3, -1, 2), personal(3, 2, -2, 0, -3)],
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
        relationship_cases: vec![RelationshipCase {
            target: 4,
            help: Some(-2),
            harm_per_damage: Some(-2),
            encounter: None,
            pressured_encounter: None,
            site_use: None,
            site_destruction: None,
            obligation_gain: None,
            obligation_relief: None,
            promise_obligation_gain: None,
        }],
        promise_obligation_gain: None,
        obligation_development: None,
    };
    let mut state = Opinions {
        rules,
        sensitivities: vec![sensitivity],
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
    invalid.sensitivities[0].relationship_cases.push(duplicate);
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
        relationship_cases: vec![RelationshipCase {
            target: 4,
            help: None,
            harm_per_damage: None,
            encounter: None,
            pressured_encounter: None,
            site_use: None,
            site_destruction: None,
            obligation_gain: Some(4),
            obligation_relief: Some(2),
            promise_obligation_gain: None,
        }],
        promise_obligation_gain: None,
        obligation_development: None,
    };
    let mut state = Opinions {
        rules: rules(),
        sensitivities: vec![sensitivity],
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
