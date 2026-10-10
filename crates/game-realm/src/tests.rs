use super::*;
use game_agreements::AgreementTerm;
use game_core::*;
use game_missions::{MissionObjective, MissionProposal};
use std::collections::{BTreeMap, BTreeSet};
fn witness(observer: PersonId, area: u64, observed_at: u64) -> LocalWitness {
    LocalWitness {
        observer,
        observer_area: AreaId(area),
        statement_area: AreaId(area),
        observed_at: SimTime(observed_at),
    }
}

fn setup() -> (World, ProposalFacts, RealmCommitmentId) {
    let a = InstitutionId(11);
    let b = InstitutionId(12);
    let id = RealmCommitmentId(100);
    let facts = ProposalFacts {
        representatives: BTreeMap::from([
            (
                a,
                RepresentativeFacts {
                    settlement: AreaId(1),
                    representative: PersonId(1),
                    may_negotiate: true,
                    delivery_caps: BTreeMap::from([(InventoryId(1), Quantity(2_000))]),
                    supply_caps: BTreeMap::from([
                        (InventoryId(1), Quantity(2_000)),
                        (InventoryId(5), Quantity(1_000)),
                    ]),
                    passage_areas: BTreeSet::from([AreaId(1)]),
                    protection_areas: BTreeSet::from([AreaId(1), AreaId(2)]),
                },
            ),
            (
                b,
                RepresentativeFacts {
                    settlement: AreaId(2),
                    representative: PersonId(2),
                    may_negotiate: true,
                    delivery_caps: BTreeMap::from([(InventoryId(3), Quantity(2_000))]),
                    supply_caps: BTreeMap::new(),
                    passage_areas: BTreeSet::from([AreaId(2)]),
                    protection_areas: BTreeSet::from([AreaId(2)]),
                },
            ),
        ]),
        known_people: BTreeSet::from([PersonId(1), PersonId(2), PersonId(3)]),
        known_areas: BTreeSet::from([AreaId(1), AreaId(2)]),
        inventories: BTreeMap::from([
            (
                InventoryId(1),
                InventoryFacts {
                    good: Good::Food,
                    capacity: Some(Quantity(5_000)),
                },
            ),
            (
                InventoryId(2),
                InventoryFacts {
                    good: Good::Food,
                    capacity: Some(Quantity(5_000)),
                },
            ),
            (
                InventoryId(3),
                InventoryFacts {
                    good: Good::Timber,
                    capacity: Some(Quantity(5_000)),
                },
            ),
            (
                InventoryId(4),
                InventoryFacts {
                    good: Good::Timber,
                    capacity: Some(Quantity(5_000)),
                },
            ),
            (
                InventoryId(5),
                InventoryFacts {
                    good: Good::Tool,
                    capacity: Some(Quantity(1_000)),
                },
            ),
            (
                InventoryId(7),
                InventoryFacts {
                    good: Good::Food,
                    capacity: Some(Quantity(10_000)),
                },
            ),
        ]),
    };
    let proposal = Proposal {
        proposer: PersonId(1),
        participants: BTreeMap::from([
            (
                a,
                Participant {
                    institution: a,
                    settlement: AreaId(1),
                    representative: PersonId(1),
                },
            ),
            (
                b,
                Participant {
                    institution: b,
                    settlement: AreaId(2),
                    representative: PersonId(2),
                },
            ),
        ]),
        terms: vec![
            Term::Delivery {
                provider: a,
                beneficiary: b,
                term: AgreementTerm::Delivery {
                    debtor: PersonId(1),
                    creditor: PersonId(2),
                    source: InventoryId(1),
                    destination: InventoryId(2),
                    quantity: Quantity(1_000),
                    deadline: SimTime(1_000),
                },
            },
            Term::Delivery {
                provider: b,
                beneficiary: a,
                term: AgreementTerm::Delivery {
                    debtor: PersonId(2),
                    creditor: PersonId(1),
                    source: InventoryId(3),
                    destination: InventoryId(4),
                    quantity: Quantity(1_000),
                    deadline: SimTime(1_000),
                },
            },
        ],
        ratify_by: SimTime(100),
    };
    let mut w = World::default();
    w.prepare_propose(id, proposal, &facts, SimTime(0))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    (w, facts, id)
}
fn bind_decide(
    w: &mut World,
    id: RealmCommitmentId,
    institution: InstitutionId,
    outcome: Decision,
    time: u64,
) {
    let proposal = RuleProposalId(institution.0 + 100);
    w.prepare_bind_ratification(id, 0, institution, proposal, SimTime(time))
        .unwrap()
        .commit(w)
        .unwrap();
    w.prepare_decision(
        id,
        0,
        institution,
        Ratification {
            proposal,
            outcome,
            decided_at: SimTime(time),
        },
        SimTime(time),
    )
    .unwrap()
    .commit(w)
    .unwrap();
}
fn ratify(w: &mut World, id: RealmCommitmentId) {
    bind_decide(w, id, InstitutionId(11), Decision::Adopted, 1);
    bind_decide(w, id, InstitutionId(12), Decision::Adopted, 2);
}
fn learn_approvals(w: &mut World, id: RealmCommitmentId, actor: PersonId) {
    w.prepare_observe_proposal(witness(actor, 1, 2), id, 0, SimTime(2))
        .unwrap()
        .commit(w)
        .unwrap();
    for i in [InstitutionId(11), InstitutionId(12)] {
        w.prepare_observe_decision(witness(actor, i.0 - 10, i.0 - 10), id, 0, i, SimTime(2))
            .unwrap()
            .commit(w)
            .unwrap();
    }
}
fn key(id: RealmCommitmentId, index: u16) -> TermKey {
    TermKey {
        commitment: id,
        revision: 0,
        index,
    }
}
fn back_delivery(w: &mut World, id: RealmCommitmentId, index: u16, at: u64) -> ObligationId {
    let Term::Delivery { term, .. } = &w.commitments[&id].current().proposal.terms[index as usize]
    else {
        panic!()
    };
    let obligation = ObligationId(201 + u64::from(index));
    w.prepare_backing(
        key(id, index),
        WorkReceipt::DeliveryOpened {
            agreement: AgreementId(301 + u64::from(index)),
            obligation: Some(obligation),
            accepted: true,
            term: term.clone(),
        },
        SimTime(at),
    )
    .unwrap()
    .commit(w)
    .unwrap();
    obligation
}

