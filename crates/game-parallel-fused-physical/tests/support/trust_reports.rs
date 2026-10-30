use game_parallel::{AgreementDecision, AgreementProposal, AgreementStatus, World};
use game_parallel_fused_physical::Kernel;
fn world() -> World {
    World::from_toml(include_str!(
        "../../../../content/parallel/trust_reports.toml"
    ))
    .unwrap()
}
#[test]
fn directed_distrust_rejects_reports_without_blocking_encounters_or_direct_observations() {
    let mut kernel = Kernel::compile(world()).unwrap();
    kernel.step().unwrap();
    let state = kernel.export();
    assert_eq!(state.social.as_ref().unwrap().events.len(), 1);
    let recipient = state
        .knowledge
        .as_ref()
        .unwrap()
        .minds
        .iter()
        .find(|mind| mind.person == 2)
        .unwrap();
    assert!(
        recipient
            .records
            .iter()
            .any(|record| record.region == 1 && record.witness == 2)
    );
    assert!(!recipient.records.iter().any(|record| record.region == 2));
    let mut reverse = world();
    let minds = &mut reverse.knowledge.as_mut().unwrap().minds;
    let record = minds[0].records.pop().unwrap();
    minds[1].records.push(record);
    let mut kernel = Kernel::compile(reverse).unwrap();
    kernel.step().unwrap();
    let received = kernel.export();
    let mind = received
        .knowledge
        .unwrap()
        .minds
        .into_iter()
        .find(|mind| mind.person == 1)
        .unwrap();
    assert_eq!(
        mind.records
            .iter()
            .find(|record| record.region == 2)
            .unwrap()
            .learned_from,
        Some(2)
    );
}
#[test]
fn a_real_default_changes_later_acceptance_of_that_persons_reports() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/agreements.toml")).unwrap();
    let mut source = world();
    source.opinions.as_mut().unwrap().records.clear();
    source.social.as_mut().unwrap().participants[0].willing = false;
    state.social = source.social;
    state.knowledge = source.knowledge;
    state.opinions = source.opinions;
    let id = state
        .propose_agreements(&[AgreementProposal {
            debtor: 1,
            creditor: 2,
            good: 1,
            quantity: 6,
            due: 1,
        }])
        .unwrap()[0];
    state
        .decide_agreements(&[AgreementDecision {
            contract: id,
            actor: 2,
            accept: true,
        }])
        .unwrap();
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    kernel.step().unwrap();
    let mut defaulted = kernel.export();
    assert_eq!(
        defaulted.agreements.as_ref().unwrap().contracts[0].status,
        AgreementStatus::Defaulted
    );
    assert_eq!(defaulted.opinions.as_ref().unwrap().index().trust(2, 1), -2);
    defaulted.social.as_mut().unwrap().participants[0].willing = true;
    kernel =
        Kernel::compile(World::from_json(&serde_json::to_string(&defaulted).unwrap()).unwrap())
            .unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(result.social.as_ref().unwrap().events.len(), 1);
    assert!(
        !result
            .knowledge
            .unwrap()
            .minds
            .iter()
            .find(|mind| mind.person == 2)
            .unwrap()
            .records
            .iter()
            .any(|record| record.region == 2)
    );
}
#[test]
fn invalid_threshold_and_unsupported_reference_host_are_rejected() {
    let mut state = world();
    let before = state.clone();
    assert!(state.step().is_err());
    assert_eq!(state, before);
    assert!(game_parallel_blocks::Engine::compile(state.clone()).is_err());
    state.opinions.as_mut().unwrap().rules.minimum_report_trust = Some(6);
    assert!(state.validate().is_err());
}

