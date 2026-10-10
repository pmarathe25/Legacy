use game_api::*;
use game_world::{
    decisions::{Aspiration, Personality},
    *,
};
use std::collections::{BTreeMap, BTreeSet};

fn fixture(workers: usize) -> Simulation {
    let mut sim = Simulation::agreement_fixture(
        7,
        Execution {
            workers,
            reverse_jobs: true,
        },
    )
    .unwrap();
    let people: Vec<_> = sim.canonical_state().people.iter().map(|p| p.id).collect();
    for person in people {
        sim.debug_set_personality(person, Personality::restful(), Aspiration::Comfort)
            .unwrap();
    }
    sim
}
fn ruler(sim: &Simulation) -> PersonId {
    sim.canonical_state().ruler
}
fn citizen(sim: &Simulation) -> PersonId {
    sim.canonical_state().people[3].id
}
fn action(sim: &mut Simulation, action: AgreementAction) -> Result<CommandReceipt, Error> {
    sim.submit(Submission {
        caller_sequence: sim.next_caller_sequence().unwrap(),
        command: Command::Agreement { action },
    })
}
fn command(sim: &mut Simulation, command: Command) {
    sim.submit(Submission {
        caller_sequence: sim.next_caller_sequence().unwrap(),
        command,
    })
    .unwrap();
}
fn food(sim: &Simulation) -> InventoryId {
    sim.canonical_state()
        .inventories
        .values()
        .find(|i| i.good == Good::Food && i.location == InventoryLocation::Area(AreaId(1)))
        .unwrap()
        .id
}
fn labor(sim: &Simulation, millis: u64) -> AgreementTerm {
    let site = sim
        .canonical_state()
        .physical
        .as_ref()
        .unwrap()
        .sites
        .values()
        .find(|s| s.area == AreaId(1))
        .unwrap()
        .id;
    AgreementTerm::Labor {
        debtor: ruler(sim),
        creditor: citizen(sim),
        site,
        millis,
        deadline: SimTime(300_000),
    }
}
fn propose(
    sim: &mut Simulation,
    terms: Vec<AgreementTerm>,
    procedure: Option<AmendmentProcedure>,
) -> AgreementId {
    let parties = BTreeSet::from([ruler(sim), citizen(sim)]);
    action(
        sim,
        AgreementAction::Propose {
            parties,
            terms,
            procedure,
        },
    )
    .unwrap();
    sim.agreement_view(ruler(sim)).last().unwrap().id
}
fn accept(sim: &mut Simulation, id: AgreementId) {
    sim.debug_agreement_action(citizen(sim), AgreementAction::Accept { agreement: id })
        .unwrap();
}

#[test]
fn signature_creates_debt_and_cargo_performance_requires_actual_loading_and_delivery() {
    let mut sim = fixture(2);
    let source = food(&sim);
    let carrier = sim.canonical_state().people[0].inventory;
    let destination = sim
        .debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(AreaId(1)),
            Quantity(0),
            Quantity(10_000),
            BTreeSet::from([ruler(&sim), citizen(&sim)]),
        )
        .unwrap();
    let term = AgreementTerm::Delivery {
        debtor: ruler(&sim),
        creditor: citizen(&sim),
        source,
        destination,
        quantity: Quantity(2_000),
        deadline: SimTime(300_000),
    };
    let id = propose(&mut sim, vec![term], None);
    let before = sim.canonical_state().inventories.clone();
    accept(&mut sim, id);
    assert_eq!(sim.canonical_state().inventories, before);
    assert_eq!(
        sim.agreement_view(ruler(&sim))[0].obligations[0].performed,
        0
    );
    command(
        &mut sim,
        Command::Load {
            source,
            destination: carrier,
            quantity: Quantity(2_000),
        },
    );
    sim.advance(MINUTE).unwrap();
    assert_eq!(
        sim.agreement_view(ruler(&sim))[0].obligations[0].performed,
        0
    );
    command(
        &mut sim,
        Command::Load {
            source: carrier,
            destination,
            quantity: Quantity(1_000),
        },
    );
    sim.advance(MINUTE).unwrap();
    let obligation = &sim.agreement_view(ruler(&sim))[0].obligations[0];
    assert_eq!(obligation.performed, 1_000);
    assert_eq!(
        sim.canonical_state()
            .agreements
            .as_ref()
            .unwrap()
            .obligations[&obligation.id]
            .cargo[&carrier],
        1_000
    );
    let mut restored = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    command(
        &mut restored,
        Command::Load {
            source: carrier,
            destination,
            quantity: Quantity(1_000),
        },
    );
    restored.advance(MINUTE).unwrap();
    assert_eq!(
        restored.agreement_view(ruler(&restored))[0].status,
        AgreementStatus::Fulfilled
    );
    assert_eq!(
        restored.canonical_state().inventories[&destination].quantity,
        Quantity(2_000)
    );
}