#[test]
fn native_ratification_requests_require_received_terms_and_registered_decisions() {
    let (mut w, facts, id) = setup();
    assert!(
        w.prepare_ratification_request(id, InstitutionId(12), PersonId(2), &facts, SimTime(0))
            .is_err()
    );
    let packet = w.prepare_report(PersonId(1), id, SimTime(0)).unwrap();
    w.prepare_receive_report(PersonId(2), &packet, SimTime(1))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    assert!(matches!(
        w.prepare_ratification_request(id, InstitutionId(12), PersonId(2), &facts, SimTime(1))
            .unwrap()
            .requests[0],
        Request::Ratification {
            institution: InstitutionId(12),
            ..
        }
    ));
    let before = w.clone();
    assert!(
        w.prepare_decision(
            id,
            0,
            InstitutionId(12),
            Ratification {
                proposal: RuleProposalId(999),
                outcome: Decision::Adopted,
                decided_at: SimTime(1)
            },
            SimTime(1)
        )
        .is_err()
    );
    assert_eq!(w, before);
    ratify(&mut w, id);
    assert_eq!(w.commitments[&id].current().status(), Status::Ratified);
    assert!(w.commitments[&id].current().backed.is_empty());
}
#[test]
fn unseen_remote_approval_cannot_unlock_work_and_reports_do_not_refresh_globally() {
    let (mut w, facts, id) = setup();
    ratify(&mut w, id);
    w.prepare_observe_decision(
        witness(PersonId(1), 1, 1),
        id,
        0,
        InstitutionId(11),
        SimTime(2),
    )
    .unwrap()
    .commit(&mut w)
    .unwrap();
    let packet = w.prepare_report(PersonId(1), id, SimTime(2)).unwrap();
    assert!(
        w.prepare_work_request(key(id, 0), PersonId(1), &facts, SimTime(3))
            .is_err()
    );
    w.prepare_receive_report(PersonId(2), &packet, SimTime(3))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    assert_eq!(
        w.view(PersonId(2), id).unwrap().revisions[&0]
            .ratifications
            .len(),
        1
    );
    assert!(
        !w.view(PersonId(2), id).unwrap().revisions[&0]
            .ratifications
            .contains_key(&InstitutionId(12))
    );
    let before = w.clone();
    let _ = w.view(PersonId(1), id);
    assert_eq!(w, before);
}
#[test]
fn ratified_terms_request_real_work_once_and_only_actual_receipts_fulfill_delivery() {
    let (mut w, facts, id) = setup();
    ratify(&mut w, id);
    learn_approvals(&mut w, id, PersonId(1));
    let request = w
        .prepare_work_request(key(id, 0), PersonId(1), &facts, SimTime(3))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    assert!(matches!(request[0], Request::Delivery { .. }));
    assert!(
        w.prepare_work_request(key(id, 0), PersonId(1), &facts, SimTime(3))
            .is_err()
    );
    assert!(
        w.prepare_outcome(
            key(id, 0),
            OutcomeReceipt::Delivery {
                obligation: ObligationId(201),
                performed: Quantity(1_000),
                defaulted: false
            },
            SimTime(4)
        )
        .is_err()
    );
    let obligation = back_delivery(&mut w, id, 0, 4);
    assert_eq!(
        w.commitments[&id].current().backed[&0].performed,
        Quantity(0)
    );
    w.prepare_outcome(
        key(id, 0),
        OutcomeReceipt::Delivery {
            obligation,
            performed: Quantity(400),
            defaulted: false,
        },
        SimTime(5),
    )
    .unwrap()
    .commit(&mut w)
    .unwrap();
    assert_eq!(w.commitments[&id].current().status(), Status::Active);
    w.prepare_outcome(
        key(id, 0),
        OutcomeReceipt::Delivery {
            obligation,
            performed: Quantity(1_000),
            defaulted: false,
        },
        SimTime(6),
    )
    .unwrap()
    .commit(&mut w)
    .unwrap();
    assert_eq!(
        w.commitments[&id].current().backed[&0].progress,
        Progress::Fulfilled
    );
    assert_ne!(w.commitments[&id].current().status(), Status::Fulfilled);
}
#[test]
fn aggregate_caps_corrupt_work_and_progress_regressions_are_inert() {
    let (mut w, facts, id) = setup();
    let mut proposal = w.commitments[&id].current().proposal.clone();
    proposal
        .terms
        .extend([proposal.terms[0].clone(), proposal.terms[0].clone()]);
    assert_eq!(
        proposal.validate(&facts, SimTime(0)),
        Err(Error::Unavailable)
    );
    ratify(&mut w, id);
    let obligation = back_delivery(&mut w, id, 0, 3);
    let wrong = WorkReceipt::DeliveryOpened {
        agreement: AgreementId(900),
        obligation: Some(ObligationId(901)),
        accepted: true,
        term: AgreementTerm::Delivery {
            debtor: PersonId(1),
            creditor: PersonId(2),
            source: InventoryId(1),
            destination: InventoryId(2),
            quantity: Quantity(2_000),
            deadline: SimTime(1_000),
        },
    };
    assert!(w.prepare_backing(key(id, 0), wrong, SimTime(4)).is_err());
    w.prepare_outcome(
        key(id, 0),
        OutcomeReceipt::Delivery {
            obligation,
            performed: Quantity(600),
            defaulted: false,
        },
        SimTime(4),
    )
    .unwrap()
    .commit(&mut w)
    .unwrap();
    let before = w.clone();
    for (foreign, performed, time) in [
        (ObligationId(999), Quantity(700), 5),
        (obligation, Quantity(500), 5),
        (obligation, Quantity(700), 3),
        (obligation, Quantity(1_001), 5),
    ] {
        assert!(
            w.prepare_outcome(
                key(id, 0),
                OutcomeReceipt::Delivery {
                    obligation: foreign,
                    performed,
                    defaulted: false
                },
                SimTime(time)
            )
            .is_err()
        );
    }
    assert_eq!(w, before);
}
#[test]
fn revised_representatives_keep_previous_unpaid_backing_and_debt_progress() {
    let (mut w, mut facts, id) = setup();
    ratify(&mut w, id);
    let obligation = back_delivery(&mut w, id, 0, 3);
    w.prepare_outcome(
        key(id, 0),
        OutcomeReceipt::Delivery {
            obligation,
            performed: Quantity(200),
            defaulted: true,
        },
        SimTime(1_001),
    )
    .unwrap()
    .commit(&mut w)
    .unwrap();
    let old = w.commitments[&id].current().clone();
    let mut successor = old.proposal.clone();
    successor.proposer = PersonId(3);
    successor.ratify_by = SimTime(1_100);
    successor
        .participants
        .get_mut(&InstitutionId(11))
        .unwrap()
        .representative = PersonId(3);
    for t in &mut successor.terms {
        let Term::Delivery {
            term:
                AgreementTerm::Delivery {
                    debtor,
                    creditor,
                    deadline,
                    ..
                },
            ..
        } = t
        else {
            panic!()
        };
        if *debtor == PersonId(1) {
            *debtor = PersonId(3)
        }
        if *creditor == PersonId(1) {
            *creditor = PersonId(3)
        }
        *deadline = SimTime(2_000);
    }
    facts
        .representatives
        .get_mut(&InstitutionId(11))
        .unwrap()
        .representative = PersonId(3);
    // The replacement has received the old arrangement through an ordinary report.
    w.prepare_observe_proposal(witness(PersonId(3), 1, 1_001), id, 0, SimTime(1_001))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    w.prepare_revise(id, successor, &facts, SimTime(1_002))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    assert_eq!(w.commitments[&id].revisions[0], old);
    assert_eq!(
        w.commitments[&id].current().status(),
        Status::AwaitingRatification
    );
    w.prepare_outcome(
        key(id, 0),
        OutcomeReceipt::Delivery {
            obligation,
            performed: Quantity(1_000),
            defaulted: true,
        },
        SimTime(1_003),
    )
    .unwrap()
    .commit(&mut w)
    .unwrap();
    let prior = &w.commitments[&id].revisions[0].backed[&0];
    assert_eq!(prior.progress, Progress::Fulfilled);
    assert_eq!(prior.defaulted_at, Some(SimTime(1_001)));
    assert!(
        w.prepare_decision(
            id,
            0,
            InstitutionId(11),
            Ratification {
                proposal: RuleProposalId(111),
                outcome: Decision::Adopted,
                decided_at: SimTime(1)
            },
            SimTime(1_003)
        )
        .is_err()
    );
}
#[test]
fn refusal_and_deadlines_do_not_invent_ratification_or_cancel_existing_backing() {
    let (mut w, _, id) = setup();
    bind_decide(&mut w, id, InstitutionId(11), Decision::Rejected, 1);
    assert_eq!(w.commitments[&id].current().status(), Status::Rejected);
    let (mut expired, _, other) = setup();
    assert!(expired.prepare_expire(other, SimTime(100)).is_err());
    expired
        .prepare_expire(other, SimTime(101))
        .unwrap()
        .commit(&mut expired)
        .unwrap();
    assert_eq!(
        expired.commitments[&other].current().status(),
        Status::Expired
    );
    assert!(
        expired
            .prepare_bind_ratification(
                other,
                0,
                InstitutionId(11),
                RuleProposalId(111),
                SimTime(101)
            )
            .is_err()
    );
    let (mut ratified, _, id) = setup();
    ratify(&mut ratified, id);
    let _ = back_delivery(&mut ratified, id, 0, 3);
    assert!(ratified.prepare_expire(id, SimTime(101)).is_err());
    assert!(!ratified.commitments[&id].current().backed.is_empty());
}
#[test]
fn frozen_reports_keep_old_physical_progress_and_merge_new_fields_independently() {
    let (mut w, _, id) = setup();
    ratify(&mut w, id);
    let obligation = back_delivery(&mut w, id, 0, 3);
    learn_approvals(&mut w, id, PersonId(1));
    w.prepare_observe_work(witness(PersonId(1), 1, 3), key(id, 0), SimTime(3))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    let frozen = w.prepare_report(PersonId(1), id, SimTime(3)).unwrap();
    w.prepare_outcome(
        key(id, 0),
        OutcomeReceipt::Delivery {
            obligation,
            performed: Quantity(1_000),
            defaulted: false,
        },
        SimTime(4),
    )
    .unwrap()
    .commit(&mut w)
    .unwrap();
    w.prepare_receive_report(PersonId(2), &frozen, SimTime(5))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    let receiver = &w.view(PersonId(2), id).unwrap().revisions[&0];
    assert_eq!(receiver.backed[&0].value.performed, Quantity(0));
    assert_eq!(receiver.backed[&0].observed_at, SimTime(3));
    assert_eq!(receiver.backed[&0].learned_at, SimTime(5));
    assert_eq!(
        receiver.backed[&0].source,
        KnowledgeSource::Reported(PersonId(1))
    );
    w.prepare_observe_work(witness(PersonId(2), 2, 4), key(id, 0), SimTime(6))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    w.prepare_receive_report(PersonId(2), &frozen, SimTime(7))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    assert_eq!(
        w.view(PersonId(2), id).unwrap().revisions[&0].backed[&0]
            .value
            .performed,
        Quantity(1_000)
    );
    let bytes = serde_json::to_vec(&w).unwrap();
    let restored: World = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(restored, w);
}
#[test]
fn stale_prepared_changes_do_not_partially_replace_new_proposals_or_knowledge() {
    let (mut w, facts, id) = setup();
    let stale = w
        .prepare_revise(
            id,
            w.commitments[&id].current().proposal.clone(),
            &facts,
            SimTime(1),
        )
        .unwrap();
    w.prepare_observe_proposal(witness(PersonId(1), 1, 2), id, 0, SimTime(2))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    let before = w.clone();
    assert_eq!(stale.commit(&mut w), Err(Error::Unavailable));
    assert_eq!(w, before);
}
#[test]
fn passage_and_protection_require_exact_owner_work_and_never_guarantee_safety() {
    let (base, facts, id) = setup();
    let mut proposal = base.commitments[&id].current().proposal.clone();
    let patrol = MissionProposal {
        leader: PersonId(3),
        recruits: BTreeSet::new(),
        objective: MissionObjective::Patrol {
            areas: BTreeSet::from([AreaId(2)]),
            until: SimTime(800),
        },
        home: AreaId(1),
        supplies: vec![],
        compensation: BTreeMap::new(),
        deadline: SimTime(900),
    };
    proposal.terms = vec![
        Term::Passage {
            grantor: InstitutionId(12),
            beneficiary: InstitutionId(11),
            areas: BTreeSet::from([AreaId(2)]),
            until: SimTime(800),
        },
        Term::Protection {
            provider: InstitutionId(11),
            beneficiary: InstitutionId(12),
            patrol: patrol.clone(),
        },
    ];
    let mut w = World::default();
    w.prepare_propose(id, proposal, &facts, SimTime(0))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    ratify(&mut w, id);
    w.prepare_backing(
        key(id, 0),
        WorkReceipt::PassageApplied {
            institution: InstitutionId(12),
            proposal: RuleProposalId(500),
            adopted: true,
            beneficiary: InstitutionId(11),
            areas: BTreeSet::from([AreaId(2)]),
            until: SimTime(800),
        },
        SimTime(3),
    )
    .unwrap()
    .commit(&mut w)
    .unwrap();
    w.prepare_backing(
        key(id, 1),
        WorkReceipt::ProtectionOpened {
            mission: MissionId(501),
            accepted: true,
            patrol,
        },
        SimTime(3),
    )
    .unwrap()
    .commit(&mut w)
    .unwrap();
    assert!(
        w.prepare_outcome(
            key(id, 1),
            OutcomeReceipt::Protection {
                mission: MissionId(501),
                progress: Progress::Fulfilled
            },
            SimTime(799)
        )
        .is_err()
    );
    w.prepare_outcome(
        key(id, 0),
        OutcomeReceipt::Passage {
            proposal: RuleProposalId(500),
            available: false,
        },
        SimTime(400),
    )
    .unwrap()
    .commit(&mut w)
    .unwrap();
    w.prepare_outcome(
        key(id, 0),
        OutcomeReceipt::Passage {
            proposal: RuleProposalId(500),
            available: true,
        },
        SimTime(800),
    )
    .unwrap()
    .commit(&mut w)
    .unwrap();
    w.prepare_outcome(
        key(id, 1),
        OutcomeReceipt::Protection {
            mission: MissionId(501),
            progress: Progress::Fulfilled,
        },
        SimTime(800),
    )
    .unwrap()
    .commit(&mut w)
    .unwrap();
    assert_eq!(w.commitments[&id].current().status(), Status::Blocked);
    assert_eq!(
        w.commitments[&id].current().backed[&0].defaulted_at,
        Some(SimTime(400))
    );
}

