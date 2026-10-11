mod support;
use game_api::*;
use game_world::{
    decisions::{Aspiration, Personality},
    *,
};
use std::collections::{BTreeMap, BTreeSet};

fn quiet(sim: &mut Simulation) {
    let actors: Vec<_> = sim
        .canonical_state()
        .people
        .iter()
        .filter(|p| p.id != sim.canonical_state().ruler)
        .map(|p| p.id)
        .collect();
    for actor in actors {
        let mut personality = Personality::restful();
        personality.discipline = 0;
        personality.honesty = 0;
        personality.empathy = 0;
        sim.debug_set_personality(actor, personality, Aspiration::Comfort)
            .unwrap();
    }
}
fn sim() -> Simulation {
    let mut sim = Simulation::agreement_fixture(1, Execution::default()).unwrap();
    quiet(&mut sim);
    let mut saved: serde_json::Value = serde_json::from_slice(&sim.save().unwrap()).unwrap();
    saved["state"]["physical"]["rules"]["hunger_harm"] = serde_json::json!(1_000_000_000_000_u64);
    saved["state"]["physical"]["rules"]["meal_relief"] = serde_json::json!(1_000_000_000_000_u64);
    saved["state"]["physical"]["rules"]["fatigue_limit"] = serde_json::json!(1_000_000_000_000_u64);
    Simulation::load(&serde_json::to_vec(&saved).unwrap(), Execution::default()).unwrap()
}
fn citizen(sim: &Simulation) -> PersonId {
    sim.canonical_state()
        .people
        .iter()
        .find(|p| p.role == Role::Citizen)
        .unwrap()
        .id
}
fn timber(sim: &Simulation, area: AreaId) -> InventoryId {
    sim.canonical_state()
        .inventories
        .values()
        .find(|stock| stock.good == Good::Timber && stock.location == InventoryLocation::Area(area))
        .unwrap()
        .id
}
fn cargo(sim: &Simulation, actor: PersonId) -> InventoryId {
    sim.canonical_state()
        .inventories
        .values()
        .find(|stock| {
            stock.good == Good::Timber && stock.location == InventoryLocation::Carried(actor)
        })
        .unwrap()
        .id
}
fn known(sim: &Simulation, actor: PersonId, id: AgreementId) -> AgreementView {
    sim.agreement_view(actor)
        .into_iter()
        .find(|a| a.id == id)
        .unwrap()
}
fn propose(
    sim: &mut Simulation,
    debtor: PersonId,
    terms: Vec<AgreementTerm>,
    procedure: Option<AmendmentProcedure>,
) -> AgreementId {
    let ruler = sim.canonical_state().ruler;
    sim.debug_agreement_action(
        ruler,
        AgreementAction::Propose {
            parties: BTreeSet::from([ruler, debtor]),
            terms,
            procedure,
        },
    )
    .unwrap();
    let id = *sim
        .canonical_state()
        .agreements
        .as_ref()
        .unwrap()
        .agreements
        .keys()
        .next_back()
        .unwrap();
    sim.debug_agreement_action(debtor, AgreementAction::Accept { agreement: id })
        .unwrap();
    id
}
fn delivery(
    debtor: PersonId,
    creditor: PersonId,
    source: InventoryId,
    destination: InventoryId,
    deadline: u64,
) -> AgreementTerm {
    AgreementTerm::Delivery {
        debtor,
        creditor,
        source,
        destination,
        quantity: Quantity(1_000),
        deadline: SimTime(deadline),
    }
}
fn load(sim: &mut Simulation, actor: PersonId, source: InventoryId, destination: InventoryId) {
    sim.debug_actor_command(
        actor,
        Command::Load {
            source,
            destination,
            quantity: Quantity(1_000),
        },
    )
    .unwrap();
    sim.advance(MINUTE).unwrap();
}

