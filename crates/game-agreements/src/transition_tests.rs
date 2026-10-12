use crate::*;

const A: PersonId = PersonId(101);
const B: PersonId = PersonId(102);
const C: PersonId = PersonId(103);
const AGREEMENT: AgreementId = AgreementId(1);
const SITE: SiteId = SiteId(201);

fn institutional_duty() -> InstitutionDuty {
    InstitutionDuty {
        institution: InstitutionId(9),
        agreement: AgreementId(10),
        obligation: ObligationId(11),
        term: AgreementTerm::Access {
            grantor: A,
            grantee: B,
            inventory: InventoryId(301),
            allowed: true,
        },
    }
}

#[test]
fn institution_declared_duties_record_debt_without_performing_or_granting_it() {
    let duty = institutional_duty();
    let mut state = fixture();
    let plan = Ledger::new(&state)
        .prepare_institutional_duties(std::slice::from_ref(&duty), SimTime(20))
        .unwrap();
    assert!(plan.permissions.is_empty());
    assert!(agreements(&state).next().is_none());
    assert_eq!(
        plan.events,
        vec![TransitionEvent::Activated {
            agreement: duty.agreement
        }]
    );
    plan.commit(&mut state).unwrap();
    assert_eq!(
        crate::agreement(&state, duty.agreement)
            .unwrap()
            .proposed_at,
        SimTime(20)
    );
    assert_eq!(
        crate::obligation(&state, duty.obligation)
            .unwrap()
            .performed,
        0
    );
    assert!(
        crate::obligation(&state, duty.obligation)
            .unwrap()
            .cargo
            .is_empty()
    );
}

#[test]
fn invalid_declared_duty_batch_cannot_partially_install_other_duties() {
    let duty = institutional_duty();
    let state = fixture();
    let duplicate = InstitutionDuty {
        obligation: ObligationId(12),
        ..duty.clone()
    };
    assert!(
        Ledger::new(&state)
            .prepare_institutional_duties(&[duty.clone(), duplicate], SimTime(20))
            .is_err()
    );
    assert_eq!(state, fixture());
    for invalid in [
        InstitutionDuty {
            institution: InstitutionId(0),
            ..duty.clone()
        },
        InstitutionDuty {
            obligation: ObligationId(9),
            ..duty.clone()
        },
        InstitutionDuty {
            agreement: AgreementId(0),
            ..duty
        },
    ] {
        assert!(
            Ledger::new(&state)
                .prepare_institutional_duties(&[invalid], SimTime(20))
                .is_err()
        );
    }
}

#[test]
fn a_stale_declared_duty_installation_preserves_the_current_ledger() {
    let duty = institutional_duty();
    let mut state = fixture();
    let first = Ledger::new(&state)
        .prepare_institutional_duties(std::slice::from_ref(&duty), SimTime(20))
        .unwrap();
    let stale = first.clone();
    first.commit(&mut state).unwrap();
    let before = state.clone();
    assert!(stale.commit(&mut state).is_err());
    assert_eq!(before, state);
}