fn references(w: &World, f: &ProposalFacts) -> References {
    let mut refs = References {
        people: f.known_people.clone(),
        areas: f.known_areas.clone(),
        institutions: f.representatives.keys().copied().collect(),
        inventories: f.inventories.keys().copied().collect(),
        ..References::default()
    };
    for r in w.commitments.values().flat_map(|c| &c.revisions) {
        refs.proposals.extend(r.ratification_proposals.values());
        for b in r.backed.values() {
            match b.backing {
                Backing::Delivery {
                    agreement,
                    obligation,
                } => {
                    refs.agreements.insert(agreement);
                    refs.obligations.extend(obligation);
                }
                Backing::Passage { proposal, .. } => {
                    refs.proposals.insert(proposal);
                }
                Backing::Protection { mission } => {
                    refs.missions.insert(mission);
                }
            }
        }
    }
    refs
}
#[test]
fn local_presence_and_snapshot_dates_reject_remote_or_retroactive_witnesses() {
    let (mut w, _, id) = setup();
    ratify(&mut w, id);
    w.prepare_observe_proposal(witness(PersonId(2), 2, 2), id, 0, SimTime(2))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    let before = w.clone();
    assert!(
        w.prepare_observe_decision(
            LocalWitness {
                observer: PersonId(2),
                observer_area: AreaId(2),
                statement_area: AreaId(1),
                observed_at: SimTime(1)
            },
            id,
            0,
            InstitutionId(11),
            SimTime(2)
        )
        .is_err()
    );
    assert!(
        w.prepare_observe_decision(
            witness(PersonId(2), 1, 2),
            id,
            0,
            InstitutionId(11),
            SimTime(2)
        )
        .is_err()
    );
    assert_eq!(w, before);
    let mut packet = w.prepare_report(PersonId(1), id, SimTime(2)).unwrap();
    packet
        .snapshot
        .revisions
        .get_mut(&0)
        .unwrap()
        .proposal
        .learned_at = SimTime(3);
    assert!(
        w.prepare_receive_report(PersonId(2), &packet, SimTime(4))
            .is_err()
    );
}
#[test]
fn current_saves_validate_owner_references_revisions_sources_and_progress() {
    let (mut w, f, id) = setup();
    ratify(&mut w, id);
    let _ = back_delivery(&mut w, id, 0, 3);
    learn_approvals(&mut w, id, PersonId(1));
    w.prepare_observe_work(witness(PersonId(1), 1, 3), key(id, 0), SimTime(3))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    let refs = references(&w, &f);
    assert_eq!(w.validate(&refs, SimTime(4)), Ok(()));
    let saved = serde_json::to_vec(&w).unwrap();
    let restored: World = serde_json::from_slice(&saved).unwrap();
    assert_eq!(restored.validate(&refs, SimTime(4)), Ok(()));
    let mut wrong = w.clone();
    wrong.commitments.get_mut(&id).unwrap().revisions[0].number = 1;
    assert!(wrong.validate(&refs, SimTime(4)).is_err());
    let mut wrong = w.clone();
    wrong
        .known
        .get_mut(&PersonId(1))
        .unwrap()
        .get_mut(&id)
        .unwrap()
        .revisions
        .get_mut(&0)
        .unwrap()
        .proposal
        .source = KnowledgeSource::Reported(PersonId(999));
    assert!(wrong.validate(&refs, SimTime(4)).is_err());
    let mut wrong = w.clone();
    wrong.commitments.get_mut(&id).unwrap().revisions[0]
        .backed
        .get_mut(&0)
        .unwrap()
        .progress = Progress::Fulfilled;
    assert!(wrong.validate(&refs, SimTime(4)).is_err());
    let mut missing = refs;
    missing.obligations.clear();
    assert!(w.validate(&missing, SimTime(4)).is_err());
}
#[test]
fn a_single_delivery_obligation_cannot_be_counted_for_multiple_regional_promises() {
    let (mut w, _, id) = setup();
    ratify(&mut w, id);
    let obligation = back_delivery(&mut w, id, 0, 3);
    let Term::Delivery { term, .. } = w.commitments[&id].current().proposal.terms[1].clone() else {
        panic!()
    };
    assert!(
        w.prepare_backing(
            key(id, 1),
            WorkReceipt::DeliveryOpened {
                agreement: AgreementId(999),
                obligation: Some(obligation),
                accepted: true,
                term
            },
            SimTime(4)
        )
        .is_err()
    );
    let before = w.clone();
    assert!(
        w.prepare_decision(
            id,
            1,
            InstitutionId(11),
            Ratification {
                proposal: RuleProposalId(111),
                outcome: Decision::Adopted,
                decided_at: SimTime(1)
            },
            SimTime(4)
        )
        .is_err()
    );
    assert_eq!(w, before);
}