#[test]
fn a_remote_creditor_learns_a_delivery_only_through_an_actual_timed_report() {
    let mut sim = sim();
    let creditor = sim.canonical_state().ruler;
    let debtor = citizen(&sim);
    let source = timber(&sim, AreaId(1));
    let destination = sim
        .debug_add_inventory(
            Good::Timber,
            InventoryLocation::Area(AreaId(1)),
            Quantity(0),
            Quantity(10_000),
            BTreeSet::from([creditor, debtor]),
        )
        .unwrap();
    let id = propose(
        &mut sim,
        debtor,
        vec![delivery(debtor, creditor, source, destination, 600_000)],
        None,
    );
    sim.debug_move_actor(creditor, AreaId(2)).unwrap();
    sim.advance(SimDuration(120_000)).unwrap();
    let carrying = cargo(&sim, debtor);
    load(&mut sim, debtor, source, carrying);
    load(&mut sim, debtor, carrying, destination);
    assert_eq!(known(&sim, debtor, id).obligations[0].performed, 1_000);
    assert_eq!(known(&sim, creditor, id).obligations[0].performed, 0);
    let before = sim.canonical_state().clone();
    for _ in 0..100 {
        sim.agreement_view(creditor);
        sim.observe();
    }
    assert_eq!(sim.canonical_state(), &before);
    sim.debug_move_actor(creditor, AreaId(1)).unwrap();
    sim.advance(SimDuration(120_000)).unwrap();
    assert_eq!(known(&sim, creditor, id).obligations[0].performed, 0);
    sim.debug_agreement_action(
        debtor,
        AgreementAction::Report {
            agreement: id,
            recipient: creditor,
        },
    )
    .unwrap();
    sim.advance(SimDuration(59_999)).unwrap();
    assert_eq!(known(&sim, creditor, id).obligations[0].performed, 0);
    sim.advance(SimDuration(1)).unwrap();
    let view = known(&sim, creditor, id);
    assert_eq!(view.status, AgreementStatus::Fulfilled);
    assert_eq!(view.obligations[0].performed, 1_000);
    assert_eq!(view.obligations[0].observed_at, SimTime(240_000));
    assert_eq!(view.obligations[0].learned_at, SimTime(420_000));
    assert_eq!(
        view.obligations[0].source,
        Some(KnowledgeSource::Reported(debtor))
    );
}

#[test]
fn witnessing_one_obligation_does_not_refresh_another_remote_obligation() {
    let mut sim = sim();
    let creditor = sim.canonical_state().ruler;
    let debtor = citizen(&sim);
    let local_source = timber(&sim, AreaId(1));
    let local_goal = sim
        .debug_add_inventory(
            Good::Timber,
            InventoryLocation::Area(AreaId(1)),
            Quantity(0),
            Quantity(10_000),
            BTreeSet::from([creditor, debtor]),
        )
        .unwrap();
    let remote_source = sim
        .debug_add_inventory(
            Good::Timber,
            InventoryLocation::Area(AreaId(2)),
            Quantity(2_000),
            Quantity(10_000),
            BTreeSet::from([creditor, debtor]),
        )
        .unwrap();
    let remote_goal = sim
        .debug_add_inventory(
            Good::Timber,
            InventoryLocation::Area(AreaId(2)),
            Quantity(0),
            Quantity(10_000),
            BTreeSet::from([creditor, debtor]),
        )
        .unwrap();
    let id = propose(
        &mut sim,
        debtor,
        vec![
            delivery(debtor, creditor, local_source, local_goal, 900_000),
            delivery(debtor, creditor, remote_source, remote_goal, 900_000),
        ],
        None,
    );
    sim.debug_move_actor(debtor, AreaId(2)).unwrap();
    sim.advance(SimDuration(120_000)).unwrap();
    let carrying = cargo(&sim, debtor);
    load(&mut sim, debtor, remote_source, carrying);
    load(&mut sim, debtor, carrying, remote_goal);
    sim.debug_move_actor(debtor, AreaId(1)).unwrap();
    sim.advance(SimDuration(120_000)).unwrap();
    load(&mut sim, debtor, local_source, carrying);
    load(&mut sim, debtor, carrying, local_goal);
    let view = known(&sim, creditor, id);
    assert_eq!(view.obligations[0].performed, 1_000);
    assert_eq!(view.obligations[1].performed, 0);
    assert_eq!(view.status, AgreementStatus::Active);
    assert_eq!(
        sim.canonical_state()
            .agreements
            .as_ref()
            .unwrap()
            .obligations[&view.obligations[1].id]
            .performed,
        1_000
    );
}