#[test]
fn default_preserves_debt_and_undelivered_goods() {
    let mut sim = fixture(1);
    let source = food(&sim);
    let destination = sim
        .debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(AreaId(2)),
            Quantity(0),
            Quantity(10_000),
            BTreeSet::from([ruler(&sim)]),
        )
        .unwrap();
    let term = AgreementTerm::Delivery {
        debtor: ruler(&sim),
        creditor: citizen(&sim),
        source,
        destination,
        quantity: Quantity(1_000),
        deadline: SimTime(10_000),
    };
    let id = propose(&mut sim, vec![term], None);
    accept(&mut sim, id);
    sim.advance(SimDuration(10_000)).unwrap();
    let view = &sim.agreement_view(ruler(&sim))[0];
    assert_eq!(view.status, AgreementStatus::Breached);
    assert!(view.obligations[0].defaulted);
    assert_eq!(view.obligations[0].performed, 0);
    assert_eq!(
        sim.canonical_state().inventories[&destination].quantity,
        Quantity(0)
    );
    assert_eq!(
        sim.canonical_state().inventories[&source].quantity,
        Quantity(100_000)
    );
}

#[test]
fn physical_work_is_credited_once_across_competing_promises_and_chunks() {
    let mut a = fixture(1);
    let term = labor(&a, 60_000);
    let first = propose(&mut a, vec![term.clone()], None);
    accept(&mut a, first);
    let second = propose(&mut a, vec![term.clone()], None);
    accept(&mut a, second);
    let AgreementTerm::Labor { site, .. } = term else {
        unreachable!()
    };
    command(&mut a, Command::Work { site });
    let mut b = Simulation::load(
        &a.save().unwrap(),
        Execution {
            workers: 4,
            reverse_jobs: true,
        },
    )
    .unwrap();
    a.advance(MINUTE).unwrap();
    b.advance(SimDuration(17_001)).unwrap();
    for _ in 0..10 {
        let _ = b.agreement_view(ruler(&b));
        let _ = b.observe();
    }
    b.advance(SimDuration(42_999)).unwrap();
    assert_eq!(a.canonical_state(), b.canonical_state());
    let views = a.agreement_view(ruler(&a));
    assert_eq!(views[0].obligations[0].performed, 60_000);
    assert_eq!(views[1].obligations[0].performed, 0);
}