#[test]
fn report_filtering_is_worker_invariant_across_a_large_population_and_reload() {
    use game_parallel::{KnowledgeRecord, Mind, Person, SocialParticipant};
    let count = 3000u64;
    let mut state = world();
    state.rules.hunger_per_tick = 0;
    state.regions[0].people = (1..=count)
        .map(|id| Person {
            id,
            hunger: 0,
            health: 100,
        })
        .collect();
    state.regions[1].people.clear();
    state.social.as_mut().unwrap().participants = (1..=count)
        .map(|person| SocialParticipant {
            person,
            willing: true,
        })
        .collect();
    state.opinions.as_mut().unwrap().records.clear();
    state.opinions.as_mut().unwrap().rules.initial = -1;
    state.knowledge.as_mut().unwrap().minds = (1..=count)
        .map(|person| Mind {
            person,
            records: if person % 2 == 1 {
                vec![KnowledgeRecord {
                    region: 2,
                    food: 4,
                    observed_at: 0,
                    witness: 1,
                    learned_from: None,
                }]
            } else {
                vec![]
            },
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                for tick in 1..=4 {
                    kernel.step().unwrap();
                    if tick == 2 {
                        kernel = Kernel::compile(
                            World::from_json(&serde_json::to_string(&kernel.export()).unwrap())
                                .unwrap(),
                        )
                        .unwrap();
                    }
                }
                kernel.export()
            })
    };
    let result = run(1);
    assert_eq!(result, run(4));
    assert!(
        result
            .knowledge
            .as_ref()
            .unwrap()
            .minds
            .iter()
            .filter(|mind| mind.person % 2 == 0)
            .all(|mind| mind.records.iter().all(|record| record.region != 2))
    );
    result.validate().unwrap();
}

#[test]
fn repeated_encounters_build_trust_and_only_later_admit_reports() {
    let state = World::from_toml(include_str!(
        "../../../../content/parallel/social_trust.toml"
    ))
    .unwrap();
    let mut kernel = Kernel::compile(state).unwrap();
    for (expected_trust, learns) in [(-1, false), (0, false), (1, true)] {
        kernel.step().unwrap();
        let state = kernel.export();
        assert_eq!(
            state.opinions.as_ref().unwrap().index().trust(2, 1),
            expected_trust
        );
        let has_report = state
            .knowledge
            .unwrap()
            .minds
            .iter()
            .find(|mind| mind.person == 2)
            .unwrap()
            .records
            .iter()
            .any(|record| record.region == 2 && record.learned_from == Some(1));
        assert_eq!(has_report, learns);
    }
}
#[test]
fn agreement_default_and_encounter_combine_before_trust_is_clamped() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/agreements.toml")).unwrap();
    let mut source = World::from_toml(include_str!(
        "../../../../content/parallel/social_trust.toml"
    ))
    .unwrap();
    source.opinions.as_mut().unwrap().records.clear();
    state.social = source.social;
    state.knowledge = source.knowledge;
    state.opinions = source.opinions;
    let id = state
        .propose_agreements(&[AgreementProposal {
            debtor: 1,
            creditor: 2,
            good: 1,
            quantity: 6,
            due: 1,
        }])
        .unwrap()[0];
    state
        .decide_agreements(&[AgreementDecision {
            contract: id,
            actor: 2,
            accept: true,
        }])
        .unwrap();
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    assert_eq!(kernel.export().opinions.unwrap().index().trust(2, 1), 1);
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(
        result.agreements.unwrap().contracts[0].status,
        AgreementStatus::Defaulted
    );
    let opinions = result.opinions.unwrap();
    assert_eq!(opinions.index().trust(2, 1), 0);
    let event = opinions
        .events
        .iter()
        .find(|event| event.observer == 2 && event.target == 1)
        .unwrap();
    assert_eq!((event.before, event.after), (1, 0));
}
#[test]
fn relationship_trust_matches_the_editable_reference_and_reload() {
    let mut reference = World::from_toml(include_str!(
        "../../../../content/parallel/social_trust.toml"
    ))
    .unwrap();
    reference
        .opinions
        .as_mut()
        .unwrap()
        .rules
        .minimum_report_trust = None;
    let mut kernel = Kernel::compile(reference.clone()).unwrap();
    for tick in 1..=6 {
        reference.step().unwrap();
        kernel.step().unwrap();
        assert_eq!(kernel.export(), reference);
        if tick == 3 {
            kernel = Kernel::compile(
                World::from_json(&serde_json::to_string(&kernel.export()).unwrap()).unwrap(),
            )
            .unwrap();
        }
    }
}