#[test]
fn a_remote_default_changes_trust_only_after_known_notice_and_a_late_delivery_updates_debt() {
    let mut sim = sim();
    let creditor = sim.canonical_state().ruler;
    let debtor = citizen(&sim);
    let source = timber(&sim, AreaId(1));
    let destination = sim
        .debug_add_inventory(
            Good::Timber,
            InventoryLocation::Area(AreaId(1)),
            Quantity(0),
            Quantity(10_000),
            BTreeSet::from([creditor, debtor]),
        )
        .unwrap();
    let id = propose(
        &mut sim,
        debtor,
        vec![delivery(debtor, creditor, source, destination, 180_000)],
        None,
    );
    sim.debug_move_actor(creditor, AreaId(2)).unwrap();
    sim.advance(SimDuration(180_000)).unwrap();
    assert!(known(&sim, debtor, id).obligations[0].defaulted);
    assert!(!known(&sim, creditor, id).obligations[0].defaulted);
    let before = sim.canonical_state().decisions.as_ref().unwrap().minds[&creditor]
        .relationships
        .get(&debtor)
        .map_or(0, |r| r.trust);
    sim.debug_move_actor(creditor, AreaId(1)).unwrap();
    sim.advance(SimDuration(120_000)).unwrap();
    sim.debug_agreement_action(
        debtor,
        AgreementAction::Report {
            agreement: id,
            recipient: creditor,
        },
    )
    .unwrap();
    sim.advance(MINUTE).unwrap();
    assert!(known(&sim, creditor, id).obligations[0].defaulted);
    let after = sim.canonical_state().decisions.as_ref().unwrap().minds[&creditor].relationships
        [&debtor]
        .trust;
    assert!(after < before);
    let carrying = cargo(&sim, debtor);
    load(&mut sim, debtor, source, carrying);
    load(&mut sim, debtor, carrying, destination);
    let view = known(&sim, creditor, id);
    assert_eq!(view.status, AgreementStatus::Fulfilled);
    assert!(view.obligations[0].defaulted);
    assert_eq!(view.obligations[0].performed, 1_000);
    assert_eq!(
        sim.canonical_state().decisions.as_ref().unwrap().minds[&creditor].relationships[&debtor]
            .trust,
        after
    );
}

#[test]
fn chosen_amendment_procedures_can_forgive_to_zero_but_initial_zero_terms_are_inert() {
    let mut sim = sim();
    let creditor = sim.canonical_state().ruler;
    let debtor = citizen(&sim);
    let source = timber(&sim, AreaId(1));
    let destination = sim
        .debug_add_inventory(
            Good::Timber,
            InventoryLocation::Area(AreaId(1)),
            Quantity(0),
            Quantity(10_000),
            BTreeSet::from([creditor, debtor]),
        )
        .unwrap();
    let zero = AgreementTerm::Delivery {
        debtor,
        creditor,
        source,
        destination,
        quantity: Quantity(0),
        deadline: SimTime(180_000),
    };
    let opening = sim.canonical_state().clone();
    assert!(
        sim.debug_agreement_action(
            creditor,
            AgreementAction::Propose {
                parties: BTreeSet::from([creditor, debtor]),
                terms: vec![zero.clone()],
                procedure: None
            }
        )
        .is_err()
    );
    assert_eq!(sim.canonical_state(), &opening);
    let procedure = AmendmentProcedure {
        eligible_proposers: BTreeSet::from([creditor]),
        approvers: BTreeSet::from([creditor]),
        threshold: 1,
        term_approvers: BTreeMap::new(),
    };
    let id = propose(
        &mut sim,
        debtor,
        vec![delivery(debtor, creditor, source, destination, 180_000)],
        Some(procedure),
    );
    sim.advance(SimDuration(180_000)).unwrap();
    assert!(known(&sim, creditor, id).obligations[0].defaulted);
    let stocks = sim.canonical_state().inventories.clone();
    sim.debug_agreement_action(
        creditor,
        AgreementAction::Amend {
            agreement: id,
            replacements: BTreeMap::from([(0, zero)]),
        },
    )
    .unwrap();
    let amendment = *sim
        .canonical_state()
        .agreements
        .as_ref()
        .unwrap()
        .amendments
        .keys()
        .next_back()
        .unwrap();
    sim.debug_agreement_action(
        creditor,
        AgreementAction::Vote {
            amendment,
            support: true,
        },
    )
    .unwrap();
    let view = known(&sim, creditor, id);
    assert_eq!(view.status, AgreementStatus::Fulfilled);
    assert_eq!(view.obligations[0].term.amount(), 0);
    assert!(!view.obligations[0].defaulted);
    assert_eq!(sim.canonical_state().inventories, stocks);
    Simulation::load(
        &sim.save().unwrap(),
        Execution {
            workers: 4,
            reverse_jobs: true,
        },
    )
    .unwrap();
}

