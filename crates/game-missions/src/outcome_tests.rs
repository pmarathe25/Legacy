use crate::transition_tests::*;
use crate::*;
use game_agreements::{AgreementStatus, AgreementView, ObligationView};
use game_core::*;
use std::collections::{BTreeMap, BTreeSet};
fn expedition(objective: MissionObjective) -> World {
    let mut w = state();
    let f = facts();
    let mut p = proposal();
    p.objective = objective;
    start(&mut w, p, &f);
    accept(&mut w, A, OFFER, &f);
    command(
        &mut w,
        A,
        MissionAction::Depart { mission: ID },
        &f,
        MissionIds::default(),
    );
    w
}
fn bodies() -> BTreeMap<PersonId, MemberBody> {
    BTreeMap::from([
        (
            A,
            MemberBody {
                alive: true,
                captive: false,
                area: Some(HOME),
            },
        ),
        (
            B,
            MemberBody {
                alive: true,
                captive: false,
                area: Some(HOME),
            },
        ),
    ])
}
fn outcome(now: u64) -> OutcomeFacts {
    OutcomeFacts {
        now: SimTime(now),
        bodies: bodies(),
        leader_knowledge: ObjectiveKnowledge::default(),
        receivers: BTreeSet::from([SP, A]),
    }
}
fn target(now: u64) -> TargetObservation {
    TargetObservation {
        person: ENEMY,
        area: HOME,
        alive: true,
        health: 100,
        incapacitation: 10,
        captor: None,
        restricted: false,
        observed_at: SimTime(now),
    }
}
fn receipt(w: &mut World, id: u64, r: MissionReceipt) {
    w.prepare_receipt(EventId(id), &r)
        .unwrap()
        .commit(w)
        .unwrap();
}
#[test]
fn conserved_origin_cargo_is_credited_once_and_real_losses_remain_owed() {
    let objective = MissionObjective::Deliver {
        source: InventoryId(20),
        destination: InventoryId(21),
        quantity: Quantity(1000),
    };
    let mut w = expedition(objective);
    receipt(
        &mut w,
        1,
        MissionReceipt::Transferred {
            actor: A,
            source: InventoryId(50),
            destination: InventoryId(21),
            quantity: Quantity(1000),
            destination_carrier: None,
        },
    );
    assert_eq!(w.missions[&ID].members[&A].delivered, 0);
    let pickup = MissionReceipt::Transferred {
        actor: A,
        source: InventoryId(20),
        destination: InventoryId(40),
        quantity: Quantity(800),
        destination_carrier: Some(A),
    };
    receipt(&mut w, 2, pickup.clone());
    let once = w.clone();
    receipt(&mut w, 2, pickup);
    assert_eq!(w, once);
    receipt(
        &mut w,
        3,
        MissionReceipt::Lost {
            source: InventoryId(40),
            quantity: Quantity(300),
        },
    );
    receipt(
        &mut w,
        4,
        MissionReceipt::Transferred {
            actor: A,
            source: InventoryId(40),
            destination: InventoryId(21),
            quantity: Quantity(500),
            destination_carrier: None,
        },
    );
    assert_eq!(w.missions[&ID].members[&A].delivered, 500);
    assert!(w.missions[&ID].members[&A].cargo.is_empty());
    let plan = w.prepare_outcome(ID, &outcome(100)).unwrap();
    plan.commit(&mut w).unwrap();
    assert!(!w.missions[&ID].succeeded);
}
#[test]
fn actual_capture_release_and_death_change_service_without_forgiving_payment() {
    let mut w = state();
    let f = facts();
    let mut p = proposal();
    p.compensation.insert(A, vec![payment()]);
    start(&mut w, p, &f);
    command(
        &mut w,
        A,
        MissionAction::Accept { recruitment: OFFER },
        &f,
        MissionIds {
            backing: Some(AgreementId(30)),
            ..Default::default()
        },
    );
    accept(&mut w, B, RecruitmentId(13), &f);
    let mut evidence = outcome(10);
    evidence.bodies.get_mut(&B).unwrap().captive = true;
    w.prepare_outcome(ID, &evidence)
        .unwrap()
        .commit(&mut w)
        .unwrap();
    assert_eq!(
        w.missions[&ID].members[&B].status,
        CompanionStatus::Captured
    );
    evidence.bodies.get_mut(&B).unwrap().captive = false;
    w.prepare_outcome(ID, &evidence)
        .unwrap()
        .commit(&mut w)
        .unwrap();
    assert_eq!(
        w.missions[&ID].members[&B].status,
        CompanionStatus::Accepted
    );
    evidence.bodies.get_mut(&A).unwrap().alive = false;
    evidence.bodies.get_mut(&A).unwrap().area = None;
    evidence.receivers.clear();
    let plan = w.prepare_outcome(ID, &evidence).unwrap();
    assert!(plan.trust.is_empty());
    plan.commit(&mut w).unwrap();
    assert_eq!(w.missions[&ID].status, MissionStatus::Failed);
    assert_eq!(w.missions[&ID].members[&A].status, CompanionStatus::Dead);
    assert_eq!(w.recruitments[&OFFER].agreement, Some(AgreementId(30)));
}
#[test]
fn confinement_requires_actual_prior_capture_and_later_release_not_a_timer() {
    let mut w = expedition(MissionObjective::Confine {
        person: ENEMY,
        until: SimTime(100),
    });
    w.missions
        .get_mut(&ID)
        .unwrap()
        .members
        .get_mut(&A)
        .unwrap()
        .contacts
        .insert(ENEMY, HOME);
    receipt(
        &mut w,
        1,
        MissionReceipt::Captured {
            actor: A,
            target: ENEMY,
            at: SimTime(10),
        },
    );
    let mut f = outcome(100);
    f.leader_knowledge.target = Some(TargetObservation {
        captor: Some(A),
        restricted: true,
        ..target(100)
    });
    w.prepare_outcome(ID, &f).unwrap().commit(&mut w).unwrap();
    assert!(!w.missions[&ID].succeeded);
    assert_ne!(w.missions[&ID].status, MissionStatus::Completed);
    receipt(
        &mut w,
        2,
        MissionReceipt::Released {
            actor: A,
            target: ENEMY,
            at: SimTime(110),
        },
    );
    f.now = SimTime(110);
    f.leader_knowledge.target = Some(target(110));
    w.prepare_outcome(ID, &f).unwrap().commit(&mut w).unwrap();
    assert_eq!(w.missions[&ID].status, MissionStatus::Completed);
    assert!(w.missions[&ID].succeeded);
    let mut early = expedition(MissionObjective::Confine {
        person: ENEMY,
        until: SimTime(100),
    });
    receipt(
        &mut early,
        1,
        MissionReceipt::Captured {
            actor: A,
            target: ENEMY,
            at: SimTime(10),
        },
    );
    receipt(
        &mut early,
        2,
        MissionReceipt::Released {
            actor: A,
            target: ENEMY,
            at: SimTime(99),
        },
    );
    early
        .prepare_outcome(ID, &f)
        .unwrap()
        .commit(&mut early)
        .unwrap();
    assert!(!early.missions[&ID].succeeded);
}
#[test]
fn remote_or_stale_target_facts_cannot_prove_force_success_and_only_receivers_gain_trust() {
    let mut w = expedition(MissionObjective::Defeat { person: ENEMY });
    let mut f = outcome(100);
    f.leader_knowledge.target = Some(TargetObservation {
        alive: false,
        ..target(99)
    });
    w.prepare_outcome(ID, &f).unwrap().commit(&mut w).unwrap();
    assert!(!w.missions[&ID].succeeded);
    f.leader_knowledge.target = Some(TargetObservation {
        area: AWAY,
        alive: false,
        ..target(100)
    });
    w.prepare_outcome(ID, &f).unwrap().commit(&mut w).unwrap();
    assert!(!w.missions[&ID].succeeded);
    f.leader_knowledge.target = Some(TargetObservation {
        alive: false,
        ..target(100)
    });
    f.receivers = BTreeSet::from([B]);
    let plan = w.prepare_outcome(ID, &f).unwrap();
    assert_eq!(
        plan.trust,
        vec![TrustEffect {
            observer: B,
            leader: A,
            delta: 100
        }]
    );
    plan.commit(&mut w).unwrap();
    assert!(w.missions[&ID].succeeded);
    assert!(w.known.is_empty()); // Authority did not publish a remote person's knowledge.
}
#[test]
fn failure_waits_for_actual_return_and_stale_outcome_cannot_overwrite_new_cargo() {
    let mut w = expedition(MissionObjective::Deliver {
        source: InventoryId(20),
        destination: InventoryId(21),
        quantity: Quantity(1000),
    });
    let mut f = outcome(2000);
    f.bodies.get_mut(&A).unwrap().area = None;
    let stale = w.prepare_outcome(ID, &f).unwrap();
    receipt(
        &mut w,
        1,
        MissionReceipt::Transferred {
            actor: A,
            source: InventoryId(20),
            destination: InventoryId(40),
            quantity: Quantity(100),
            destination_carrier: Some(A),
        },
    );
    let before = w.clone();
    assert_eq!(stale.commit(&mut w), Err(Error::Unavailable));
    assert_eq!(w, before);
    w.prepare_outcome(ID, &f).unwrap().commit(&mut w).unwrap();
    assert_eq!(w.missions[&ID].status, MissionStatus::Returning);
    f.now = SimTime(2001);
    f.bodies.get_mut(&A).unwrap().area = Some(HOME);
    w.prepare_outcome(ID, &f).unwrap().commit(&mut w).unwrap();
    assert_eq!(w.missions[&ID].status, MissionStatus::Failed);
}
#[test]
fn personal_observation_and_reports_preserve_remote_dates_and_declared_explanations() {
    let mut w = expedition(MissionObjective::Visit { destination: AWAY });
    let f = ObservationFacts {
        now: SimTime(10),
        visible_locations: BTreeMap::from([
            (SP, Location::AtArea(HOME)),
            (A, Location::AtArea(HOME)),
            (B, Location::AtArea(HOME)),
        ]),
        heard: BTreeSet::from([SP, A, B]),
    };
    w.prepare_observation(SP, ID, &f)
        .unwrap()
        .commit(&mut w)
        .unwrap();
    let old = w.known[&SP][&ID].clone();
    let m = w.missions.get_mut(&ID).unwrap();
    m.status = MissionStatus::Failed;
    m.proposal.objective = MissionObjective::Defeat { person: ENEMY };
    let b = m.members.get_mut(&B).unwrap();
    b.status = CompanionStatus::Dead;
    b.explanation = "Private fears and calculations".into();
    b.declared_explanation = "Died".into();
    let remote = ObservationFacts {
        now: SimTime(20),
        visible_locations: BTreeMap::from([(SP, Location::AtArea(HOME))]),
        heard: BTreeSet::from([SP]),
    };
    let personal = w.prepare_observation(SP, ID, &remote).unwrap();
    assert_eq!(personal.snapshot.status, old.status);
    assert_eq!(personal.snapshot.objective, old.objective);
    assert_eq!(
        personal
            .snapshot
            .companions
            .iter()
            .find(|p| p.person == B)
            .unwrap()
            .status,
        CompanionStatus::Offered
    );
    personal.commit(&mut w).unwrap();
    w.prepare_report(SP, B, &w.known[&SP][&ID], SimTime(20))
        .unwrap()
        .unwrap()
        .commit(&mut w)
        .unwrap();
    assert_eq!(w.known[&B][&ID].status, old.status);
    let local = ObservationFacts {
        now: SimTime(30),
        ..f
    };
    w.prepare_observation(SP, ID, &local)
        .unwrap()
        .commit(&mut w)
        .unwrap();
    let current = &w.known[&SP][&ID];
    assert_eq!(current.status, MissionStatus::Failed);
    assert_eq!(
        current
            .companions
            .iter()
            .find(|p| p.person == B)
            .unwrap()
            .explanation,
        "Died"
    );
    w.prepare_report(SP, B, current, SimTime(30))
        .unwrap()
        .unwrap()
        .commit(&mut w)
        .unwrap();
    assert!(
        w.prepare_report(SP, B, &old, SimTime(40))
            .unwrap()
            .is_none()
    );
}
fn payment_view(defaulted: bool) -> AgreementView {
    AgreementView {
        id: AgreementId(30),
        proposer: SP,
        parties: BTreeSet::from([SP, A]),
        accepted: BTreeSet::from([SP, A]),
        status: if defaulted {
            AgreementStatus::Breached
        } else {
            AgreementStatus::Active
        },
        terms: vec![payment()],
        obligations: vec![ObligationView {
            id: ObligationId(31),
            term: payment(),
            performed: 0,
            accrued: 0,
            share_remainder: 0,
            share_scale: 0,
            defaulted,
            cargo: BTreeMap::new(),
            observed_at: SimTime(10),
            learned_at: SimTime(10),
            source: Some(KnowledgeSource::Observed(A)),
        }],
        procedure: None,
        amendments: Vec::new(),
        revision: 0,
        observed_at: SimTime(10),
        learned_at: SimTime(10),
        source: Some(KnowledgeSource::Observed(A)),
    }
}
#[test]
fn known_failed_service_and_personal_values_determine_withdrawal_or_actual_defection_request() {
    let mut w = expedition(MissionObjective::Defeat { person: ENEMY });
    w.recruitments.get_mut(&OFFER).unwrap().agreement = Some(AgreementId(30));
    let motives = ServiceMotives {
        honesty: 400,
        leader_trust: 0,
        guardian_duty: false,
        payment: None,
    };
    let mut f = DefectionFacts {
        motives: motives.clone(),
        own_team: 0,
        opponents: BTreeMap::from([(
            ENEMY,
            ObservedOpponent {
                trust: 900,
                team: 1,
            },
        )]),
    };
    assert!(w.prepare_defection(A, ID, &f).unwrap().is_none());
    assert!(w.service_withdrawal(A, ID, &motives).is_none());
    f.motives.payment = Some(payment_view(true));
    let plan = w.prepare_defection(A, ID, &f).unwrap().unwrap();
    assert_eq!(
        plan.allegiance,
        Some(AllegianceEffect {
            actor: A,
            opponent: ENEMY,
            team: 1
        })
    );
    assert_eq!(
        w.service_withdrawal(A, ID, &f.motives),
        Some(MissionAction::Leave { mission: ID })
    );
    let honorable = ServiceMotives {
        honesty: 900,
        ..f.motives.clone()
    };
    assert!(w.service_withdrawal(A, ID, &honorable).is_none());
    plan.commit(&mut w).unwrap();
    assert_eq!(
        w.missions[&ID].members[&A].status,
        CompanionStatus::Defected
    );
    assert_eq!(w.recruitments[&OFFER].agreement, Some(AgreementId(30)));
}
#[test]
fn fleeing_uses_own_injury_and_remembered_return_order_not_remote_authority() {
    assert!(combat_retreat_wanted(40, 100, 500, None));
    assert!(!combat_retreat_wanted(40, 100, 700, None));
    assert!(!combat_retreat_wanted(100, 100, 500, None));
    assert!(combat_retreat_wanted(
        100,
        100,
        500,
        Some(MissionStatus::Returning)
    ));
}