#[test]
fn passage_expiry_and_later_withdrawal_do_not_breach_the_ended_permission() {
    let (base, facts, id) = setup();
    let mut proposal = base.commitments[&id].current().proposal.clone();
    proposal.terms = vec![Term::Passage {
        grantor: InstitutionId(12),
        beneficiary: InstitutionId(11),
        areas: BTreeSet::from([AreaId(2)]),
        until: SimTime(800),
    }];
    let mut w = World::default();
    w.prepare_propose(id, proposal, &facts, SimTime(0))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    ratify(&mut w, id);
    w.prepare_backing(
        key(id, 0),
        WorkReceipt::PassageApplied {
            institution: InstitutionId(12),
            proposal: RuleProposalId(500),
            adopted: true,
            beneficiary: InstitutionId(11),
            areas: BTreeSet::from([AreaId(2)]),
            until: SimTime(800),
        },
        SimTime(3),
    )
    .unwrap()
    .commit(&mut w)
    .unwrap();
    for at in [800, 801] {
        w.prepare_outcome(
            key(id, 0),
            OutcomeReceipt::Passage {
                proposal: RuleProposalId(500),
                available: false,
            },
            SimTime(at),
        )
        .unwrap()
        .commit(&mut w)
        .unwrap();
    }
    assert_eq!(w.commitments[&id].current().status(), Status::Fulfilled);
    assert_eq!(w.commitments[&id].current().backed[&0].defaulted_at, None);
}

