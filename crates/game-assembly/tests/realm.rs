use game_api::*;
use game_assembly::{Execution, Simulation};
use game_realm::{Participant, Progress, Proposal, Status, Term, TermKey};
use std::collections::{BTreeMap, BTreeSet};
fn command(sim: &mut Simulation, c: Command) {
    sim.submit(Submission {
        caller_sequence: sim.next_caller_sequence().unwrap(),
        command: c,
    })
    .unwrap();
}
fn actor(sim: &Simulation, n: usize) -> PersonId {
    sim.canonical_state().state.people[n].id
}
fn travel(sim: &mut Simulation, p: PersonId, to: AreaId) {
    if p == sim.canonical_state().state.ruler {
        command(sim, Command::Move { destination: to });
    } else {
        sim.debug_move_actor(p, to).unwrap();
    }
    let Location::InTransit { arrives, .. } =
        *game_spatial::location(sim.canonical_state().components, game_ecs::StableId((p).0))
            .expect("location exists")
    else {
        panic!("travel not started")
    };
    sim.advance(SimDuration(arrives.0 - sim.time().0)).unwrap();
}
struct Context {
    a: InstitutionId,
    b: InstitutionId,
    mayor: PersonId,
    av: PersonId,
    bv: PersonId,
    food_a: InventoryId,
    food_b: InventoryId,
    timber_a: InventoryId,
    timber_b: InventoryId,
}
fn fixture(seed: u64) -> (Simulation, Context) {
    let mut sim = Simulation::realm_fixture(seed, Execution::default()).unwrap();
    let mayor = actor(&sim, 4);
    let av = actor(&sim, 3);
    let bv = actor(&sim, 5);
    let mut disposition = game_assembly::decisions::Personality::restful();
    disposition.honesty = 100;
    disposition.discipline = 100;
    sim.debug_set_personality(
        mayor,
        disposition,
        game_assembly::decisions::Aspiration::Comfort,
    )
    .unwrap();
    let institutions = sim.canonical_state().state.institutions.as_ref().unwrap();
    let a = institutions
        .institutions
        .values()
        .find(|i| i.charter.purpose == "Willowbank trading council")
        .unwrap()
        .id;
    let b = institutions
        .institutions
        .values()
        .find(|i| i.charter.purpose == "Northwood trading council")
        .unwrap()
        .id;
    let stock = |institution: InstitutionId, good| {
        *institutions.institutions[&institution].charter.offices[0]
            .inventories
            .iter()
            .find(|id| sim.canonical_state().state.inventories[id].good == good)
            .unwrap()
    };
    let ctx = Context {
        a,
        b,
        mayor,
        av,
        bv,
        food_a: stock(a, Good::Food),
        food_b: stock(b, Good::Food),
        timber_a: stock(a, Good::Timber),
        timber_b: stock(b, Good::Timber),
    };
    let ruler = sim.canonical_state().state.ruler;
    travel(&mut sim, ruler, AreaId(3));
    travel(&mut sim, ruler, AreaId(1));
    (sim, ctx)
}
fn propose(sim: &mut Simulation, c: &Context) -> RealmCommitmentId {
    let ruler = sim.canonical_state().state.ruler;
    let deadline = SimTime(sim.time().0 + 3_600_000);
    let p = Proposal {
        proposer: ruler,
        participants: BTreeMap::from([
            (
                c.a,
                Participant {
                    institution: c.a,
                    settlement: AreaId(1),
                    representative: ruler,
                },
            ),
            (
                c.b,
                Participant {
                    institution: c.b,
                    settlement: AreaId(3),
                    representative: c.mayor,
                },
            ),
        ]),
        ratify_by: SimTime(sim.time().0 + 1_800_000),
        terms: vec![
            Term::Delivery {
                provider: c.a,
                beneficiary: c.b,
                term: AgreementTerm::Delivery {
                    debtor: ruler,
                    creditor: c.mayor,
                    source: c.food_a,
                    destination: c.food_b,
                    quantity: Quantity(1_000),
                    deadline,
                },
            },
            Term::Delivery {
                provider: c.b,
                beneficiary: c.a,
                term: AgreementTerm::Delivery {
                    debtor: c.mayor,
                    creditor: ruler,
                    source: c.timber_b,
                    destination: c.timber_a,
                    quantity: Quantity(1_000),
                    deadline,
                },
            },
        ],
    };
    command(
        sim,
        Command::Realm {
            action: RealmAction::Propose { proposal: p },
        },
    );
    *sim.canonical_state()
        .state
        .realm
        .as_ref()
        .unwrap()
        .kernel
        .commitments
        .keys()
        .last()
        .unwrap()
}
fn report(sim: &mut Simulation, sender: PersonId, to: PersonId, id: RealmCommitmentId) {
    let action = RealmAction::Report {
        commitment: id,
        recipient: to,
    };
    if sender == sim.canonical_state().state.ruler {
        command(sim, Command::Realm { action });
    } else {
        sim.debug_actor_command(sender, Command::Realm { action })
            .unwrap();
    }
    sim.advance(MINUTE).unwrap();
    assert!(sim.canonical_state().events.iter().any(|e|matches!(e.event,EventKind::RealmReported {commitment,sender:s,recipient} if commitment==id && s==sender && recipient==to)));
}
fn ratify(
    sim: &mut Simulation,
    inst: InstitutionId,
    delegate: PersonId,
    voter: PersonId,
    id: RealmCommitmentId,
) {
    let action = InstitutionAction::Propose {
        institution: inst,
        change: InstitutionChange::RatifyRegional {
            commitment: id,
            revision: 0,
            support: true,
        },
    };
    sim.debug_actor_command(delegate, Command::Institution { action })
        .unwrap();
    let proposal = *sim
        .canonical_state()
        .state
        .institutions
        .as_ref()
        .unwrap()
        .proposals
        .keys()
        .last()
        .unwrap();
    for p in [delegate, voter] {
        sim.debug_actor_command(
            p,
            Command::Institution {
                action: InstitutionAction::Vote {
                    proposal,
                    support: true,
                },
            },
        )
        .unwrap();
    }
    assert_eq!(
        sim.canonical_state()
            .state
            .institutions
            .as_ref()
            .unwrap()
            .proposals[&proposal]
            .status,
        RuleProposalStatus::Adopted
    );
}
fn ready(seed: u64) -> (Simulation, Context, RealmCommitmentId, AgreementId) {
    let (mut sim, c) = fixture(seed);
    let ruler = sim.canonical_state().state.ruler;
    let id = propose(&mut sim, &c);
    travel(&mut sim, c.mayor, AreaId(1));
    report(&mut sim, ruler, c.mayor, id);
    let terms = sim
        .canonical_state()
        .state
        .realm
        .as_ref()
        .unwrap()
        .kernel
        .commitments[&id]
        .current()
        .proposal
        .terms
        .iter()
        .map(|t| {
            if let Term::Delivery { term, .. } = t {
                term.clone()
            } else {
                panic!()
            }
        })
        .collect();
    command(
        &mut sim,
        Command::Agreement {
            action: AgreementAction::Propose {
                parties: BTreeSet::from([ruler, c.mayor]),
                terms,
                procedure: None,
            },
        },
    );
    let agreement = *sim
        .canonical_state()
        .state
        .agreements
        .as_ref()
        .unwrap()
        .agreements
        .keys()
        .last()
        .unwrap();
    sim.debug_actor_command(
        c.mayor,
        Command::Agreement {
            action: AgreementAction::Accept { agreement },
        },
    )
    .unwrap();
    ratify(&mut sim, c.a, ruler, c.av, id);
    travel(&mut sim, c.mayor, AreaId(3));
    ratify(&mut sim, c.b, c.mayor, c.bv, id);
    assert_eq!(
        sim.canonical_state()
            .state
            .realm
            .as_ref()
            .unwrap()
            .kernel
            .commitments[&id]
            .current()
            .status(),
        Status::Ratified
    );
    let before = sim.canonical_snapshot().unwrap();
    assert_eq!(
        sim.debug_actor_command(
            ruler,
            Command::Realm {
                action: RealmAction::RequestWork {
                    key: TermKey {
                        commitment: id,
                        revision: 0,
                        index: 0
                    }
                }
            }
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(sim.canonical_snapshot().unwrap(), before);
    travel(&mut sim, c.bv, AreaId(1));
    report(&mut sim, c.bv, ruler, id);
    command(
        &mut sim,
        Command::Realm {
            action: RealmAction::RequestWork {
                key: TermKey {
                    commitment: id,
                    revision: 0,
                    index: 0,
                },
            },
        },
    );
    sim.debug_actor_command(
        c.mayor,
        Command::Realm {
            action: RealmAction::RequestWork {
                key: TermKey {
                    commitment: id,
                    revision: 0,
                    index: 1,
                },
            },
        },
    )
    .unwrap();
    sim.advance(SimDuration(1)).unwrap();
    (sim, c, id, agreement)
}
#[test]
fn two_settlements_ratify_and_actual_hauling_fulfills_only_the_goods_delivered() {
    let (mut sim, c, id, agreement) = ready(41);
    let ruler = sim.canonical_state().state.ruler;
    let cargo = sim
        .canonical_state()
        .state
        .people
        .iter()
        .find(|p| p.id == ruler)
        .unwrap()
        .inventory;
    assert_eq!(
        sim.canonical_state().state.inventories[&c.food_b].quantity,
        Quantity(0)
    );
    let total = sim.canonical_state().state.inventories[&c.food_a]
        .quantity
        .0
        + sim.canonical_state().state.inventories[&c.food_b]
            .quantity
            .0
        + sim.canonical_state().state.inventories[&cargo].quantity.0;
    command(
        &mut sim,
        Command::Load {
            source: c.food_a,
            destination: cargo,
            quantity: Quantity(1_000),
        },
    );
    sim.advance(MINUTE).unwrap();
    travel(&mut sim, ruler, AreaId(3));
    assert_eq!(
        sim.canonical_state().state.inventories[&c.food_b].quantity,
        Quantity(0)
    );
    command(
        &mut sim,
        Command::Load {
            source: cargo,
            destination: c.food_b,
            quantity: Quantity(1_000),
        },
    );
    sim.advance(MINUTE).unwrap();
    assert_eq!(
        sim.canonical_state().state.inventories[&c.food_a]
            .quantity
            .0
            + sim.canonical_state().state.inventories[&c.food_b]
                .quantity
                .0
            + sim.canonical_state().state.inventories[&cargo].quantity.0,
        total
    );
    let r = &sim
        .canonical_state()
        .state
        .realm
        .as_ref()
        .unwrap()
        .kernel
        .commitments[&id]
        .revisions[0];
    assert_eq!(r.backed[&0].performed, Quantity(1_000));
    assert_eq!(r.backed[&0].progress, Progress::Fulfilled);
    assert_eq!(r.backed[&1].performed, Quantity(0));
    assert_ne!(r.status(), Status::Fulfilled);
    let debtor = sim
        .canonical_state()
        .state
        .agreements
        .as_ref()
        .unwrap()
        .agreements[&agreement]
        .obligations
        .iter()
        .find(|o| {
            sim.canonical_state()
                .state
                .agreements
                .as_ref()
                .unwrap()
                .obligations[o]
                .term
                .parties()[0]
                == c.mayor
        })
        .copied()
        .unwrap();
    travel(&mut sim, ruler, AreaId(1));
    let due = sim
        .canonical_state()
        .state
        .agreements
        .as_ref()
        .unwrap()
        .obligations[&debtor]
        .term
        .deadline()
        .unwrap();
    let saved = sim.save().unwrap();
    let mut parallel = Simulation::load(
        &saved,
        Execution {
            workers: 4,
            reverse_jobs: true,
        },
    )
    .unwrap();
    let duration = due.0 - sim.time().0;
    sim.advance(SimDuration(duration)).unwrap();
    let first = duration / 3;
    parallel.advance(SimDuration(first)).unwrap();
    parallel.advance(SimDuration(duration - first)).unwrap();
    assert_eq!(sim.canonical_state(), parallel.canonical_state());
    assert_eq!(
        sim.canonical_state()
            .state
            .realm
            .as_ref()
            .unwrap()
            .kernel
            .commitments[&id]
            .revisions[0]
            .backed[&1]
            .progress,
        Progress::Defaulted
    );
    assert!(
        sim.canonical_state()
            .state
            .agreements
            .as_ref()
            .unwrap()
            .obligations[&debtor]
            .defaulted
    );
    assert_eq!(
        sim.canonical_state()
            .state
            .agreements
            .as_ref()
            .unwrap()
            .obligations[&debtor]
            .performed,
        0
    );
    let known = sim
        .observe()
        .realm
        .commitments
        .into_iter()
        .find(|k| k.id == id)
        .unwrap();
    assert!(
        known.knowledge.revisions[&0]
            .backed
            .get(&1)
            .is_none_or(|b| b.value.progress != Progress::Defaulted)
    );
    travel(&mut sim, c.mayor, AreaId(1));
    report(&mut sim, c.mayor, ruler, id);
    let known = sim
        .observe()
        .realm
        .commitments
        .into_iter()
        .find(|k| k.id == id)
        .unwrap();
    assert_eq!(
        known.knowledge.revisions[&0].backed[&1].value.progress,
        Progress::Defaulted
    );
    assert_eq!(
        known.knowledge.revisions[&0].backed[&1].source,
        KnowledgeSource::Reported(c.mayor)
    );
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
}
#[test]
fn a_completed_report_survives_minute_review_but_a_moving_listener_cancels_it() {
    let (mut sim, c) = fixture(42);
    let ruler = sim.canonical_state().state.ruler;
    let id = propose(&mut sim, &c);
    travel(&mut sim, c.mayor, AreaId(1));
    command(
        &mut sim,
        Command::Realm {
            action: RealmAction::Report {
                commitment: id,
                recipient: c.mayor,
            },
        },
    );
    let before = sim.canonical_snapshot().unwrap();
    assert_eq!(
        sim.debug_actor_command(
            c.av,
            Command::Realm {
                action: RealmAction::Report {
                    commitment: id,
                    recipient: c.mayor
                }
            }
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(sim.canonical_snapshot().unwrap(), before);
    sim.advance(MINUTE).unwrap();
    assert!(sim.canonical_state().events.iter().any(|e|matches!(e.event,EventKind::RealmReported {sender,recipient,..} if sender==ruler && recipient==c.mayor)));
    let reports = sim
        .canonical_state()
        .events
        .iter()
        .filter(|e| matches!(e.event, EventKind::RealmReported { .. }))
        .count();
    command(
        &mut sim,
        Command::Realm {
            action: RealmAction::Report {
                commitment: id,
                recipient: c.mayor,
            },
        },
    );
    travel(&mut sim, c.mayor, AreaId(3));
    assert_eq!(
        sim.canonical_state()
            .events
            .iter()
            .filter(|e| matches!(e.event, EventKind::RealmReported { .. }))
            .count(),
        reports
    );
    assert!(
        sim.canonical_state()
            .state
            .realm
            .as_ref()
            .unwrap()
            .talks
            .is_empty()
    );
    assert_eq!(
        *sim.canonical_state()
            .components
            .activity(ruler)
            .expect("body exists"),
        Activity::Idle
    );
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
}

#[test]
fn institutional_ratification_does_not_accept_personal_terms_and_a_real_refusal_remains() {
    let (mut sim, c) = fixture(43);
    let ruler = sim.canonical_state().state.ruler;
    let id = propose(&mut sim, &c);
    travel(&mut sim, c.mayor, AreaId(1));
    report(&mut sim, ruler, c.mayor, id);
    ratify(&mut sim, c.a, ruler, c.av, id);
    travel(&mut sim, c.mayor, AreaId(3));
    ratify(&mut sim, c.b, c.mayor, c.bv, id);
    travel(&mut sim, c.bv, AreaId(1));
    report(&mut sim, c.bv, ruler, id);
    travel(&mut sim, c.mayor, AreaId(1));
    command(
        &mut sim,
        Command::Realm {
            action: RealmAction::RequestWork {
                key: TermKey {
                    commitment: id,
                    revision: 0,
                    index: 0,
                },
            },
        },
    );
    sim.advance(SimDuration(1)).unwrap();
    let agreement = match sim.canonical_state().state.realm.as_ref().unwrap().pending[&TermKey {
        commitment: id,
        revision: 0,
        index: 0,
    }]
        .owner
    {
        game_assembly::realm::PendingOwner::Agreement(id) => id,
        _ => panic!("expected the actual proposed agreement"),
    };
    assert_eq!(
        sim.canonical_state()
            .state
            .agreements
            .as_ref()
            .unwrap()
            .agreements[&agreement]
            .status,
        AgreementStatus::Proposed
    );
    assert!(
        sim.canonical_state()
            .state
            .agreements
            .as_ref()
            .unwrap()
            .agreements[&agreement]
            .obligations
            .is_empty()
    );
    assert_eq!(
        sim.canonical_state()
            .state
            .realm
            .as_ref()
            .unwrap()
            .kernel
            .commitments[&id]
            .current()
            .backed[&0]
            .progress,
        Progress::AwaitingWork
    );
    let stock = sim.canonical_state().state.inventories[&c.food_a].quantity;
    sim.debug_actor_command(
        c.mayor,
        Command::Agreement {
            action: AgreementAction::Reject { agreement },
        },
    )
    .unwrap();
    sim.advance(SimDuration(1)).unwrap();
    assert_eq!(
        sim.canonical_state()
            .state
            .realm
            .as_ref()
            .unwrap()
            .kernel
            .commitments[&id]
            .current()
            .backed[&0]
            .progress,
        Progress::Refused
    );
    assert_eq!(
        sim.canonical_state().state.inventories[&c.food_a].quantity,
        stock
    );
    assert_eq!(
        sim.canonical_state().state.inventories[&c.food_b].quantity,
        Quantity(0)
    );
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
}

#[test]
fn a_willing_patrol_can_be_retasked_without_freezing_the_world_or_fulfilling_its_old_promise() {
    let (mut sim, c) = fixture(44);
    let ruler = sim.canonical_state().state.ruler;
    let until = SimTime(sim.time().0 + 3_600_000);
    let ratify_by = SimTime(sim.time().0 + 1_800_000);
    let patrol = MissionProposal {
        leader: c.av,
        recruits: BTreeSet::new(),
        objective: MissionObjective::Patrol {
            areas: BTreeSet::from([AreaId(1)]),
            until,
        },
        home: AreaId(1),
        supplies: vec![],
        compensation: BTreeMap::new(),
        deadline: SimTime(until.0 + 600_000),
    };
    command(
        &mut sim,
        Command::Realm {
            action: RealmAction::Propose {
                proposal: Proposal {
                    proposer: ruler,
                    participants: BTreeMap::from([
                        (
                            c.a,
                            Participant {
                                institution: c.a,
                                settlement: AreaId(1),
                                representative: ruler,
                            },
                        ),
                        (
                            c.b,
                            Participant {
                                institution: c.b,
                                settlement: AreaId(3),
                                representative: c.mayor,
                            },
                        ),
                    ]),
                    terms: vec![Term::Protection {
                        provider: c.a,
                        beneficiary: c.b,
                        patrol,
                    }],
                    ratify_by,
                },
            },
        },
    );
    let id = *sim
        .canonical_state()
        .state
        .realm
        .as_ref()
        .unwrap()
        .kernel
        .commitments
        .keys()
        .last()
        .unwrap();
    travel(&mut sim, c.mayor, AreaId(1));
    report(&mut sim, ruler, c.mayor, id);
    ratify(&mut sim, c.a, ruler, c.av, id);
    travel(&mut sim, c.mayor, AreaId(3));
    ratify(&mut sim, c.b, c.mayor, c.bv, id);
    travel(&mut sim, c.bv, AreaId(1));
    report(&mut sim, c.bv, ruler, id);
    command(
        &mut sim,
        Command::Realm {
            action: RealmAction::RequestWork {
                key: TermKey {
                    commitment: id,
                    revision: 0,
                    index: 0,
                },
            },
        },
    );
    let mission = *sim
        .canonical_state()
        .state
        .missions
        .as_ref()
        .unwrap()
        .missions
        .keys()
        .last()
        .unwrap();
    let recruitment = sim
        .canonical_state()
        .state
        .missions
        .as_ref()
        .unwrap()
        .missions[&mission]
        .members[&c.av]
        .recruitment;
    sim.debug_actor_command(
        c.av,
        Command::Mission {
            action: MissionAction::Accept { recruitment },
        },
    )
    .unwrap();
    sim.advance(SimDuration(1)).unwrap();
    assert_eq!(
        sim.canonical_state()
            .state
            .realm
            .as_ref()
            .unwrap()
            .kernel
            .commitments[&id]
            .current()
            .backed[&0]
            .progress,
        Progress::Active
    );
    command(
        &mut sim,
        Command::Mission {
            action: MissionAction::Objective {
                mission,
                objective: MissionObjective::Visit {
                    destination: AreaId(2),
                },
            },
        },
    );
    sim.advance(MINUTE).unwrap();
    assert_eq!(
        sim.canonical_state()
            .state
            .realm
            .as_ref()
            .unwrap()
            .kernel
            .commitments[&id]
            .current()
            .backed[&0]
            .progress,
        Progress::Defaulted
    );
    assert_eq!(
        sim.canonical_state()
            .state
            .missions
            .as_ref()
            .unwrap()
            .missions[&mission]
            .proposal
            .objective,
        MissionObjective::Visit {
            destination: AreaId(2)
        }
    );
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    sim.advance(MINUTE).unwrap();
}
