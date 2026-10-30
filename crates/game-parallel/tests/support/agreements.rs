use game_parallel::{
    AgreementDecision, AgreementDelivery, AgreementProposal, AgreementStatus, World,
};
fn world() -> World {
    World::from_toml(include_str!("../../../../content/parallel/agreements.toml")).unwrap()
}
#[test]
fn a_consensual_promise_requires_real_partial_and_final_delivery() {
    let mut state = world();
    let ids = state
        .propose_agreements(&[AgreementProposal {
            debtor: 1,
            creditor: 2,
            good: 1,
            quantity: 4,
            due: 3,
        }])
        .unwrap();
    let before = state.clone();
    assert!(
        state
            .deliver_agreements(&[AgreementDelivery {
                contract: ids[0],
                actor: 1,
                quantity: 2
            }])
            .is_err()
    );
    assert_eq!(state, before);
    assert!(
        state
            .decide_agreements(&[AgreementDecision {
                contract: ids[0],
                actor: 1,
                accept: true
            }])
            .is_err()
    );
    state
        .decide_agreements(&[AgreementDecision {
            contract: ids[0],
            actor: 2,
            accept: true,
        }])
        .unwrap();
    state
        .deliver_agreements(&[AgreementDelivery {
            contract: ids[0],
            actor: 1,
            quantity: 2,
        }])
        .unwrap();
    let contract = &state.agreements.as_ref().unwrap().contracts[0];
    assert_eq!(
        (contract.status, contract.delivered),
        (AgreementStatus::Active, 2)
    );
    let cargo = state.goods_cargo.as_ref().unwrap();
    assert_eq!(
        cargo.holdings.iter().find(|h| h.person == 1).unwrap().items[0].quantity,
        3
    );
    assert_eq!(
        cargo.holdings.iter().find(|h| h.person == 2).unwrap().items[0].quantity,
        2
    );
    let before = state.clone();
    assert!(
        state
            .deliver_agreements(&[AgreementDelivery {
                contract: ids[0],
                actor: 1,
                quantity: 3
            }])
            .is_err()
    );
    assert_eq!(state, before);
    state
        .deliver_agreements(&[AgreementDelivery {
            contract: ids[0],
            actor: 1,
            quantity: 2,
        }])
        .unwrap();
    assert_eq!(
        state.agreements.as_ref().unwrap().contracts[0].status,
        AgreementStatus::Fulfilled
    );
    state.validate().unwrap();
    assert_eq!(
        state,
        World::from_json(&serde_json::to_string(&state).unwrap()).unwrap()
    );
}
#[test]
fn failed_physical_delivery_is_atomic_and_an_unfulfilled_promise_defaults() {
    let mut state = world();
    let id = state
        .propose_agreements(&[AgreementProposal {
            debtor: 1,
            creditor: 2,
            good: 1,
            quantity: 6,
            due: 2,
        }])
        .unwrap()[0];
    state
        .decide_agreements(&[AgreementDecision {
            contract: id,
            actor: 2,
            accept: true,
        }])
        .unwrap();
    let before = state.clone();
    assert!(
        state
            .deliver_agreements(&[AgreementDelivery {
                contract: id,
                actor: 1,
                quantity: 6
            }])
            .is_err()
    );
    assert_eq!(state, before);
    for _ in 0..3 {
        state.step().unwrap();
    }
    assert_eq!(
        state.agreements.as_ref().unwrap().contracts[0].status,
        AgreementStatus::Defaulted
    );
    state.validate().unwrap();
}

#[test]
fn an_engaged_participant_cannot_make_or_accept_a_promise() {
    use game_parallel::StartWork;
    let mut state = world();
    let proposal = AgreementProposal {
        debtor: 1,
        creditor: 2,
        good: 1,
        quantity: 1,
        due: 5,
    };
    let id = state
        .propose_agreements(std::slice::from_ref(&proposal))
        .unwrap()[0];
    state
        .start_work(&[StartWork {
            id: 1,
            worker: 2,
            region: 1,
            recipe: 1,
            units: 1,
        }])
        .unwrap();
    let before = state.clone();
    assert!(state.propose_agreements(&[proposal]).is_err());
    assert!(
        state
            .decide_agreements(&[AgreementDecision {
                contract: id,
                actor: 2,
                accept: true
            }])
            .is_err()
    );
    assert_eq!(state, before);
}

#[test]
fn manual_fulfillment_updates_trust_once_and_failed_delivery_does_not() {
    use game_parallel::{OpinionRules, Opinions};
    let mut state = world();
    state.opinions = Some(Opinions {
        sensitivities: Vec::new(),
        rules: OpinionRules {
            minimum: -5,
            maximum: 5,
            initial: 0,
            fulfilled_gain: 2,
            default_loss: 2,
            theft_loss: 0,
            encounter_gain: 0,
            minimum_report_trust: None,
            affinity: None,
            newborn_obligation_development: None,
            fear_per_damage: 0,
            fear_decay_per_tick: 0,
            gratitude_gain: 0,
            obligation_relief: 0,
        },
        records: vec![],
        events: vec![],
    });
    let id = state
        .propose_agreements(&[AgreementProposal {
            debtor: 1,
            creditor: 2,
            good: 1,
            quantity: 2,
            due: 3,
        }])
        .unwrap()[0];
    state
        .decide_agreements(&[AgreementDecision {
            contract: id,
            actor: 2,
            accept: true,
        }])
        .unwrap();
    let before = state.clone();
    assert!(
        state
            .deliver_agreements(&[AgreementDelivery {
                contract: id,
                actor: 1,
                quantity: 3
            }])
            .is_err()
    );
    assert_eq!(state, before);
    state
        .deliver_agreements(&[AgreementDelivery {
            contract: id,
            actor: 1,
            quantity: 2,
        }])
        .unwrap();
    assert_eq!(state.opinions.as_ref().unwrap().index().trust(2, 1), 2);
    state.deliver_agreements(&[]).unwrap();
    state.step().unwrap();
    assert_eq!(state.opinions.as_ref().unwrap().index().trust(2, 1), 2);
}
