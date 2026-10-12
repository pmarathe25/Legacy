use crate::fixture::Fixture;
use crate::*;
use game_agreements::{AgreementTerm, InventoryTerms, TermsContext};
use game_core::*;
use std::collections::{BTreeMap, BTreeSet};
pub(super) const SP: PersonId = PersonId(101);
pub(super) const A: PersonId = PersonId(102);
pub(super) const B: PersonId = PersonId(103);
pub(super) const ENEMY: PersonId = PersonId(104);
pub(super) const ID: MissionId = MissionId(11);
pub(super) const HOME: AreaId = AreaId(1);
pub(super) const AWAY: AreaId = AreaId(2);
pub(super) const OFFER: RecruitmentId = RecruitmentId(12);
pub(super) fn state() -> Fixture {
    Fixture::new()
}
pub(super) fn facts() -> CommandFacts {
    CommandFacts {
        now: SimTime(0),
        area: HOME,
        available: BTreeSet::from([SP, A, B]),
        duties: BTreeSet::new(),
        contacts: BTreeMap::from([(SP, HOME), (A, HOME), (B, HOME)]),
        assets: MissionAssets {
            areas: BTreeSet::from([HOME, AWAY]),
            people: BTreeSet::from([ENEMY]),
            terms: TermsContext {
                inventories: BTreeMap::from([
                    (
                        InventoryId(20),
                        InventoryTerms {
                            good: Good::Food,
                            capacity: Quantity(10_000),
                        },
                    ),
                    (
                        InventoryId(21),
                        InventoryTerms {
                            good: Good::Food,
                            capacity: Quantity(10_000),
                        },
                    ),
                ]),
                ..Default::default()
            },
            carrying_capacity: BTreeMap::from([(A, Quantity(5000)), (B, Quantity(5000))]),
        },
    }
}
pub(super) fn payment() -> AgreementTerm {
    AgreementTerm::Delivery {
        debtor: SP,
        creditor: A,
        source: InventoryId(20),
        destination: InventoryId(21),
        quantity: Quantity(1000),
        deadline: SimTime(1000),
    }
}
pub(super) fn proposal() -> MissionProposal {
    MissionProposal {
        leader: A,
        recruits: BTreeSet::from([B]),
        objective: MissionObjective::Visit { destination: AWAY },
        home: HOME,
        supplies: vec![MissionSupply {
            carrier: A,
            source: InventoryId(20),
            quantity: Quantity(2000),
        }],
        compensation: BTreeMap::new(),
        deadline: SimTime(2000),
    }
}
pub(super) fn start(w: &mut Fixture, p: MissionProposal, f: &CommandFacts) {
    w.prepare_command(
        &w.store,
        SP,
        &MissionAction::Propose { proposal: p },
        f,
        &MissionIds {
            mission: Some(ID),
            recruitments: BTreeMap::from([(A, OFFER), (B, RecruitmentId(13))]),
            backing: None,
        },
    )
    .unwrap()
    .commit(&mut w.runtime, &mut w.store)
    .unwrap();
}
pub(super) fn command(
    w: &mut Fixture,
    person: PersonId,
    action: MissionAction,
    f: &CommandFacts,
    ids: MissionIds,
) -> PreparedMission {
    let p = w
        .prepare_command(&w.store, person, &action, f, &ids)
        .unwrap();
    p.clone().commit(&mut w.runtime, &mut w.store).unwrap();
    p
}
pub(super) fn accept(w: &mut Fixture, person: PersonId, id: RecruitmentId, f: &CommandFacts) {
    command(
        w,
        person,
        MissionAction::Accept { recruitment: id },
        f,
        MissionIds::default(),
    );
}
pub(super) fn input(actor: PersonId) -> RecruitmentInput {
    RecruitmentInput {
        actor,
        guardian_duty: false,
        violent: false,
        risk_tolerance: 1000,
        ambition: 900,
        sociability: 900,
        empathy: 0,
        sponsor_trust: 0,
        social_disruption: 0,
        urgent_care: false,
        hungry: false,
    }
}