#[test]
fn independent_native_results_have_stable_order_and_reject_late_or_foreign_decisions() {
    let (mut first, _, id) = setup();
    for institution in [InstitutionId(11), InstitutionId(12)] {
        first
            .prepare_bind_ratification(
                id,
                0,
                institution,
                RuleProposalId(institution.0 + 100),
                SimTime(0),
            )
            .unwrap()
            .commit(&mut first)
            .unwrap();
    }
    let mut reversed = first.clone();
    for order in [[11, 12], [12, 11]] {
        let world = if order[0] == 11 {
            &mut first
        } else {
            &mut reversed
        };
        for number in order {
            world
                .prepare_decision(
                    id,
                    0,
                    InstitutionId(number),
                    Ratification {
                        proposal: RuleProposalId(number + 100),
                        outcome: Decision::Adopted,
                        decided_at: SimTime(number - 10),
                    },
                    SimTime(2),
                )
                .unwrap()
                .commit(world)
                .unwrap();
        }
    }
    assert_eq!(first, reversed);
    let (mut pending, _, id) = setup();
    pending
        .prepare_bind_ratification(id, 0, InstitutionId(11), RuleProposalId(111), SimTime(0))
        .unwrap()
        .commit(&mut pending)
        .unwrap();
    let before = pending.clone();
    for (institution, time) in [(InstitutionId(11), 101), (InstitutionId(99), 1)] {
        assert!(
            pending
                .prepare_decision(
                    id,
                    0,
                    institution,
                    Ratification {
                        proposal: RuleProposalId(111),
                        outcome: Decision::Adopted,
                        decided_at: SimTime(time)
                    },
                    SimTime(time)
                )
                .is_err()
        );
    }
    assert_eq!(pending, before);
}