fn facts() -> TransitionFacts {
    TransitionFacts {
        actor_available: true,
        present: BTreeSet::from([A, B, C]),
        assets: TermsContext {
            now: SimTime(0),
            sites: BTreeSet::from([SITE, SiteId(202)]),
            inventories: BTreeMap::from([
                (
                    InventoryId(301),
                    InventoryTerms {
                        good: Good::Food,
                        capacity: Quantity(10_000),
                    },
                ),
                (
                    InventoryId(302),
                    InventoryTerms {
                        good: Good::Food,
                        capacity: Quantity(10_000),
                    },
                ),
            ]),
            grants: BTreeSet::from([(PermissionAsset::Inventory(InventoryId(301)), A)]),
            ..TermsContext::default()
        },
    }
}
fn labor(debtor: PersonId, millis: u64, deadline: u64) -> AgreementTerm {
    AgreementTerm::Labor {
        debtor,
        creditor: if debtor == A { B } else { A },
        site: SITE,
        millis,
        deadline: SimTime(deadline),
    }
}
fn procedure(threshold: usize) -> AmendmentProcedure {
    AmendmentProcedure {
        eligible_proposers: BTreeSet::from([A, B]),
        approvers: BTreeSet::from([A, B]),
        threshold,
        term_approvers: BTreeMap::new(),
    }
}
fn perform(
    state: &mut Store,
    actor: PersonId,
    action: AgreementAction,
    facts: &TransitionFacts,
    ids: TransitionIds,
) -> PreparedTransition {
    let plan = Ledger::new(state)
        .prepare_transition(actor, &action, facts, &ids)
        .unwrap();
    plan.clone().commit(state).unwrap();
    plan
}
fn initial(
    state: &mut Store,
    terms: Vec<AgreementTerm>,
    procedure: Option<AmendmentProcedure>,
    facts: &TransitionFacts,
) {
    perform(
        state,
        A,
        AgreementAction::Propose {
            parties: BTreeSet::from([A, B]),
            terms,
            procedure,
        },
        facts,
        TransitionIds {
            agreement: Some(AGREEMENT),
            ..TransitionIds::default()
        },
    );
}
fn activate(state: &mut Store, facts: &TransitionFacts) -> PreparedTransition {
    let count = Ledger::new(state)
        .allocation_needs(&AgreementAction::Accept {
            agreement: AGREEMENT,
        })
        .unwrap()
        .obligations;
    perform(
        state,
        B,
        AgreementAction::Accept {
            agreement: AGREEMENT,
        },
        facts,
        TransitionIds {
            obligations: (2..2 + count as u64).map(ObligationId).collect(),
            ..TransitionIds::default()
        },
    )
}
fn amend(
    state: &mut Store,
    id: u64,
    replacements: BTreeMap<u16, AgreementTerm>,
    facts: &TransitionFacts,
) {
    perform(
        state,
        A,
        AgreementAction::Amend {
            agreement: AGREEMENT,
            replacements,
        },
        facts,
        TransitionIds {
            amendment: Some(AmendmentId(id)),
            ..TransitionIds::default()
        },
    );
}
fn vote(
    state: &mut Store,
    actor: PersonId,
    id: u64,
    support: bool,
    facts: &TransitionFacts,
) -> PreparedTransition {
    perform(
        state,
        actor,
        AgreementAction::Vote {
            amendment: AmendmentId(id),
            support,
        },
        facts,
        TransitionIds::default(),
    )
}

#[test]
fn voluntary_signatures_create_owed_duties_and_permission_effects_only() {
    let mut state = fixture();
    let facts = facts();
    initial(
        &mut state,
        vec![
            labor(A, 100, 1000),
            AgreementTerm::Delivery {
                debtor: A,
                creditor: B,
                source: InventoryId(301),
                destination: InventoryId(302),
                quantity: Quantity(500),
                deadline: SimTime(1000),
            },
            AgreementTerm::Access {
                grantor: A,
                grantee: B,
                inventory: InventoryId(301),
                allowed: true,
            },
        ],
        None,
        &facts,
    );
    assert!(obligations(&state).next().is_none());
    assert_eq!(
        crate::agreement(&state, AGREEMENT).unwrap().accepted,
        BTreeSet::from([A])
    );
    let plan = activate(&mut state, &facts);
    assert_eq!(
        crate::agreement(&state, AGREEMENT).unwrap().status,
        AgreementStatus::Active
    );
    assert_eq!(
        crate::obligation(&state, ObligationId(2))
            .unwrap()
            .performed,
        0
    );
    assert_eq!(
        crate::obligation(&state, ObligationId(3))
            .unwrap()
            .performed,
        0
    );
    assert_eq!(
        crate::obligation(&state, ObligationId(4))
            .unwrap()
            .performed,
        1
    );
    assert_eq!(
        plan.permissions,
        vec![PermissionEffect {
            asset: PermissionAsset::Inventory(InventoryId(301)),
            grantor: A,
            grantee: B,
            allowed: true
        }]
    );
    // There is deliberately no world stock balance in this promise transition.
    assert_eq!(
        plan.events,
        vec![TransitionEvent::Activated {
            agreement: AGREEMENT
        }]
    );
}