#[test]
fn proposal_is_not_recruitment_payment_or_provisioning_and_guarded_acceptance_is_required() {
    let mut w = state();
    let f = facts();
    let mut p = proposal();
    p.compensation.insert(A, vec![payment()]);
    start(&mut w, p, &f);
    assert_eq!(
        mission(&w.store, ID).unwrap().members[&A].status,
        CompanionStatus::Offered
    );
    assert!(mission(&w.store, ID).unwrap().members[&A].cargo.is_empty());
    assert_eq!(recruitment(&w.store, OFFER).unwrap().agreement, None);
    let before = w.clone();
    assert_eq!(
        w.prepare_command(
            &w.store,
            A,
            &MissionAction::Accept { recruitment: OFFER },
            &f,
            &MissionIds::default()
        )
        .unwrap_err(),
        Error::Unavailable
    );
    assert_eq!(w, before);
    let prepared = command(
        &mut w,
        A,
        MissionAction::Accept { recruitment: OFFER },
        &f,
        MissionIds {
            backing: Some(AgreementId(30)),
            ..Default::default()
        },
    );
    assert_eq!(
        prepared.consent,
        Some(ServiceConsent {
            recruitment: OFFER,
            sponsor: SP,
            recruit: A,
            terms: vec![payment()],
            agreement: AgreementId(30)
        })
    );
    assert_eq!(
        recruitment(&w.store, OFFER).unwrap().agreement,
        Some(AgreementId(30))
    );
    assert!(mission(&w.store, ID).unwrap().members[&A].cargo.is_empty());
}
#[test]
fn absent_sponsor_urgent_duty_and_wrong_recruit_cannot_accept() {
    let mut w = state();
    let mut f = facts();
    start(&mut w, proposal(), &f);
    let before = w.clone();
    f.available.remove(&SP);
    assert!(
        w.prepare_command(
            &w.store,
            A,
            &MissionAction::Accept { recruitment: OFFER },
            &f,
            &MissionIds::default()
        )
        .is_err()
    );
    f.available.insert(SP);
    f.duties.insert(A);
    assert!(
        w.prepare_command(
            &w.store,
            A,
            &MissionAction::Accept { recruitment: OFFER },
            &f,
            &MissionIds::default()
        )
        .is_err()
    );
    f.duties.clear();
    assert!(
        w.prepare_command(
            &w.store,
            B,
            &MissionAction::Accept { recruitment: OFFER },
            &f,
            &MissionIds::default()
        )
        .is_err()
    );
    assert_eq!(w, before);
    command(
        &mut w,
        A,
        MissionAction::Refuse { recruitment: OFFER },
        &f,
        MissionIds::default(),
    );
    assert_eq!(
        mission(&w.store, ID).unwrap().members[&A].status,
        CompanionStatus::Refused
    );
}
#[test]
fn counteroffer_needs_sponsor_assent_and_recruits_later_actual_acceptance() {
    let mut w = state();
    let f = facts();
    start(&mut w, proposal(), &f);
    command(
        &mut w,
        A,
        MissionAction::Counteroffer {
            recruitment: OFFER,
            compensation: vec![payment()],
        },
        &f,
        MissionIds::default(),
    );
    let ids = MissionIds {
        backing: Some(AgreementId(30)),
        ..Default::default()
    };
    assert!(
        w.prepare_command(
            &w.store,
            A,
            &MissionAction::Accept { recruitment: OFFER },
            &f,
            &ids
        )
        .is_err()
    );
    command(
        &mut w,
        SP,
        MissionAction::AcceptCounteroffer { recruitment: OFFER },
        &f,
        MissionIds::default(),
    );
    assert_eq!(
        mission(&w.store, ID).unwrap().members[&A].status,
        CompanionStatus::Offered
    );
    command(
        &mut w,
        A,
        MissionAction::Accept { recruitment: OFFER },
        &f,
        ids,
    );
    assert_eq!(
        mission(&w.store, ID).unwrap().members[&A].status,
        CompanionStatus::Accepted
    );
    command(
        &mut w,
        A,
        MissionAction::Leave { mission: ID },
        &f,
        MissionIds::default(),
    );
    assert_eq!(
        recruitment(&w.store, OFFER).unwrap().agreement,
        Some(AgreementId(30))
    ); // Existing debt is not forgiven by exit.
}
#[test]
fn stale_acceptance_and_existing_commitments_are_atomic() {
    let mut w = state();
    let f = facts();
    start(&mut w, proposal(), &f);
    let prepared = w
        .prepare_command(
            &w.store,
            A,
            &MissionAction::Accept { recruitment: OFFER },
            &f,
            &MissionIds::default(),
        )
        .unwrap();
    command(
        &mut w,
        A,
        MissionAction::Refuse { recruitment: OFFER },
        &f,
        MissionIds::default(),
    );
    let before = w.clone();
    assert_eq!(
        prepared.commit(&mut w.runtime, &mut w.store),
        Err(Error::Unavailable)
    );
    assert_eq!(w, before);
    let mut w = state();
    start(&mut w, proposal(), &f);
    accept(&mut w, A, OFFER, &f);
    assert!(
        w.prepare_command(
            &w.store,
            SP,
            &MissionAction::Propose {
                proposal: proposal()
            },
            &f,
            &MissionIds {
                mission: Some(MissionId(31)),
                recruitments: BTreeMap::from([(A, RecruitmentId(32)), (B, RecruitmentId(33))]),
                backing: None
            }
        )
        .is_err()
    );
}
#[test]
fn autonomous_offer_response_uses_personal_needs_and_own_known_food() {
    let mut w = state();
    let f = facts();
    start(&mut w, proposal(), &f);
    assert_eq!(
        w.recruitment_response(&w.store, OFFER, input(A), HOME, InventoryId(21), &[]),
        Some(MissionAction::Accept { recruitment: OFFER })
    );
    assert_eq!(
        w.recruitment_response(
            &w.store,
            OFFER,
            RecruitmentInput {
                guardian_duty: true,
                ..input(A)
            },
            HOME,
            InventoryId(21),
            &[]
        ),
        Some(MissionAction::Refuse { recruitment: OFFER })
    );
    let cautious = RecruitmentInput {
        ambition: 100,
        sociability: 100,
        ..input(A)
    };
    assert_eq!(
        w.recruitment_response(&w.store, OFFER, cautious, HOME, InventoryId(21), &[]),
        Some(MissionAction::Refuse { recruitment: OFFER })
    );
    let known = StockKnowledge {
        inventory: InventoryId(20),
        area: HOME,
        good: Good::Food,
        quantity: Quantity(1000),
        accessible: true,
        observed_at: SimTime(0),
        learned_at: SimTime(0),
        source: KnowledgeSource::Observed(A),
    };
    assert_eq!(
        w.recruitment_response(
            &w.store,
            OFFER,
            cautious,
            HOME,
            InventoryId(21),
            std::slice::from_ref(&known)
        ),
        Some(MissionAction::Counteroffer {
            recruitment: OFFER,
            compensation: vec![AgreementTerm::Delivery {
                debtor: SP,
                creditor: A,
                source: InventoryId(20),
                destination: InventoryId(21),
                quantity: Quantity(1000),
                deadline: SimTime(2000)
            }]
        })
    );
    assert_eq!(
        w.recruitment_response(
            &w.store,
            OFFER,
            cautious,
            HOME,
            InventoryId(21),
            &[StockKnowledge {
                accessible: false,
                ..known
            }]
        ),
        Some(MissionAction::Refuse { recruitment: OFFER })
    );
}
#[test]
fn local_order_keeps_distant_directives_and_every_person_can_refuse() {
    let mut w = state();
    let f = facts();
    start(&mut w, proposal(), &f);
    accept(&mut w, A, OFFER, &f);
    accept(&mut w, B, RecruitmentId(13), &f);
    let objective = MissionObjective::Defeat { person: ENEMY };
    let old_b = mission(&w.store, ID).unwrap().members[&B].clone();
    let order = OrderFacts {
        now: SimTime(10),
        heard: BTreeMap::from([(A, ServiceChoice::Personal(input(A)))]),
        assets: f.assets.clone(),
    };
    w.prepare_order(&w.store, ID, A, &objective, &order, false)
        .unwrap()
        .commit(&mut w.runtime, &mut w.store)
        .unwrap();
    assert_eq!(mission(&w.store, ID).unwrap().members[&B], old_b);
    let unwilling = RecruitmentInput {
        ambition: 0,
        sociability: 0,
        risk_tolerance: 0,
        empathy: 1000,
        ..input(B)
    };
    let order = OrderFacts {
        heard: BTreeMap::from([
            (A, ServiceChoice::Personal(input(A))),
            (B, ServiceChoice::Personal(unwilling)),
        ]),
        ..order
    };
    let prepared = w
        .prepare_order(&w.store, ID, A, &objective, &order, false)
        .unwrap();
    assert_eq!(
        prepared.events[0],
        MissionEvent::RecruitmentChanged {
            mission: ID,
            person: B,
            status: CompanionStatus::Withdrawn
        }
    );
    prepared.commit(&mut w.runtime, &mut w.store).unwrap();
    assert_eq!(
        mission(&w.store, ID).unwrap().members[&B].status,
        CompanionStatus::Withdrawn
    );
}
#[test]
fn departure_and_retreat_are_commitments_and_do_not_move_bodies() {
    let mut w = state();
    let f = facts();
    start(&mut w, proposal(), &f);
    assert!(
        w.prepare_command(
            &w.store,
            SP,
            &MissionAction::Depart { mission: ID },
            &f,
            &MissionIds::default()
        )
        .is_err()
    );
    accept(&mut w, A, OFFER, &f);
    command(
        &mut w,
        A,
        MissionAction::Depart { mission: ID },
        &f,
        MissionIds::default(),
    );
    assert_eq!(
        mission(&w.store, ID).unwrap().status,
        MissionStatus::Traveling
    );
    assert_eq!(
        mission(&w.store, ID).unwrap().members[&A].visited,
        BTreeSet::from([HOME])
    );
    let order = OrderFacts {
        now: SimTime(10),
        heard: BTreeMap::from([(A, ServiceChoice::Personal(input(A)))]),
        assets: f.assets,
    };
    w.prepare_order(
        &w.store,
        ID,
        A,
        &MissionObjective::Visit { destination: HOME },
        &order,
        true,
    )
    .unwrap()
    .commit(&mut w.runtime, &mut w.store)
    .unwrap();
    assert_eq!(
        mission(&w.store, ID).unwrap().status,
        MissionStatus::Returning
    );
    assert!(!mission(&w.store, ID).unwrap().succeeded);
    assert_eq!(
        mission(&w.store, ID).unwrap().members[&A].visited,
        BTreeSet::from([HOME])
    );
}