#[test]
fn stale_report_and_its_saved_snapshot_cannot_undo_a_later_known_amendment() {
    let mut sim = sim();
    let creditor = sim.canonical_state().ruler;
    let debtor = citizen(&sim);
    let source = timber(&sim, AreaId(1));
    let destination = sim
        .debug_add_inventory(
            Good::Timber,
            InventoryLocation::Area(AreaId(1)),
            Quantity(0),
            Quantity(10_000),
            BTreeSet::from([creditor, debtor]),
        )
        .unwrap();
    let procedure = AmendmentProcedure {
        eligible_proposers: BTreeSet::from([creditor]),
        approvers: BTreeSet::from([creditor]),
        threshold: 1,
        term_approvers: BTreeMap::new(),
    };
    let id = propose(
        &mut sim,
        debtor,
        vec![delivery(debtor, creditor, source, destination, 600_000)],
        Some(procedure),
    );
    sim.debug_agreement_action(
        debtor,
        AgreementAction::Report {
            agreement: id,
            recipient: creditor,
        },
    )
    .unwrap();
    let zero = AgreementTerm::Delivery {
        debtor,
        creditor,
        source,
        destination,
        quantity: Quantity(0),
        deadline: SimTime(600_000),
    };
    sim.debug_agreement_action(
        creditor,
        AgreementAction::Amend {
            agreement: id,
            replacements: BTreeMap::from([(0, zero)]),
        },
    )
    .unwrap();
    let amendment = *sim
        .canonical_state()
        .agreements
        .as_ref()
        .unwrap()
        .amendments
        .keys()
        .next_back()
        .unwrap();
    sim.debug_agreement_action(
        creditor,
        AgreementAction::Vote {
            amendment,
            support: true,
        },
    )
    .unwrap();
    let saved = sim.save().unwrap();
    let mut restored = Simulation::load(
        &saved,
        Execution {
            workers: 4,
            reverse_jobs: true,
        },
    )
    .unwrap();
    let mut invalid: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    support::component(&mut invalid, "coordination/activity", debtor)["ReportAgreement"]["snapshot"]
        ["source"] = serde_json::json!({"Recorded": 999});
    assert!(
        Simulation::load(&serde_json::to_vec(&invalid).unwrap(), Execution::default()).is_err()
    );
    sim.advance(MINUTE).unwrap();
    for duration in [1, 29_999, 30_000] {
        restored.advance(SimDuration(duration)).unwrap();
    }
    assert_eq!(sim.canonical_state(), restored.canonical_state());
    let view = known(&sim, creditor, id);
    assert_eq!(view.revision, 1);
    assert_eq!(view.obligations[0].term.amount(), 0);
    assert_eq!(view.status, AgreementStatus::Fulfilled);
}

#[test]
fn a_bound_conscious_guardian_can_accept_but_cannot_physically_work() {
    let mut sim = Simulation::property_fixture(1, Execution::default()).unwrap();
    quiet(&mut sim);
    let ruler = sim.canonical_state().ruler;
    let guardian = sim.canonical_state().people[1].id;
    sim.submit(Submission {
        caller_sequence: 1,
        command: Command::Engage { target: guardian },
    })
    .unwrap();
    let encounter = sim.observe().combat.unwrap().encounter;
    for _ in 0..4 {
        sim.submit(Submission {
            caller_sequence: sim.next_caller_sequence().unwrap(),
            command: Command::CombatTurn {
                encounter,
                movement: None,
                action: CombatAction::Attack { target: guardian },
            },
        })
        .unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
    }
    sim.submit(Submission {
        caller_sequence: sim.next_caller_sequence().unwrap(),
        command: Command::CombatTurn {
            encounter,
            movement: None,
            action: CombatAction::Capture { target: guardian },
        },
    })
    .unwrap();
    sim.advance(SimDuration(6_000)).unwrap();
    let site = sim
        .canonical_state()
        .components
        .sites()
        .map(|(_, site)| site)
        .find(|site| site.area == AreaId(1))
        .unwrap()
        .id;
    let id = propose(
        &mut sim,
        guardian,
        vec![AgreementTerm::Labor {
            debtor: guardian,
            creditor: ruler,
            site,
            millis: 60_000,
            deadline: SimTime(600_000),
        }],
        None,
    );
    assert_eq!(known(&sim, guardian, id).status, AgreementStatus::Active);
    assert!(
        sim.debug_actor_command(guardian, Command::Work { site })
            .is_err()
    );
    assert!(
        sim.canonical_state()
            .combat
            .as_ref()
            .unwrap()
            .captivity
            .contains_key(&guardian)
    );
}