#[test]
fn contact_custody_and_rejection_are_real_gates_and_invalid_changes_are_inert() {
    let mut state = fixture();
    let mut facts = facts();
    let propose = AgreementAction::Propose {
        parties: BTreeSet::from([A, B]),
        terms: vec![AgreementTerm::Access {
            grantor: A,
            grantee: B,
            inventory: InventoryId(301),
            allowed: true,
        }],
        procedure: None,
    };
    let ids = TransitionIds {
        agreement: Some(AGREEMENT),
        ..TransitionIds::default()
    };
    facts.present.remove(&B);
    assert_eq!(
        Ledger::new(&state)
            .prepare_transition(A, &propose, &facts, &ids)
            .unwrap_err(),
        Error::Unavailable
    );
    facts.present.insert(B);
    facts.assets.grants.clear();
    assert_eq!(
        Ledger::new(&state)
            .prepare_transition(A, &propose, &facts, &ids)
            .unwrap_err(),
        Error::Unavailable
    );
    assert_eq!(state, fixture());
    facts
        .assets
        .grants
        .insert((PermissionAsset::Inventory(InventoryId(301)), A));
    perform(&mut state, A, propose, &facts, ids);
    let before = state.clone();
    assert_eq!(
        Ledger::new(&state)
            .prepare_transition(
                C,
                &AgreementAction::Accept {
                    agreement: AGREEMENT
                },
                &facts,
                &TransitionIds {
                    obligations: vec![ObligationId(2)],
                    ..TransitionIds::default()
                }
            )
            .unwrap_err(),
        Error::Unavailable
    );
    assert_eq!(state, before);
    perform(
        &mut state,
        B,
        AgreementAction::Reject {
            agreement: AGREEMENT,
        },
        &facts,
        TransitionIds::default(),
    );
    assert_eq!(
        crate::agreement(&state, AGREEMENT).unwrap().status,
        AgreementStatus::Rejected
    );
    assert!(obligations(&state).next().is_none());
}
#[test]
fn counteroffer_withdraws_old_offer_without_accepting_the_replacement() {
    let mut state = fixture();
    let facts = facts();
    initial(&mut state, vec![labor(A, 100, 1000)], None, &facts);
    let plan = perform(
        &mut state,
        B,
        AgreementAction::Counteroffer {
            agreement: AGREEMENT,
            terms: vec![labor(A, 50, 1000)],
            procedure: Some(procedure(1)),
        },
        &facts,
        TransitionIds {
            agreement: Some(AgreementId(9)),
            ..TransitionIds::default()
        },
    );
    assert_eq!(
        crate::agreement(&state, AGREEMENT).unwrap().status,
        AgreementStatus::Withdrawn
    );
    assert_eq!(
        crate::agreement(&state, AgreementId(9)).unwrap().accepted,
        BTreeSet::from([B])
    );
    assert_eq!(
        crate::agreement(&state, AgreementId(9)).unwrap().status,
        AgreementStatus::Proposed
    );
    assert!(obligations(&state).next().is_none());
    assert_eq!(
        plan.notices.iter().map(|n| n.agreement).collect::<Vec<_>>(),
        vec![AgreementId(9), AGREEMENT]
    );
}

#[test]
fn chosen_quorum_and_term_approvers_determine_changes_and_invalidate_stale_proposals() {
    let mut state = fixture();
    let facts = facts();
    let mut rule = procedure(1);
    rule.term_approvers.insert(0, BTreeSet::from([B]));
    initial(&mut state, vec![labor(A, 100, 1000)], Some(rule), &facts);
    activate(&mut state, &facts);
    amend(
        &mut state,
        10,
        BTreeMap::from([(0, labor(A, 200, 1000))]),
        &facts,
    );
    amend(
        &mut state,
        11,
        BTreeMap::from([(0, labor(A, 300, 1000))]),
        &facts,
    );
    vote(&mut state, A, 10, true, &facts);
    assert_eq!(
        amendment(&state, AmendmentId(10)).unwrap().status,
        AmendmentStatus::Proposed
    );
    let stale = Ledger::new(&state)
        .prepare_transition(
            B,
            &AgreementAction::Vote {
                amendment: AmendmentId(11),
                support: true,
            },
            &facts,
            &TransitionIds::default(),
        )
        .unwrap();
    vote(&mut state, B, 10, true, &facts);
    assert_eq!(crate::agreement(&state, AGREEMENT).unwrap().revision, 1);
    assert_eq!(
        crate::obligation(&state, ObligationId(2)).unwrap().amount(),
        200
    );
    assert_eq!(
        amendment(&state, AmendmentId(11)).unwrap().status,
        AmendmentStatus::Rejected
    );
    let before = state.clone();
    assert_eq!(stale.commit(&mut state), Err(Error::Unavailable));
    assert_eq!(state, before);
}

#[test]
fn no_universal_forgiveness_or_increase_consent_rule_is_invented() {
    let facts = facts();
    let mut state = fixture();
    initial(&mut state, vec![labor(A, 100, 1000)], None, &facts);
    activate(&mut state, &facts);
    assert_eq!(
        Ledger::new(&state)
            .prepare_transition(
                B,
                &AgreementAction::Amend {
                    agreement: AGREEMENT,
                    replacements: BTreeMap::from([(0, labor(A, 0, 1000))])
                },
                &facts,
                &TransitionIds {
                    amendment: Some(AmendmentId(10)),
                    ..TransitionIds::default()
                }
            )
            .unwrap_err(),
        Error::Unavailable
    );
    let mut chosen = fixture();
    let mut rule = procedure(1);
    rule.approvers = BTreeSet::from([B]);
    initial(&mut chosen, vec![labor(A, 100, 1000)], Some(rule), &facts);
    activate(&mut chosen, &facts);
    amend(
        &mut chosen,
        10,
        BTreeMap::from([(0, labor(A, 200, 1000))]),
        &facts,
    );
    vote(&mut chosen, B, 10, true, &facts);
    assert_eq!(
        crate::obligation(&chosen, ObligationId(2))
            .unwrap()
            .amount(),
        200
    ); // A chose this original procedure.
}