#[test]
fn a_new_unratified_draft_does_not_revoke_old_approved_work_or_impose_a_compartment_limit() {
    let (mut w, mut facts, id) = setup();
    ratify(&mut w, id);
    learn_approvals(&mut w, id, PersonId(1));
    facts.inventories.get_mut(&InventoryId(2)).unwrap().capacity = Some(Quantity(100));
    facts.inventories.get_mut(&InventoryId(4)).unwrap().capacity = None;
    assert_eq!(
        w.commitments[&id]
            .current()
            .proposal
            .validate(&facts, SimTime(0)),
        Ok(())
    );
    let mut proposal = w.commitments[&id].current().proposal.clone();
    proposal.ratify_by = SimTime(200);
    w.prepare_revise(id, proposal, &facts, SimTime(3))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    assert_eq!(
        w.commitments[&id].current().status(),
        Status::AwaitingRatification
    );
    let requested = w
        .prepare_work_request(key(id, 0), PersonId(1), &facts, SimTime(4))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    assert!(matches!(requested[0], Request::Delivery { .. }));
    assert!(w.commitments[&id].revisions[0].requested.contains(&0));
    assert!(w.commitments[&id].current().requested.is_empty());
}

#[test]
fn real_pending_owner_proposals_are_reported_without_creating_obligations_or_acceptance() {
    let (mut w, f, id) = setup();
    ratify(&mut w, id);
    learn_approvals(&mut w, id, PersonId(1));
    let Term::Delivery { term, .. } = w.commitments[&id].current().proposal.terms[0].clone() else {
        panic!()
    };
    w.prepare_backing(
        key(id, 0),
        WorkReceipt::DeliveryProposed {
            agreement: AgreementId(301),
            term: term.clone(),
        },
        SimTime(3),
    )
    .unwrap()
    .commit(&mut w)
    .unwrap();
    assert_eq!(w.commitments[&id].current().status(), Status::Ratified);
    assert_eq!(
        w.commitments[&id].current().backed[&0].backing,
        Backing::Delivery {
            agreement: AgreementId(301),
            obligation: None
        }
    );
    w.prepare_observe_work(witness(PersonId(1), 1, 3), key(id, 0), SimTime(3))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    let frozen = w.prepare_report(PersonId(1), id, SimTime(3)).unwrap();
    w.prepare_backing(
        key(id, 0),
        WorkReceipt::DeliveryOpened {
            agreement: AgreementId(301),
            obligation: Some(ObligationId(201)),
            accepted: true,
            term,
        },
        SimTime(4),
    )
    .unwrap()
    .commit(&mut w)
    .unwrap();
    w.prepare_receive_report(PersonId(2), &frozen, SimTime(5))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    assert_eq!(
        w.view(PersonId(2), id).unwrap().revisions[&0].backed[&0]
            .value
            .progress,
        Progress::AwaitingWork
    );
    assert_eq!(w.validate(&references(&w, &f), SimTime(5)), Ok(()));
}
#[test]
fn owner_cache_projection_keeps_old_dates_and_fresh_local_observations_remain_valid() {
    let (mut w, f, id) = setup();
    ratify(&mut w, id);
    let obligation = back_delivery(&mut w, id, 0, 3);
    learn_approvals(&mut w, id, PersonId(1));
    w.prepare_outcome(
        key(id, 0),
        OutcomeReceipt::Delivery {
            obligation,
            performed: Quantity(1_000),
            defaulted: false,
        },
        SimTime(4),
    )
    .unwrap()
    .commit(&mut w)
    .unwrap();
    let old = BackedTerm {
        backing: Backing::Delivery {
            agreement: AgreementId(301),
            obligation: Some(obligation),
        },
        progress: Progress::Active,
        performed: Quantity(0),
        changed_at: SimTime(3),
        defaulted_at: None,
    };
    w.prepare_learn_work(
        PersonId(1),
        key(id, 0),
        Dated {
            value: old,
            observed_at: SimTime(3),
            learned_at: SimTime(6),
            source: KnowledgeSource::Reported(PersonId(2)),
        },
        SimTime(6),
    )
    .unwrap()
    .commit(&mut w)
    .unwrap();
    assert_eq!(
        w.view(PersonId(1), id).unwrap().revisions[&0].backed[&0]
            .value
            .performed,
        Quantity(0)
    );
    assert_eq!(
        w.view(PersonId(1), id).unwrap().revisions[&0].backed[&0].observed_at,
        SimTime(3)
    );
    let fresh = BackedTerm {
        backing: Backing::Delivery {
            agreement: AgreementId(301),
            obligation: Some(obligation),
        },
        progress: Progress::Fulfilled,
        performed: Quantity(1_000),
        changed_at: SimTime(7),
        defaulted_at: None,
    };
    w.prepare_learn_work(
        PersonId(1),
        key(id, 0),
        Dated {
            value: fresh,
            observed_at: SimTime(7),
            learned_at: SimTime(7),
            source: KnowledgeSource::Observed(PersonId(1)),
        },
        SimTime(7),
    )
    .unwrap()
    .commit(&mut w)
    .unwrap();
    let report = w.prepare_report(PersonId(1), id, SimTime(7)).unwrap();
    w.prepare_receive_report(PersonId(2), &report, SimTime(8))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    assert_eq!(w.validate(&references(&w, &f), SimTime(8)), Ok(()));
}
