use game_ecs::{Schema, StableId, Store};
fn store() -> Store {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    Store::with_schema(schema)
}
fn restore(bytes: &[u8]) -> Store {
    let snapshot = serde_json::from_slice(bytes).unwrap();
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    Store::restore(schema, snapshot).unwrap()
}
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

fn setup() -> (Store, ProposalFacts, RealmCommitmentId) {
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
    let mut w = store();
    Ledger::new(&w)
        .prepare_propose(id, proposal, &facts, SimTime(0))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    (w, facts, id)
}
fn bind_decide(
    w: &mut Store,
    id: RealmCommitmentId,
    institution: InstitutionId,
    outcome: Decision,
    time: u64,
) {
    let proposal = RuleProposalId(institution.0 + 100);
    Ledger::new(w)
        .prepare_bind_ratification(id, 0, institution, proposal, SimTime(time))
        .unwrap()
        .commit(w)
        .unwrap();
    Ledger::new(w)
        .prepare_decision(
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
fn ratify(w: &mut Store, id: RealmCommitmentId) {
    bind_decide(w, id, InstitutionId(11), Decision::Adopted, 1);
    bind_decide(w, id, InstitutionId(12), Decision::Adopted, 2);
}
fn learn_approvals(w: &mut Store, id: RealmCommitmentId, actor: PersonId) {
    Ledger::new(w)
        .prepare_observe_proposal(witness(actor, 1, 2), id, 0, SimTime(2))
        .unwrap()
        .commit(w)
        .unwrap();
    for i in [InstitutionId(11), InstitutionId(12)] {
        Ledger::new(w)
            .prepare_observe_decision(witness(actor, i.0 - 10, i.0 - 10), id, 0, i, SimTime(2))
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
fn back_delivery(w: &mut Store, id: RealmCommitmentId, index: u16, at: u64) -> ObligationId {
    let Term::Delivery { term, .. } =
        &commitment(w, id).unwrap().current().proposal.terms[index as usize]
    else {
        panic!()
    };
    let obligation = ObligationId(201 + u64::from(index));
    Ledger::new(w)
        .prepare_backing(
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
        Ledger::new(&w)
            .prepare_ratification_request(id, InstitutionId(12), PersonId(2), &facts, SimTime(0))
            .is_err()
    );
    let packet = Ledger::new(&w)
        .prepare_report(PersonId(1), id, SimTime(0))
        .unwrap();
    Ledger::new(&w)
        .prepare_receive_report(PersonId(2), &packet, SimTime(1))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    assert!(matches!(
        Ledger::new(&w)
            .prepare_ratification_request(id, InstitutionId(12), PersonId(2), &facts, SimTime(1))
            .unwrap()
            .requests[0],
        Request::Ratification {
            institution: InstitutionId(12),
            ..
        }
    ));
    let before = w.clone();
    assert!(
        Ledger::new(&w)
            .prepare_decision(
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
    assert_eq!(
        commitment(&w, id).unwrap().current().status(),
        Status::Ratified
    );
    assert!(commitment(&w, id).unwrap().current().backed.is_empty());
}
#[test]
fn unseen_remote_approval_cannot_unlock_work_and_reports_do_not_refresh_globally() {
    let (mut w, facts, id) = setup();
    ratify(&mut w, id);
    Ledger::new(&w)
        .prepare_observe_decision(
            witness(PersonId(1), 1, 1),
            id,
            0,
            InstitutionId(11),
            SimTime(2),
        )
        .unwrap()
        .commit(&mut w)
        .unwrap();
    let packet = Ledger::new(&w)
        .prepare_report(PersonId(1), id, SimTime(2))
        .unwrap();
    assert!(
        Ledger::new(&w)
            .prepare_work_request(key(id, 0), PersonId(1), &facts, SimTime(3))
            .is_err()
    );
    Ledger::new(&w)
        .prepare_receive_report(PersonId(2), &packet, SimTime(3))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    assert_eq!(
        Ledger::new(&w).view(PersonId(2), id).unwrap().revisions[&0]
            .ratifications
            .len(),
        1
    );
    assert!(
        !Ledger::new(&w).view(PersonId(2), id).unwrap().revisions[&0]
            .ratifications
            .contains_key(&InstitutionId(12))
    );
    let before = w.clone();
    let _ = Ledger::new(&w).view(PersonId(1), id);
    assert_eq!(w, before);
}
#[test]
fn ratified_terms_request_real_work_once_and_only_actual_receipts_fulfill_delivery() {
    let (mut w, facts, id) = setup();
    ratify(&mut w, id);
    learn_approvals(&mut w, id, PersonId(1));
    let request = Ledger::new(&w)
        .prepare_work_request(key(id, 0), PersonId(1), &facts, SimTime(3))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    assert!(matches!(request[0], Request::Delivery { .. }));
    assert!(
        Ledger::new(&w)
            .prepare_work_request(key(id, 0), PersonId(1), &facts, SimTime(3))
            .is_err()
    );
    assert!(
        Ledger::new(&w)
            .prepare_outcome(
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
        commitment(&w, id).unwrap().current().backed[&0].performed,
        Quantity(0)
    );
    Ledger::new(&w)
        .prepare_outcome(
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
    assert_eq!(
        commitment(&w, id).unwrap().current().status(),
        Status::Active
    );
    Ledger::new(&w)
        .prepare_outcome(
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
        commitment(&w, id).unwrap().current().backed[&0].progress,
        Progress::Fulfilled
    );
    assert_ne!(
        commitment(&w, id).unwrap().current().status(),
        Status::Fulfilled
    );
}
#[test]
fn aggregate_caps_corrupt_work_and_progress_regressions_are_inert() {
    let (mut w, facts, id) = setup();
    let mut proposal = commitment(&w, id).unwrap().current().proposal.clone();
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
    assert!(
        Ledger::new(&w)
            .prepare_backing(key(id, 0), wrong, SimTime(4))
            .is_err()
    );
    Ledger::new(&w)
        .prepare_outcome(
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
            Ledger::new(&w)
                .prepare_outcome(
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
    Ledger::new(&w)
        .prepare_outcome(
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
    let old = commitment(&w, id).unwrap().current().clone();
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
    Ledger::new(&w)
        .prepare_observe_proposal(witness(PersonId(3), 1, 1_001), id, 0, SimTime(1_001))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    Ledger::new(&w)
        .prepare_revise(id, successor, &facts, SimTime(1_002))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    assert_eq!(commitment(&w, id).unwrap().revisions[0], old);
    assert_eq!(
        commitment(&w, id).unwrap().current().status(),
        Status::AwaitingRatification
    );
    Ledger::new(&w)
        .prepare_outcome(
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
    let prior = &commitment(&w, id).unwrap().revisions[0].backed[&0];
    assert_eq!(prior.progress, Progress::Fulfilled);
    assert_eq!(prior.defaulted_at, Some(SimTime(1_001)));
    assert!(
        Ledger::new(&w)
            .prepare_decision(
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
    assert_eq!(
        commitment(&w, id).unwrap().current().status(),
        Status::Rejected
    );
    let (mut expired, _, other) = setup();
    assert!(
        Ledger::new(&expired)
            .prepare_expire(other, SimTime(100))
            .is_err()
    );
    Ledger::new(&expired)
        .prepare_expire(other, SimTime(101))
        .unwrap()
        .commit(&mut expired)
        .unwrap();
    assert_eq!(
        commitment(&expired, other).unwrap().current().status(),
        Status::Expired
    );
    assert!(
        Ledger::new(&expired)
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
    assert!(
        Ledger::new(&ratified)
            .prepare_expire(id, SimTime(101))
            .is_err()
    );
    assert!(
        !commitment(&ratified, id)
            .unwrap()
            .current()
            .backed
            .is_empty()
    );
}
#[test]
fn frozen_reports_keep_old_physical_progress_and_merge_new_fields_independently() {
    let (mut w, _, id) = setup();
    ratify(&mut w, id);
    let obligation = back_delivery(&mut w, id, 0, 3);
    learn_approvals(&mut w, id, PersonId(1));
    Ledger::new(&w)
        .prepare_observe_work(witness(PersonId(1), 1, 3), key(id, 0), SimTime(3))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    let frozen = Ledger::new(&w)
        .prepare_report(PersonId(1), id, SimTime(3))
        .unwrap();
    Ledger::new(&w)
        .prepare_outcome(
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
    Ledger::new(&w)
        .prepare_receive_report(PersonId(2), &frozen, SimTime(5))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    let receiver = &Ledger::new(&w).view(PersonId(2), id).unwrap().revisions[&0];
    assert_eq!(receiver.backed[&0].value.performed, Quantity(0));
    assert_eq!(receiver.backed[&0].observed_at, SimTime(3));
    assert_eq!(receiver.backed[&0].learned_at, SimTime(5));
    assert_eq!(
        receiver.backed[&0].source,
        KnowledgeSource::Reported(PersonId(1))
    );
    Ledger::new(&w)
        .prepare_observe_work(witness(PersonId(2), 2, 4), key(id, 0), SimTime(6))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    Ledger::new(&w)
        .prepare_receive_report(PersonId(2), &frozen, SimTime(7))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    assert_eq!(
        Ledger::new(&w).view(PersonId(2), id).unwrap().revisions[&0].backed[&0]
            .value
            .performed,
        Quantity(1_000)
    );
    let bytes = serde_json::to_vec(&w.snapshot().unwrap()).unwrap();
    let restored = restore(&bytes);
    assert_eq!(restored, w);
}
#[test]
fn stale_prepared_changes_do_not_partially_replace_new_proposals_or_knowledge() {
    let (mut w, facts, id) = setup();
    let stale = Ledger::new(&w)
        .prepare_revise(
            id,
            commitment(&w, id).unwrap().current().proposal.clone(),
            &facts,
            SimTime(1),
        )
        .unwrap();
    Ledger::new(&w)
        .prepare_observe_proposal(witness(PersonId(1), 1, 2), id, 0, SimTime(2))
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
    let mut proposal = commitment(&base, id).unwrap().current().proposal.clone();
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
    let mut w = store();
    Ledger::new(&w)
        .prepare_propose(id, proposal, &facts, SimTime(0))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    ratify(&mut w, id);
    Ledger::new(&w)
        .prepare_backing(
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
    Ledger::new(&w)
        .prepare_backing(
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
        Ledger::new(&w)
            .prepare_outcome(
                key(id, 1),
                OutcomeReceipt::Protection {
                    mission: MissionId(501),
                    progress: Progress::Fulfilled
                },
                SimTime(799)
            )
            .is_err()
    );
    Ledger::new(&w)
        .prepare_outcome(
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
    Ledger::new(&w)
        .prepare_outcome(
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
    Ledger::new(&w)
        .prepare_outcome(
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
    assert_eq!(
        commitment(&w, id).unwrap().current().status(),
        Status::Blocked
    );
    assert_eq!(
        commitment(&w, id).unwrap().current().backed[&0].defaulted_at,
        Some(SimTime(400))
    );
}

fn references(w: &Store, f: &ProposalFacts) -> References {
    let mut refs = References {
        people: f.known_people.clone(),
        areas: f.known_areas.clone(),
        institutions: f.representatives.keys().copied().collect(),
        inventories: f.inventories.keys().copied().collect(),
        ..References::default()
    };
    for r in commitments(w).map(|(_, c)| c).flat_map(|c| &c.revisions) {
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
    Ledger::new(&w)
        .prepare_observe_proposal(witness(PersonId(2), 2, 2), id, 0, SimTime(2))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    let before = w.clone();
    assert!(
        Ledger::new(&w)
            .prepare_observe_decision(
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
        Ledger::new(&w)
            .prepare_observe_decision(
                witness(PersonId(2), 1, 2),
                id,
                0,
                InstitutionId(11),
                SimTime(2)
            )
            .is_err()
    );
    assert_eq!(w, before);
    let mut packet = Ledger::new(&w)
        .prepare_report(PersonId(1), id, SimTime(2))
        .unwrap();
    packet
        .snapshot
        .revisions
        .get_mut(&0)
        .unwrap()
        .proposal
        .learned_at = SimTime(3);
    assert!(
        Ledger::new(&w)
            .prepare_receive_report(PersonId(2), &packet, SimTime(4))
            .is_err()
    );
}
#[test]
fn current_saves_validate_owner_references_revisions_sources_and_progress() {
    let (mut w, f, id) = setup();
    ratify(&mut w, id);
    let _ = back_delivery(&mut w, id, 0, 3);
    learn_approvals(&mut w, id, PersonId(1));
    Ledger::new(&w)
        .prepare_observe_work(witness(PersonId(1), 1, 3), key(id, 0), SimTime(3))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    let refs = references(&w, &f);
    assert_eq!(Ledger::new(&w).validate(&refs, SimTime(4)), Ok(()));
    let saved = serde_json::to_vec(&w.snapshot().unwrap()).unwrap();
    let restored = restore(&saved);
    assert_eq!(Ledger::new(&restored).validate(&refs, SimTime(4)), Ok(()));
    let mut wrong = w.clone();
    wrong
        .get_mut::<Commitment>(StableId(id.0))
        .unwrap()
        .revisions[0]
        .number = 1;
    assert!(Ledger::new(&wrong).validate(&refs, SimTime(4)).is_err());
    let mut wrong = w.clone();
    wrong
        .get_mut::<PersonalKnowledge>(StableId(1))
        .unwrap()
        .commitments
        .get_mut(&id)
        .unwrap()
        .revisions
        .get_mut(&0)
        .unwrap()
        .proposal
        .source = KnowledgeSource::Reported(PersonId(999));
    assert!(Ledger::new(&wrong).validate(&refs, SimTime(4)).is_err());
    let mut wrong = w.clone();
    wrong
        .get_mut::<Commitment>(StableId(id.0))
        .unwrap()
        .revisions[0]
        .backed
        .get_mut(&0)
        .unwrap()
        .progress = Progress::Fulfilled;
    assert!(Ledger::new(&wrong).validate(&refs, SimTime(4)).is_err());
    let mut missing = refs;
    missing.obligations.clear();
    assert!(Ledger::new(&w).validate(&missing, SimTime(4)).is_err());
}
#[test]
fn a_single_delivery_obligation_cannot_be_counted_for_multiple_regional_promises() {
    let (mut w, _, id) = setup();
    ratify(&mut w, id);
    let obligation = back_delivery(&mut w, id, 0, 3);
    let Term::Delivery { term, .. } =
        commitment(&w, id).unwrap().current().proposal.terms[1].clone()
    else {
        panic!()
    };
    assert!(
        Ledger::new(&w)
            .prepare_backing(
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
        Ledger::new(&w)
            .prepare_decision(
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
    let mut proposal = commitment(&base, id).unwrap().current().proposal.clone();
    proposal.terms = vec![Term::Passage {
        grantor: InstitutionId(12),
        beneficiary: InstitutionId(11),
        areas: BTreeSet::from([AreaId(2)]),
        until: SimTime(800),
    }];
    let mut w = store();
    Ledger::new(&w)
        .prepare_propose(id, proposal, &facts, SimTime(0))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    ratify(&mut w, id);
    Ledger::new(&w)
        .prepare_backing(
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
        Ledger::new(&w)
            .prepare_outcome(
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
    assert_eq!(
        commitment(&w, id).unwrap().current().status(),
        Status::Fulfilled
    );
    assert_eq!(
        commitment(&w, id).unwrap().current().backed[&0].defaulted_at,
        None
    );
}

#[test]
fn independent_native_results_have_stable_order_and_reject_late_or_foreign_decisions() {
    let (mut first, _, id) = setup();
    for institution in [InstitutionId(11), InstitutionId(12)] {
        Ledger::new(&first)
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
            Ledger::new(world)
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
    Ledger::new(&pending)
        .prepare_bind_ratification(id, 0, InstitutionId(11), RuleProposalId(111), SimTime(0))
        .unwrap()
        .commit(&mut pending)
        .unwrap();
    let before = pending.clone();
    for (institution, time) in [(InstitutionId(11), 101), (InstitutionId(99), 1)] {
        assert!(
            Ledger::new(&pending)
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
        commitment(&w, id)
            .unwrap()
            .current()
            .proposal
            .validate(&facts, SimTime(0)),
        Ok(())
    );
    let mut proposal = commitment(&w, id).unwrap().current().proposal.clone();
    proposal.ratify_by = SimTime(200);
    Ledger::new(&w)
        .prepare_revise(id, proposal, &facts, SimTime(3))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    assert_eq!(
        commitment(&w, id).unwrap().current().status(),
        Status::AwaitingRatification
    );
    let requested = Ledger::new(&w)
        .prepare_work_request(key(id, 0), PersonId(1), &facts, SimTime(4))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    assert!(matches!(requested[0], Request::Delivery { .. }));
    assert!(
        commitment(&w, id).unwrap().revisions[0]
            .requested
            .contains(&0)
    );
    assert!(commitment(&w, id).unwrap().current().requested.is_empty());
}

#[test]
fn real_pending_owner_proposals_are_reported_without_creating_obligations_or_acceptance() {
    let (mut w, f, id) = setup();
    ratify(&mut w, id);
    learn_approvals(&mut w, id, PersonId(1));
    let Term::Delivery { term, .. } =
        commitment(&w, id).unwrap().current().proposal.terms[0].clone()
    else {
        panic!()
    };
    Ledger::new(&w)
        .prepare_backing(
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
    assert_eq!(
        commitment(&w, id).unwrap().current().status(),
        Status::Ratified
    );
    assert_eq!(
        commitment(&w, id).unwrap().current().backed[&0].backing,
        Backing::Delivery {
            agreement: AgreementId(301),
            obligation: None
        }
    );
    Ledger::new(&w)
        .prepare_observe_work(witness(PersonId(1), 1, 3), key(id, 0), SimTime(3))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    let frozen = Ledger::new(&w)
        .prepare_report(PersonId(1), id, SimTime(3))
        .unwrap();
    Ledger::new(&w)
        .prepare_backing(
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
    Ledger::new(&w)
        .prepare_receive_report(PersonId(2), &frozen, SimTime(5))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    assert_eq!(
        Ledger::new(&w).view(PersonId(2), id).unwrap().revisions[&0].backed[&0]
            .value
            .progress,
        Progress::AwaitingWork
    );
    assert_eq!(
        Ledger::new(&w).validate(&references(&w, &f), SimTime(5)),
        Ok(())
    );
}
#[test]
fn owner_cache_projection_keeps_old_dates_and_fresh_local_observations_remain_valid() {
    let (mut w, f, id) = setup();
    ratify(&mut w, id);
    let obligation = back_delivery(&mut w, id, 0, 3);
    learn_approvals(&mut w, id, PersonId(1));
    Ledger::new(&w)
        .prepare_outcome(
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
    Ledger::new(&w)
        .prepare_learn_work(
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
        Ledger::new(&w).view(PersonId(1), id).unwrap().revisions[&0].backed[&0]
            .value
            .performed,
        Quantity(0)
    );
    assert_eq!(
        Ledger::new(&w).view(PersonId(1), id).unwrap().revisions[&0].backed[&0].observed_at,
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
    Ledger::new(&w)
        .prepare_learn_work(
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
    let report = Ledger::new(&w)
        .prepare_report(PersonId(1), id, SimTime(7))
        .unwrap();
    Ledger::new(&w)
        .prepare_receive_report(PersonId(2), &report, SimTime(8))
        .unwrap()
        .commit(&mut w)
        .unwrap();
    assert_eq!(
        Ledger::new(&w).validate(&references(&w, &f), SimTime(8)),
        Ok(())
    );
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct NativeActivity(u8);
#[derive(Clone, Default)]
struct NativePresence {
    busy: BTreeSet<PersonId>,
    remote: BTreeSet<PersonId>,
    unavailable: BTreeSet<PersonId>,
}
impl Presence for NativePresence {
    type Activity = NativeActivity;
    fn actor_reads(&self, actor: PersonId) -> game_ecs::ReadFootprint {
        let mut reads = game_ecs::ReadFootprint::new();
        reads.protect::<NativeActivity>(game_ecs::StableId(actor.0));
        reads
    }
    fn inventory_reads(&self, id: InventoryId) -> game_ecs::ReadFootprint {
        let mut reads = game_ecs::ReadFootprint::new();
        reads.protect::<game_cargo::Inventory>(game_ecs::StableId(id.0));
        reads
    }

    fn actor(&self, actor: PersonId) -> Option<ActorFacts> {
        Some(ActorFacts {
            alive: !self.unavailable.contains(&actor),
            health: 1,
            independent: true,
            in_combat: false,
            location: Location::AtArea(AreaId(if self.remote.contains(&actor) { 2 } else { 1 })),
            idle_or_rest: !self.busy.contains(&actor),
            foreign_talking: false,
            controlled: false,
            past_life_controlled: false,
        })
    }
    fn people(&self) -> impl Iterator<Item = PersonId> {
        [PersonId(1), PersonId(2), PersonId(3)].into_iter()
    }
    fn local_people(&self, area: AreaId) -> impl Iterator<Item = PersonId> {
        [PersonId(1), PersonId(2), PersonId(3)]
            .into_iter()
            .filter(move |p| actor_area(self, *p) == Some(area))
    }
    fn same_place(&self, a: PersonId, b: PersonId) -> bool {
        actor_area(self, a) == actor_area(self, b)
    }
    fn inventory_area(&self, _: InventoryId) -> Option<AreaId> {
        Some(AreaId(1))
    }
}
impl game_agreements::Presence for NativePresence {
    fn contact(&self, actor: PersonId) -> Option<game_agreements::ContactFacts> {
        Some(game_agreements::ContactFacts {
            alive: !self.unavailable.contains(&actor),
            health: 1,
            independent: true,
            in_combat: false,
            area: actor_area(self, actor),
        })
    }
}
fn composed_store() -> (Store, ProposalFacts, RealmCommitmentId) {
    let (mut base, facts, id) = setup();
    ratify(&mut base, id);
    learn_approvals(&mut base, id, PersonId(1));
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    game_agreements::register_components(&mut schema).unwrap();
    game_cargo::register_components(&mut schema).unwrap();
    game_characters::decisions::register_minds::<u8>(&mut schema).unwrap();
    game_society::register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    for (_, row) in commitments(&base) {
        insert_commitment(&mut store, row.clone()).unwrap();
    }
    for (actor, row) in knowledges(&base) {
        insert_knowledge(&mut store, actor, row.clone()).unwrap();
    }
    for actor in [PersonId(1), PersonId(2), PersonId(3)] {
        game_characters::decisions::insert_stocks(
            &mut store,
            actor,
            game_characters::decisions::Stocks::default(),
        )
        .unwrap();
    }
    for (inventory, quantity) in [(InventoryId(1), 1_000), (InventoryId(2), 0)] {
        game_cargo::insert_inventory(
            &mut store,
            game_cargo::Inventory {
                id: inventory,
                good: Good::Food,
                quantity: Quantity(quantity),
                capacity: Quantity(5_000),
                location: game_cargo::InventoryLocation::Area(AreaId(1)),
                access: BTreeSet::from([PersonId(1), PersonId(2)]),
                last_granted: None,
                spoils_at: None,
            },
        )
        .unwrap();
    }
    (store, facts, id)
}
fn native_terms(now: SimTime) -> game_agreements::TransitionFacts {
    game_agreements::TransitionFacts {
        actor_available: true,
        present: BTreeSet::from([PersonId(1), PersonId(2)]),
        assets: game_agreements::TermsContext {
            now,
            inventories: BTreeMap::from([
                (
                    InventoryId(1),
                    game_agreements::InventoryTerms {
                        good: Good::Food,
                        capacity: Quantity(5_000),
                    },
                ),
                (
                    InventoryId(2),
                    game_agreements::InventoryTerms {
                        good: Good::Food,
                        capacity: Quantity(5_000),
                    },
                ),
            ]),
            ..Default::default()
        },
    }
}
#[test]
fn regional_work_and_foreign_promise_publish_atomically_then_reconcile_without_refreshing_ignorance()
 {
    let (mut store, facts, id) = composed_store();
    let key = key(id, 0);
    let actor = PersonId(1);
    let agreement = AgreementId(500);
    let regional = Ledger::new(&store)
        .prepare_request(actor, key, &facts, SimTime(3))
        .unwrap();
    assert!(regional.existing.is_none());
    let Request::Delivery { parties, term, .. } = regional.request.clone() else {
        panic!()
    };
    let foreign = game_agreements::Ledger::new(&store)
        .prepare_transition(
            actor,
            &game_agreements::AgreementAction::Propose {
                parties,
                terms: vec![term],
                procedure: None,
            },
            &native_terms(SimTime(3)),
            &game_agreements::TransitionIds {
                agreement: Some(agreement),
                ..Default::default()
            },
        )
        .unwrap();
    let regional = regional.bind(PendingOwner::Agreement(agreement)).unwrap();
    let before = store.snapshot().unwrap();
    let mut failed = game_ecs::Batch::new();
    foreign.stage(&mut failed);
    let guard = regional.clone().stage_into(&store, &mut failed).unwrap();
    guard.validate(&failed).unwrap();
    let mut invalid = game_cargo::inventory(&store, InventoryId(1))
        .unwrap()
        .clone();
    invalid.quantity = Quantity(6_000);
    failed.insert(StableId(1), invalid);
    assert!(store.apply_batch(failed).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
    let mut batch = game_ecs::Batch::new();
    foreign.stage(&mut batch);
    let guard = regional.stage_into(&store, &mut batch).unwrap();
    guard.validate(&batch).unwrap();
    store.apply_batch(batch).unwrap();
    assert_eq!(
        commitment(&store, id).unwrap().revisions[0].pending[&0].owner,
        PendingOwner::Agreement(agreement)
    );
    game_agreements::Ledger::new(&store)
        .prepare_transition(
            PersonId(2),
            &game_agreements::AgreementAction::Accept { agreement },
            &native_terms(SimTime(4)),
            &game_agreements::TransitionIds {
                obligations: vec![ObligationId(501)],
                ..Default::default()
            },
        )
        .unwrap()
        .commit(&mut store)
        .unwrap();
    let presence = NativePresence::default();
    Ledger::new(&store)
        .prepare_reconciliation(id, SimTime(4), &presence)
        .unwrap()
        .commit(&mut store)
        .unwrap();
    assert_eq!(
        commitment(&store, id).unwrap().current().backed[&0].progress,
        Progress::Active
    );
    assert!(
        Ledger::new(&store).view(actor, id).unwrap().revisions[&0]
            .backed
            .is_empty()
    );
    game_agreements::local_notice(&mut store, SimTime(5), agreement, actor, None, &presence)
        .unwrap();
    Ledger::new(&store)
        .prepare_reconciliation(id, SimTime(5), &presence)
        .unwrap()
        .commit(&mut store)
        .unwrap();
    let known = &Ledger::new(&store).view(actor, id).unwrap().revisions[&0].backed[&0];
    assert_eq!(known.observed_at, SimTime(5));
    assert_eq!(known.source, KnowledgeSource::Observed(actor));
    assert_eq!(known.value.progress, Progress::Active);
}
#[test]
fn report_start_rejects_stale_stock_rules_presence_and_wrong_batch_replacement() {
    let (mut store, _, id) = composed_store();
    let runtime = Runtime::default();
    let now = SimTime(3);
    let presence = NativePresence::default();
    let plan = runtime
        .prepare_report_start(&store, now, PersonId(1), PersonId(2), id, &presence)
        .unwrap();
    game_characters::decisions::stocks_mut(&mut store, PersonId(1))
        .unwrap()
        .known
        .insert(
            InventoryId(1),
            StockKnowledge {
                inventory: InventoryId(1),
                area: AreaId(1),
                good: Good::Food,
                quantity: Quantity(1_000),
                accessible: true,
                observed_at: now,
                learned_at: now,
                source: KnowledgeSource::Observed(PersonId(1)),
            },
        );
    assert!(plan.check(&runtime, &store, now, &presence).is_err());
    game_characters::decisions::stocks_mut(&mut store, PersonId(1))
        .unwrap()
        .known
        .clear();
    let mut changed = runtime.clone();
    changed.rules.talk_time = SimDuration(1);
    assert!(plan.check(&changed, &store, now, &presence).is_err());
    let mut remote = presence.clone();
    remote.remote.insert(PersonId(2));
    assert!(plan.check(&runtime, &store, now, &remote).is_err());
    let mut first = game_ecs::Batch::new();
    let replacement = plan
        .stage_into_with_replacement(&runtime, &store, now, &presence, &mut first)
        .unwrap();
    let cancellations = runtime.prepare_cancel_talks(&store, &[PersonId(1)]);
    let mut other = game_ecs::Batch::new();
    assert!(
        cancellations
            .clone()
            .stage_into(&store, &mut other, Some(&replacement))
            .is_err()
    );
    let guard = cancellations
        .stage_into(&store, &mut first, Some(&replacement))
        .unwrap();
    guard.validate(&first).unwrap();
    store.apply_batch(first).unwrap();
    let before = store.snapshot().unwrap();
    let completion = runtime.prepare_report_completion(&store, now, PersonId(1), &presence);
    assert!(completion.is_err());
    assert_eq!(store.snapshot().unwrap(), before);
}
#[test]
fn native_realm_rows_restore_shape_and_detect_personal_or_allocated_orphans() {
    let (store, facts, id) = composed_store();
    let refs = references(&store, &facts);
    let mut malformed = store.snapshot().unwrap();
    let column = malformed.columns.get_mut("realm/commitment").unwrap();
    column[0].value =
        serde_json::value::RawValue::from_string("{\"id\":100,\"revisions\":[]}".into()).unwrap();
    let schema = {
        let mut schema = Schema::new();
        register_components(&mut schema).unwrap();
        game_agreements::register_components(&mut schema).unwrap();
        game_cargo::register_components(&mut schema).unwrap();
        game_characters::decisions::register_minds::<u8>(&mut schema).unwrap();
        game_society::register_components(&mut schema).unwrap();
        schema
    };
    assert!(Store::restore(schema, malformed).is_err());
    let mut wrong = store.clone();
    let row = commitment(&wrong, id).unwrap().clone();
    wrong.remove::<Commitment>(StableId(id.0)).unwrap();
    wrong.insert_or_spawn(StableId(999), row).unwrap();
    assert!(Ledger::new(&wrong).validate(&refs, SimTime(5)).is_err());
    let mut wrong = store.clone();
    insert_knowledge(&mut wrong, PersonId(999), PersonalKnowledge::default()).unwrap();
    assert!(Ledger::new(&wrong).validate(&refs, SimTime(5)).is_err());
}

#[test]
fn society_projected_publication_and_realm_receiving_reaction_share_one_guarded_batch() {
    use game_society::institutions::*;
    let (base, facts, id) = setup();
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    game_society::register_components(&mut schema).unwrap();
    game_cargo::register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    for (_, row) in commitments(&base) {
        insert_commitment(&mut store, row.clone()).unwrap();
    }
    for (actor, row) in knowledges(&base) {
        insert_knowledge(&mut store, actor, row.clone()).unwrap();
    }
    let office = OfficeMandate {
        slot: 0,
        holder: PersonId(1),
        inventories: BTreeSet::from([InventoryId(1)]),
        sites: BTreeSet::new(),
        powers: BTreeSet::from([OfficePower::Negotiate, OfficePower::Delivery]),
        minimum_support: 2,
    };
    let institution = Institution {
        id: InstitutionId(11),
        origin: None,
        charter: InstitutionCharter {
            purpose: "native council".into(),
            jurisdiction: AreaId(1),
            founders: BTreeSet::from([PersonId(1), PersonId(2)]),
            open_membership: true,
            procedure: DecisionProcedure {
                proposers: Subjects::members(),
                voters: Subjects::members(),
                minimum_support: 2,
            },
            offices: vec![office],
            rules: vec![],
            contributions: vec![],
            evidence: BTreeSet::new(),
        },
        members: BTreeSet::from([PersonId(1), PersonId(2)]),
        declined: BTreeSet::new(),
        invited: BTreeSet::new(),
        status: InstitutionStatus::Active,
        concepts: vec![],
        office_support: BTreeMap::from([(0, BTreeSet::from([PersonId(1), PersonId(2)]))]),
        grants: BTreeMap::new(),
        applied: BTreeMap::new(),
        funding: vec![],
        revision: 0,
    };
    game_society::insert_institution(&mut store, institution.id, institution).unwrap();
    let now = SimTime(3);
    let authority = AuthorityFacts {
        alive: facts.known_people.clone(),
    };
    let references = ReferenceFacts {
        people: facts.known_people.clone(),
        eligible_locations: BTreeMap::from([
            (PersonId(1), AreaId(1)),
            (PersonId(2), AreaId(2)),
            (PersonId(3), AreaId(1)),
        ]),
        ..Default::default()
    };
    let change = InstitutionChange::RatifyRegional {
        commitment: id,
        revision: 0,
        support: true,
    };
    let permit = ExternalChangePermit {
        actor: PersonId(1),
        institution: InstitutionId(11),
        institution_revision: 0,
        change: change.clone(),
    };
    let world = game_society::institutions::Runtime::default();
    let command = world
        .prepare_command(
            &store,
            &InstitutionAction::Propose {
                institution: InstitutionId(11),
                change,
            },
            &CommandFacts {
                actor: PersonId(1),
                area: AreaId(1),
                now,
                references: &references,
                authority: &authority,
                external_permit: Some(&permit),
            },
            CommandIds { first: 500 },
        )
        .unwrap();
    let observers = [
        game_society::perception::ObserverFacts {
            actor: PersonId(1),
            area: Some(AreaId(1)),
            alive: true,
            independent: true,
            conversation_allowed: true,
            empathy: 500,
        },
        game_society::perception::ObserverFacts {
            actor: PersonId(3),
            area: Some(AreaId(1)),
            alive: true,
            independent: true,
            conversation_allowed: true,
            empathy: 500,
        },
    ];
    let index = game_society::perception::ObservationIndex::new(&observers).unwrap();
    let publications = command
        .prepare_publications(&world, &store, &authority, now, &index)
        .unwrap();
    let runtime = crate::Runtime::default();
    let presence = NativePresence::default();
    let reaction = runtime
        .prepare_institution_reaction(&store, now, &publications, vec![], true, &presence)
        .unwrap();
    let before = store.snapshot().unwrap();
    let mut batch = game_ecs::Batch::new();
    let source = command
        .stage_with_publications(&world, &store, publications, &mut batch)
        .unwrap();
    let guard = reaction.stage_into(&store, &source, &mut batch).unwrap();
    source.validate(&batch).unwrap();
    guard.validate(&batch).unwrap();
    assert_eq!(store.snapshot().unwrap(), before);
    store.apply_batch(batch).unwrap();
    assert_eq!(
        commitment(&store, id)
            .unwrap()
            .current()
            .ratification_proposals[&InstitutionId(11)],
        RuleProposalId(500)
    );
    assert!(Ledger::new(&store).view(PersonId(3), id).is_some());
    assert!(Ledger::new(&store).view(PersonId(2), id).is_none());
}

fn pending_completion_store() -> (Store, Runtime, NativePresence, SimTime) {
    let (old, _, id) = composed_store();
    let mut schema = old.schema().clone();
    schema
        .register::<NativeActivity>("test/activity", |_| Ok(()))
        .unwrap();
    let mut snapshot = old.snapshot().unwrap();
    snapshot.columns.insert("test/activity".into(), vec![]);
    let mut store = Store::restore(schema, snapshot).unwrap();
    for actor in [1, 2, 3] {
        store
            .insert_or_spawn(
                StableId(actor),
                NativeActivity(if actor == 1 { 9 } else { 0 }),
            )
            .unwrap();
    }
    let runtime = Runtime::default();
    let presence = NativePresence::default();
    let now = SimTime(3);
    let plan = runtime
        .prepare_report_start(&store, now, PersonId(1), PersonId(2), id, &presence)
        .unwrap();
    let completes = plan.pending().completes;
    let mut batch = game_ecs::Batch::new();
    plan.stage_into_with_replacement(&runtime, &store, now, &presence, &mut batch)
        .unwrap();
    store.apply_batch(batch).unwrap();
    (store, runtime, presence, completes)
}
#[test]
fn completion_rechecks_time_contact_death_busy_and_selected_activity_before_any_publication() {
    let (store, runtime, presence, now) = pending_completion_store();
    let plan = runtime
        .prepare_report_completion(&store, now, PersonId(1), &presence)
        .unwrap();
    assert!(matches!(plan.event, RealmEvent::Reported { .. }));
    let before = store.snapshot().unwrap();
    for failure in 0..4 {
        let mut changed = presence.clone();
        let at = if failure == 0 {
            SimTime(now.0 + 1)
        } else {
            now
        };
        match failure {
            1 => {
                changed.remote.insert(PersonId(2));
            }
            2 => {
                changed.unavailable.insert(PersonId(2));
            }
            3 => {
                changed.busy.insert(PersonId(2));
            }
            _ => {}
        }
        assert!(
            plan.clone()
                .stage_into(
                    &store,
                    at,
                    &changed,
                    NativeActivity(0),
                    &mut game_ecs::Batch::new()
                )
                .is_err()
        );
        assert_eq!(store.snapshot().unwrap(), before);
    }
    let mut changed = store.clone();
    changed
        .insert_or_spawn(StableId(2), NativeActivity(1))
        .unwrap();
    let before_changed = changed.snapshot().unwrap();
    assert!(
        plan.stage_into(
            &changed,
            now,
            &presence,
            NativeActivity(0),
            &mut game_ecs::Batch::new()
        )
        .is_err()
    );
    assert_eq!(changed.snapshot().unwrap(), before_changed);
}
#[test]
fn completion_guards_late_contact_edits_and_owns_clear_while_unrelated_rows_compose() {
    let (mut store, runtime, presence, now) = pending_completion_store();
    for actor in [1, 2] {
        let plan = runtime
            .prepare_report_completion(&store, now, PersonId(1), &presence)
            .unwrap();
        let mut batch = game_ecs::Batch::new();
        let (guard, _) = plan
            .stage_into(&store, now, &presence, NativeActivity(0), &mut batch)
            .unwrap();
        batch.insert(StableId(actor), NativeActivity(1));
        assert!(guard.validate(&batch).is_err());
        assert!(talk(&store, PersonId(1)).is_some());
    }
    let plan = runtime
        .prepare_report_completion(&store, now, PersonId(1), &presence)
        .unwrap();
    let mut batch = game_ecs::Batch::new();
    let (guard, knowledge) = plan
        .stage_into(&store, now, &presence, NativeActivity(0), &mut batch)
        .unwrap();
    batch.insert(StableId(3), NativeActivity(1));
    assert!(guard.validate(&game_ecs::Batch::new()).is_err());
    guard.validate(&batch).unwrap();
    knowledge.unwrap().validate(&batch).unwrap();
    store.apply_batch(batch).unwrap();
    assert!(talk(&store, PersonId(1)).is_none());
    assert_eq!(
        store.get::<NativeActivity>(StableId(1)),
        Some(&NativeActivity(0))
    );
}
fn admission_store() -> (
    Store,
    Runtime,
    NativePresence,
    game_society::institutions::AuthorityFacts,
    RealmCommitmentId,
) {
    use game_characters::decisions::*;
    use game_society::institutions::*;
    let (mut store, facts, id) = composed_store();
    let now = SimTime(3);
    let authority = AuthorityFacts {
        alive: facts.known_people.clone(),
    };
    let mut mind = Mind::<u8> {
        interests: BTreeMap::new(),
        routes: BTreeMap::from([(AreaId(1), BTreeMap::from([(AreaId(2), SimDuration(1))]))]),
        last_failure: None,
        child_signals: BTreeMap::new(),
        personality: Personality::industrious(),
        dream: Aspiration::Comfort,
        sites: BTreeMap::new(),
        relationships: BTreeMap::new(),
        values: BTreeMap::new(),
        goal: Goal::Rest,
        plan: None,
        explanation: Explanation {
            goal: Goal::Rest,
            need: 0,
            benefit: 0,
            value: 0,
            total: 0,
            blocker: None,
        },
        retry_after: SimTime(0),
    };
    for actor in [PersonId(1), PersonId(3)] {
        insert_mind(&mut store, actor, mind.clone()).unwrap();
    }
    mind.routes.clear();
    for (inventory, good) in [
        (1, Good::Food),
        (2, Good::Food),
        (3, Good::Timber),
        (4, Good::Timber),
    ] {
        stocks_mut(&mut store, PersonId(1)).unwrap().known.insert(
            InventoryId(inventory),
            StockKnowledge {
                inventory: InventoryId(inventory),
                area: AreaId(1),
                good,
                quantity: Quantity(1000),
                accessible: true,
                observed_at: now,
                learned_at: now,
                source: KnowledgeSource::Observed(PersonId(1)),
            },
        );
    }
    let world = game_society::institutions::Runtime::default();
    for (institution, person, area, inventory) in [(11, 1, 1, 1), (12, 2, 2, 3)] {
        let row = Institution {
            id: InstitutionId(institution),
            origin: None,
            charter: InstitutionCharter {
                purpose: format!("council{institution}"),
                jurisdiction: AreaId(area),
                founders: BTreeSet::from([PersonId(person), PersonId(3)]),
                open_membership: true,
                procedure: DecisionProcedure {
                    proposers: Subjects::members(),
                    voters: Subjects::members(),
                    minimum_support: 1,
                },
                offices: vec![OfficeMandate {
                    slot: 0,
                    holder: PersonId(person),
                    inventories: BTreeSet::from([InventoryId(inventory)]),
                    sites: BTreeSet::new(),
                    powers: BTreeSet::from([OfficePower::Negotiate, OfficePower::Delivery]),
                    minimum_support: 1,
                }],
                rules: vec![],
                contributions: vec![],
                evidence: BTreeSet::new(),
            },
            members: BTreeSet::from([PersonId(person), PersonId(3)]),
            declined: BTreeSet::new(),
            invited: BTreeSet::new(),
            status: InstitutionStatus::Active,
            concepts: vec![],
            office_support: BTreeMap::from([(0, BTreeSet::from([PersonId(person)]))]),
            grants: BTreeMap::new(),
            applied: BTreeMap::new(),
            funding: vec![],
            revision: 0,
        };
        let view = world.actual_institution_view(&store, &row, &authority, now);
        game_society::insert_institution(&mut store, row.id, row).unwrap();
        let mut known = game_society::knowledge(&store, PersonId(1))
            .cloned()
            .unwrap_or_default();
        known.institutions.insert(view.id, view);
        game_society::insert_knowledge(&mut store, PersonId(1), known).unwrap();
    }
    (
        store,
        crate::Runtime::default(),
        NativePresence::default(),
        authority,
        id,
    )
}
#[test]
fn proposal_and_bound_work_keep_foreign_read_dependencies_through_final_batch_validation() {
    let (store, runtime, presence, authority, id) = admission_store();
    let proposal = commitment(&store, id).unwrap().current().proposal.clone();
    let context = RealmContext {
        now: SimTime(3),
        authority: &authority,
        presence: &presence,
    };
    let before = store.snapshot().unwrap();
    for failure in 0..4 {
        for proposal_path in [true, false] {
            let mut batch = game_ecs::Batch::new();
            let guard = if proposal_path {
                runtime
                    .prepare_proposal::<u8>(
                        &store,
                        PersonId(1),
                        RealmCommitmentId(200),
                        &proposal,
                        &context,
                    )
                    .unwrap()
                    .stage_into(&runtime, &store, &context, &mut batch)
                    .unwrap()
            } else {
                runtime
                    .prepare_work::<u8>(
                        &store,
                        PersonId(1),
                        key(id, 0),
                        context.now,
                        &authority,
                        &presence,
                    )
                    .unwrap()
                    .bind(PendingOwner::Agreement(AgreementId(500)))
                    .unwrap()
                    .stage_into(&runtime, &store, &context, &mut batch)
                    .unwrap()
            };
            match failure {
                0 => {
                    batch.remove::<game_society::institutions::Institution>(StableId(11));
                }
                1 => {
                    batch.insert(StableId(1), game_characters::decisions::Stocks::default());
                }
                2 => {
                    batch.remove::<game_characters::decisions::Mind<u8>>(StableId(1));
                }
                _ => {
                    batch.remove::<game_cargo::Inventory>(StableId(1));
                }
            }
            assert!(guard.validate(&batch).is_err());
            assert_eq!(store.snapshot().unwrap(), before);
        }
    }
    let mut batch = game_ecs::Batch::new();
    let guard = runtime
        .prepare_proposal::<u8>(
            &store,
            PersonId(1),
            RealmCommitmentId(200),
            &proposal,
            &context,
        )
        .unwrap()
        .stage_into(&runtime, &store, &context, &mut batch)
        .unwrap();
    batch.insert(StableId(3), game_characters::decisions::Stocks::default());
    guard.validate(&batch).unwrap();
    assert!(guard.validate(&game_ecs::Batch::new()).is_err());
}

#[test]
fn bound_delivery_accepts_only_its_exact_owner_creation_receipt_and_rejects_late_edits() {
    let (store, runtime, presence, authority, id) = admission_store();
    let context = RealmContext {
        now: SimTime(3),
        authority: &authority,
        presence: &presence,
    };
    let prepared = runtime
        .prepare_work::<u8>(
            &store,
            PersonId(1),
            key(id, 0),
            context.now,
            &authority,
            &presence,
        )
        .unwrap();
    let Request::Delivery { parties, term, .. } = prepared.request().clone() else {
        panic!()
    };
    let foreign = game_agreements::Ledger::new(&store)
        .prepare_transition(
            PersonId(1),
            &game_agreements::AgreementAction::Propose {
                parties,
                terms: vec![term],
                procedure: None,
            },
            &native_terms(context.now),
            &game_agreements::TransitionIds {
                agreement: Some(AgreementId(500)),
                ..Default::default()
            },
        )
        .unwrap();
    let foreign_runtime = game_agreements::Runtime::default();
    let receipt = game_agreements::PreparedProposalPublication::prepare(
        foreign,
        &store,
        &foreign_runtime,
        context.now,
        1,
        &[],
        &presence,
    )
    .unwrap();
    let bound = prepared
        .bind(PendingOwner::Agreement(AgreementId(500)))
        .unwrap();
    let mut batch = game_ecs::Batch::new();
    let source = receipt
        .stage_into(&store, &foreign_runtime, &mut batch)
        .unwrap();
    assert!(
        bound
            .clone()
            .stage_into(&runtime, &store, &context, &mut batch)
            .is_err()
    );
    // The rejected draft batch is discarded; an independently staged source
    // receipt cannot be reused across batches or substitute a different term.
    let Request::Delivery { parties, term, .. } = runtime
        .prepare_work::<u8>(
            &store,
            PersonId(1),
            key(id, 0),
            context.now,
            &authority,
            &presence,
        )
        .unwrap()
        .request()
        .clone()
    else {
        panic!()
    };
    assert!(
        source
            .authorizes_creation(
                &game_ecs::Batch::new(),
                AgreementId(500),
                PersonId(1),
                &parties,
                &term
            )
            .is_err()
    );
    let foreign = game_agreements::Ledger::new(&store)
        .prepare_transition(
            PersonId(1),
            &game_agreements::AgreementAction::Propose {
                parties,
                terms: vec![term],
                procedure: None,
            },
            &native_terms(context.now),
            &game_agreements::TransitionIds {
                agreement: Some(AgreementId(500)),
                ..Default::default()
            },
        )
        .unwrap();
    let receipt = game_agreements::PreparedProposalPublication::prepare(
        foreign,
        &store,
        &foreign_runtime,
        context.now,
        1,
        &[],
        &presence,
    )
    .unwrap();
    let mut batch = game_ecs::Batch::new();
    let source = receipt
        .stage_into(&store, &foreign_runtime, &mut batch)
        .unwrap();
    let guard = bound
        .stage_with_agreement_publication(&runtime, &store, &context, &source, &mut batch)
        .unwrap();
    guard.validate(&batch).unwrap();
    batch.remove::<game_agreements::Knowledge>(StableId(1));
    assert!(guard.validate(&batch).is_err());
    assert!(commitment(&store, id).unwrap().current().pending.is_empty());
}

#[test]
fn bound_passage_uses_exact_projected_society_receipt_and_keeps_foreign_reads_guarded() {
    use game_society::institutions::*;
    let (mut store, runtime, presence, authority, id) = admission_store();
    let mut row = commitment(&store, id).unwrap().clone();
    row.revisions[0].proposal.terms[0] = Term::Passage {
        grantor: InstitutionId(11),
        beneficiary: InstitutionId(12),
        areas: BTreeSet::from([AreaId(1)]),
        until: SimTime(1000),
    };
    let proposal = row.revisions[0].proposal.clone();
    insert_commitment(&mut store, row).unwrap();
    let mut known = knowledge(&store, PersonId(1)).unwrap().clone();
    known
        .commitments
        .get_mut(&id)
        .unwrap()
        .revisions
        .get_mut(&0)
        .unwrap()
        .proposal
        .value = proposal;
    insert_knowledge(&mut store, PersonId(1), known).unwrap();
    let now = SimTime(3);
    let context = RealmContext {
        now,
        authority: &authority,
        presence: &presence,
    };
    let bound = runtime
        .prepare_work::<u8>(&store, PersonId(1), key(id, 0), now, &authority, &presence)
        .unwrap()
        .bind(PendingOwner::Passage(RuleProposalId(500)))
        .unwrap();
    let change = InstitutionChange::AuthorizeRegionalPassage {
        commitment: id,
        revision: 0,
        index: 0,
    };
    let permit = ExternalChangePermit {
        actor: PersonId(1),
        institution: InstitutionId(11),
        institution_revision: 0,
        change: change.clone(),
    };
    let refs = ReferenceFacts {
        people: BTreeSet::from([PersonId(1), PersonId(2), PersonId(3)]),
        eligible_locations: BTreeMap::from([(PersonId(1), AreaId(1)), (PersonId(3), AreaId(1))]),
        ..Default::default()
    };
    let foreign_runtime = game_society::institutions::Runtime::default();
    let foreign = foreign_runtime
        .prepare_command(
            &store,
            &InstitutionAction::Propose {
                institution: InstitutionId(11),
                change,
            },
            &CommandFacts {
                actor: PersonId(1),
                area: AreaId(1),
                now,
                references: &refs,
                authority: &authority,
                external_permit: Some(&permit),
            },
            CommandIds { first: 500 },
        )
        .unwrap();
    let observers = [game_society::perception::ObserverFacts {
        actor: PersonId(1),
        area: Some(AreaId(1)),
        alive: true,
        independent: true,
        conversation_allowed: true,
        empathy: 500,
    }];
    let index = game_society::perception::ObservationIndex::new(&observers).unwrap();
    let pubs = foreign
        .prepare_publications(&foreign_runtime, &store, &authority, now, &index)
        .unwrap();
    let reaction = bound
        .receive_publications(&runtime, &store, &context, &pubs)
        .unwrap();
    let mut batch = game_ecs::Batch::new();
    let before = store.snapshot().unwrap();
    let source = foreign
        .stage_with_publications(&foreign_runtime, &store, pubs, &mut batch)
        .unwrap();
    let guard = reaction
        .stage_into(&runtime, &store, &context, &source, &mut batch)
        .unwrap();
    guard.validate(&batch).unwrap();
    assert_eq!(store.snapshot().unwrap(), before);
    batch.remove::<Institution>(StableId(11));
    assert!(guard.validate(&batch).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
}