#[test]
fn negative_votes_reject_when_named_procedure_can_no_longer_pass() {
    let facts = facts();
    let mut state = fixture();
    initial(
        &mut state,
        vec![labor(A, 100, 1000)],
        Some(procedure(2)),
        &facts,
    );
    activate(&mut state, &facts);
    amend(
        &mut state,
        10,
        BTreeMap::from([(0, labor(A, 200, 1000))]),
        &facts,
    );
    vote(&mut state, B, 10, false, &facts);
    assert_eq!(
        amendment(&state, AmendmentId(10)).unwrap().status,
        AmendmentStatus::Rejected
    );
    assert_eq!(crate::agreement(&state, AGREEMENT).unwrap().revision, 0);
}

#[test]
fn actual_work_survives_same_duty_amendment_and_changed_duty_defaults_without_free_credit() {
    let mut state = fixture();
    let mut facts = facts();
    initial(
        &mut state,
        vec![labor(A, 100, 1000)],
        Some(procedure(1)),
        &facts,
    );
    activate(&mut state, &facts);
    Ledger::new(&state)
        .prepare_labor(
            SimTime(0),
            SimTime(30),
            &[WorkAssignment {
                actor: A,
                target: WorkTarget::Site(SITE),
            }],
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
    facts.assets.now = SimTime(30);
    amend(
        &mut state,
        10,
        BTreeMap::from([(0, labor(A, 200, 1000))]),
        &facts,
    );
    let plan = vote(&mut state, A, 10, true, &facts);
    assert_eq!(
        crate::obligation(&state, ObligationId(2))
            .unwrap()
            .performed,
        30
    );
    assert!(pending_works(&state).next().is_none());
    assert_eq!(
        plan.events[0],
        TransitionEvent::Performed {
            obligation: ObligationId(2),
            actor: A,
            quantity: Quantity(30)
        }
    );
    Ledger::new(&state)
        .prepare_labor(
            SimTime(30),
            SimTime(50),
            &[WorkAssignment {
                actor: A,
                target: WorkTarget::Site(SITE),
            }],
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
    facts.assets.now = SimTime(50);
    amend(
        &mut state,
        11,
        BTreeMap::from([(0, labor(B, 80, 40))]),
        &facts,
    );
    let plan = vote(&mut state, A, 11, true, &facts);
    assert_eq!(
        plan.events[0],
        TransitionEvent::Performed {
            obligation: ObligationId(2),
            actor: A,
            quantity: Quantity(20)
        }
    );
    assert_eq!(
        crate::obligation(&state, ObligationId(2))
            .unwrap()
            .performed,
        0
    );
    assert!(
        crate::obligation(&state, ObligationId(2))
            .unwrap()
            .defaulted
    );
    assert_eq!(
        crate::agreement(&state, AGREEMENT).unwrap().status,
        AgreementStatus::Breached
    );
}

#[test]
fn prepared_vote_cannot_overwrite_actual_performance_and_pending_credit() {
    let mut state = fixture();
    let facts = facts();
    initial(
        &mut state,
        vec![labor(A, 100, 1000)],
        Some(procedure(1)),
        &facts,
    );
    activate(&mut state, &facts);
    amend(
        &mut state,
        10,
        BTreeMap::from([(0, labor(A, 200, 1000))]),
        &facts,
    );
    let plan = Ledger::new(&state)
        .prepare_transition(
            A,
            &AgreementAction::Vote {
                amendment: AmendmentId(10),
                support: true,
            },
            &facts,
            &TransitionIds::default(),
        )
        .unwrap();
    Ledger::new(&state)
        .prepare_labor(
            SimTime(0),
            SimTime(10),
            &[WorkAssignment {
                actor: A,
                target: WorkTarget::Site(SITE),
            }],
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let before = state.clone();
    assert_eq!(plan.commit(&mut state), Err(Error::Unavailable));
    assert_eq!(state, before);
}

#[test]
fn amendment_keeps_actual_fractional_output_debt_when_future_share_changes() {
    let mut state = fixture();
    let mut facts = facts();
    facts
        .assets
        .outputs
        .insert((SITE, Good::Food), InventoryId(301));
    let share = |denominator| AgreementTerm::OutputShare {
        debtor: A,
        creditor: B,
        site: SITE,
        good: Good::Food,
        numerator: 1,
        denominator,
        destination: InventoryId(302),
        pay_by: SimTime(2000),
        ends_at: SimTime(1000),
    };
    initial(&mut state, vec![share(3)], Some(procedure(1)), &facts);
    activate(&mut state, &facts);
    Ledger::new(&state)
        .prepare_production(ProductionFact {
            actor: A,
            site: SITE,
            good: Good::Food,
            quantity: Quantity(1),
            time: SimTime(10),
        })
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        (
            crate::obligation(&state, ObligationId(2))
                .unwrap()
                .share_remainder,
            crate::obligation(&state, ObligationId(2))
                .unwrap()
                .share_scale
        ),
        (1, 3)
    );
    facts.assets.now = SimTime(10);
    amend(&mut state, 10, BTreeMap::from([(0, share(2))]), &facts);
    vote(&mut state, A, 10, true, &facts);
    Ledger::new(&state)
        .prepare_production(ProductionFact {
            actor: A,
            site: SITE,
            good: Good::Food,
            quantity: Quantity(2),
            time: SimTime(20),
        })
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let owed = &crate::obligation(&state, ObligationId(2)).unwrap();
    assert_eq!(
        (owed.accrued, owed.share_remainder, owed.share_scale),
        (1, 1, 3)
    );
    assert_eq!(owed.performed, 0);
}

#[test]
fn prepared_adoption_rejects_a_later_concurrent_proposal_without_partial_changes() {
    let mut state = fixture();
    let facts = facts();
    initial(
        &mut state,
        vec![labor(A, 100, 1000)],
        Some(procedure(1)),
        &facts,
    );
    activate(&mut state, &facts);
    amend(
        &mut state,
        10,
        BTreeMap::from([(0, labor(A, 200, 1000))]),
        &facts,
    );
    let prepared = Ledger::new(&state)
        .prepare_transition(
            A,
            &AgreementAction::Vote {
                amendment: AmendmentId(10),
                support: true,
            },
            &facts,
            &TransitionIds::default(),
        )
        .unwrap();
    amend(
        &mut state,
        11,
        BTreeMap::from([(0, labor(A, 300, 1000))]),
        &facts,
    );
    let before = state.clone();
    assert_eq!(prepared.commit(&mut state), Err(Error::Unavailable));
    assert_eq!(state, before);
}

#[test]
fn service_commitment_needs_exact_heard_terms_and_resolved_counteroffer() {
    let state = fixture();
    let facts = facts();
    let consent = HeardServiceConsent {
        recruitment: RecruitmentId(400),
        sponsor: A,
        recruit: B,
        offered_terms: vec![labor(A, 100, 1000)],
        accepted_terms: vec![labor(A, 100, 1000)],
        unanswered_counteroffer: false,
    };
    let ids = TransitionIds {
        agreement: Some(AGREEMENT),
        obligations: vec![ObligationId(2)],
        ..Default::default()
    };
    assert!(
        Ledger::new(&state)
            .prepare_service_commitment(
                &HeardServiceConsent {
                    unanswered_counteroffer: true,
                    ..consent.clone()
                },
                &facts,
                &ids
            )
            .is_err()
    );
    assert!(
        Ledger::new(&state)
            .prepare_service_commitment(
                &HeardServiceConsent {
                    accepted_terms: vec![labor(A, 200, 1000)],
                    ..consent.clone()
                },
                &facts,
                &ids
            )
            .is_err()
    );
    let mut missing = facts.clone();
    missing.present.remove(&B);
    assert!(
        Ledger::new(&state)
            .prepare_service_commitment(&consent, &missing, &ids)
            .is_err()
    );
    let plan = Ledger::new(&state)
        .prepare_service_commitment(&consent, &facts, &ids)
        .unwrap();
    assert_eq!(
        plan.events,
        vec![
            TransitionEvent::Proposed {
                agreement: AGREEMENT,
                proposer: A
            },
            TransitionEvent::Activated {
                agreement: AGREEMENT
            }
        ]
    );
    let mut after = state;
    plan.commit(&mut after).unwrap();
    assert_eq!(
        crate::agreement(&after, AGREEMENT).unwrap().accepted,
        BTreeSet::from([A, B])
    );
    assert_eq!(
        crate::obligation(&after, ObligationId(2))
            .unwrap()
            .performed,
        0
    );
}