#[test]
fn amendment_authority_comes_from_the_procedure_people_accepted() {
    for threshold in [1, 2] {
        let mut sim = fixture(1);
        let procedure = AmendmentProcedure {
            eligible_proposers: BTreeSet::from([ruler(&sim)]),
            approvers: BTreeSet::from([ruler(&sim), citizen(&sim)]),
            threshold,
            term_approvers: BTreeMap::new(),
        };
        let original = labor(&sim, 60_000);
        let id = propose(&mut sim, vec![original], Some(procedure));
        accept(&mut sim, id);
        let replacement = labor(&sim, 120_000);
        action(
            &mut sim,
            AgreementAction::Amend {
                agreement: id,
                replacements: BTreeMap::from([(0, replacement.clone())]),
            },
        )
        .unwrap();
        let amendment = sim.agreement_view(ruler(&sim))[0].amendments[0].id;
        action(
            &mut sim,
            AgreementAction::Vote {
                amendment,
                support: true,
            },
        )
        .unwrap();
        assert_eq!(
            sim.agreement_view(ruler(&sim))[0].obligations[0]
                .term
                .amount(),
            if threshold == 1 { 120_000 } else { 60_000 }
        );
        if threshold == 2 {
            sim.debug_agreement_action(
                citizen(&sim),
                AgreementAction::Vote {
                    amendment,
                    support: true,
                },
            )
            .unwrap();
        }
        assert_eq!(
            sim.agreement_view(ruler(&sim))[0].obligations[0].term,
            replacement
        );
    }
    let mut sim = fixture(1);
    let term = labor(&sim, 60_000);
    let id = propose(&mut sim, vec![term.clone()], None);
    accept(&mut sim, id);
    let before = sim.canonical_state().clone();
    assert_eq!(
        action(
            &mut sim,
            AgreementAction::Amend {
                agreement: id,
                replacements: BTreeMap::from([(0, term)])
            }
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(sim.canonical_state(), &before);
}

#[test]
fn npc_uses_own_knowledge_to_refuse_or_counteroffer_and_can_accept_useful_trade() {
    let mut sim = fixture(1);
    let npc = citizen(&sim);
    let site = sim
        .canonical_state()
        .physical
        .as_ref()
        .unwrap()
        .sites
        .values()
        .find(|s| s.area == AreaId(1))
        .unwrap()
        .id;
    let terms = vec![AgreementTerm::Labor {
        debtor: npc,
        creditor: ruler(&sim),
        site,
        millis: 180_000,
        deadline: SimTime(300_000),
    }];
    let id = propose(&mut sim, terms, None);
    sim.advance(MINUTE).unwrap();
    assert_ne!(
        sim.agreement_view(ruler(&sim))[0].status,
        AgreementStatus::Active
    );
    assert!(matches!(
        sim.canonical_state()
            .agreements
            .as_ref()
            .unwrap()
            .agreements[&id]
            .status,
        AgreementStatus::Rejected | AgreementStatus::Withdrawn
    ));
    let source = food(&sim);
    let term = AgreementTerm::Delivery {
        debtor: ruler(&sim),
        creditor: npc,
        source,
        destination: sim.canonical_state().people[3].inventory,
        quantity: Quantity(1_000),
        deadline: SimTime(300_000),
    };
    let useful = propose(&mut sim, vec![term], None);
    sim.advance(MINUTE).unwrap();
    assert_eq!(
        sim.canonical_state()
            .agreements
            .as_ref()
            .unwrap()
            .agreements[&useful]
            .status,
        AgreementStatus::Active
    );
}

#[test]
fn permission_is_chosen_by_the_actual_custodian_without_relocating_goods() {
    let mut sim = fixture(1);
    let source = food(&sim);
    let owner = ruler(&sim);
    let recipient = citizen(&sim);
    assert!(
        !sim.canonical_state().inventories[&source]
            .access
            .contains(&recipient)
    );
    let before = sim.canonical_state().inventories[&source].clone();
    let id = propose(
        &mut sim,
        vec![AgreementTerm::Access {
            grantor: owner,
            grantee: recipient,
            inventory: source,
            allowed: true,
        }],
        None,
    );
    accept(&mut sim, id);
    let after = &sim.canonical_state().inventories[&source];
    assert!(after.access.contains(&recipient));
    assert_eq!(before.quantity, after.quantity);
    assert_eq!(before.location, after.location);
    let invalid = AgreementTerm::Access {
        grantor: recipient,
        grantee: owner,
        inventory: source,
        allowed: false,
    };
    let before = sim.canonical_state().clone();
    assert!(
        action(
            &mut sim,
            AgreementAction::Propose {
                parties: BTreeSet::from([owner, recipient]),
                terms: vec![invalid],
                procedure: None
            }
        )
        .is_err()
    );
    assert_eq!(sim.canonical_state(), &before);
}

#[test]
fn competing_deliveries_do_not_double_credit_and_consumed_cargo_remains_owed() {
    let mut sim = fixture(1);
    let source = food(&sim);
    let actor = ruler(&sim);
    let recipient = citizen(&sim);
    let cargo = sim.canonical_state().people[0].inventory;
    let destination = sim
        .debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(AreaId(1)),
            Quantity(0),
            Quantity(10_000),
            BTreeSet::from([actor, recipient]),
        )
        .unwrap();
    for _ in 0..2 {
        let term = AgreementTerm::Delivery {
            debtor: actor,
            creditor: recipient,
            source,
            destination,
            quantity: Quantity(1_000),
            deadline: SimTime(300_000),
        };
        let id = propose(&mut sim, vec![term], None);
        accept(&mut sim, id);
    }
    command(
        &mut sim,
        Command::Load {
            source,
            destination: cargo,
            quantity: Quantity(2_000),
        },
    );
    sim.advance(MINUTE).unwrap();
    command(
        &mut sim,
        Command::Load {
            source: cargo,
            destination,
            quantity: Quantity(1_000),
        },
    );
    sim.advance(MINUTE).unwrap();
    let views = sim.agreement_view(actor);
    assert_eq!(views[0].obligations[0].performed, 1_000);
    assert_eq!(views[1].obligations[0].performed, 0);
    command(&mut sim, Command::Eat { source: cargo });
    sim.advance(MINUTE).unwrap();
    let id = sim.agreement_view(actor)[1].obligations[0].id;
    assert!(
        sim.canonical_state()
            .agreements
            .as_ref()
            .unwrap()
            .obligations[&id]
            .cargo
            .is_empty()
    );
    assert_eq!(sim.agreement_view(actor)[1].obligations[0].performed, 0);
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
}

#[test]
fn hungry_people_construct_local_exchange_proposals_from_known_opportunities() {
    let mut sim = fixture(1);
    sim.advance(SimDuration(180_000)).unwrap();
    let offered = sim.agreement_view(ruler(&sim));
    assert!(offered.iter().any(|a| {
        a.proposer != ruler(&sim)
            && a.status == AgreementStatus::Proposed
            && a.terms
                .iter()
                .any(|t| matches!(t, AgreementTerm::Access { allowed: true, .. }))
            && a.terms
                .iter()
                .any(|t| matches!(t, AgreementTerm::Labor { .. }))
    }));
}

#[test]
fn accepted_receipt_does_not_allow_remote_loading_or_interrupt_a_valid_action() {
    let mut sim = fixture(1);
    let actor = ruler(&sim);
    let recipient = citizen(&sim);
    let source = food(&sim);
    let cargo = sim.canonical_state().people[0].inventory;
    let destination = sim.canonical_state().people[3].inventory;
    let id = propose(
        &mut sim,
        vec![AgreementTerm::Delivery {
            debtor: actor,
            creditor: recipient,
            source,
            destination,
            quantity: Quantity(1_000),
            deadline: SimTime(300_000),
        }],
        None,
    );
    accept(&mut sim, id);
    command(
        &mut sim,
        Command::Load {
            source,
            destination: cargo,
            quantity: Quantity(1_000),
        },
    );
    sim.advance(MINUTE).unwrap();
    sim.debug_move_actor(recipient, AreaId(2)).unwrap();
    command(&mut sim, Command::Rest);
    let before = sim.canonical_state().clone();
    let result = sim.submit(Submission {
        caller_sequence: sim.next_caller_sequence().unwrap(),
        command: Command::Load {
            source: cargo,
            destination,
            quantity: Quantity(1_000),
        },
    });
    assert_eq!(result, Err(Error::Unavailable));
    assert_eq!(sim.canonical_state(), &before);
    assert_eq!(sim.observe().physical.unwrap().activity, Activity::Rest);
}

#[test]
fn named_term_approvers_can_vote_and_stale_concurrent_amendments_cannot_overwrite() {
    let mut sim = fixture(1);
    let actor = ruler(&sim);
    let npc = citizen(&sim);
    let procedure = AmendmentProcedure {
        eligible_proposers: BTreeSet::from([actor]),
        approvers: BTreeSet::from([actor]),
        threshold: 1,
        term_approvers: BTreeMap::from([(0, BTreeSet::from([npc]))]),
    };
    let term = labor(&sim, 60_000);
    let id = propose(&mut sim, vec![term], Some(procedure));
    accept(&mut sim, id);
    for millis in [90_000, 120_000] {
        let replacement = labor(&sim, millis);
        action(
            &mut sim,
            AgreementAction::Amend {
                agreement: id,
                replacements: BTreeMap::from([(0, replacement)]),
            },
        )
        .unwrap();
    }
    let first = sim.agreement_view(actor)[0].amendments[0].id;
    let second = sim.agreement_view(actor)[0].amendments[1].id;
    action(
        &mut sim,
        AgreementAction::Vote {
            amendment: first,
            support: true,
        },
    )
    .unwrap();
    assert_eq!(
        sim.agreement_view(actor)[0].obligations[0].term.amount(),
        60_000
    );
    sim.advance(MINUTE).unwrap();
    assert_eq!(
        sim.agreement_view(actor)[0].obligations[0].term.amount(),
        90_000
    );
    assert_eq!(
        sim.agreement_view(actor)[0].amendments[1].status,
        AmendmentStatus::Rejected
    );
    let before = sim.canonical_state().clone();
    assert!(
        action(
            &mut sim,
            AgreementAction::Vote {
                amendment: second,
                support: true
            }
        )
        .is_err()
    );
    assert_eq!(sim.canonical_state(), &before);
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
}
