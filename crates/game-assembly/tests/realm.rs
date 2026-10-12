mod support;
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
    let a = game_society::institutions(sim.canonical_state().components)
        .map(|(_, row)| row)
        .find(|i| i.charter.purpose == "Willowbank trading council")
        .unwrap()
        .id;
    let b = game_society::institutions(sim.canonical_state().components)
        .map(|(_, row)| row)
        .find(|i| i.charter.purpose == "Northwood trading council")
        .unwrap()
        .id;
    let stock = |institution: InstitutionId, good| {
        *game_society::institution(sim.canonical_state().components, institution)
            .unwrap()
            .charter
            .offices[0]
            .inventories
            .iter()
            .find(|id| {
                game_cargo::inventory(sim.canonical_state().components, **id)
                    .expect("stock exists")
                    .good
                    == good
            })
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
    game_realm::commitments(sim.canonical_state().components)
        .last()
        .unwrap()
        .0
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
    let proposal = game_society::proposals(sim.canonical_state().components)
        .map(|(id, _)| id)
        .next_back()
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
        game_society::proposal(sim.canonical_state().components, proposal)
            .unwrap()
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
    let terms = game_realm::commitment(sim.canonical_state().components, id)
        .unwrap()
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
    let agreement = game_agreements::agreements(sim.canonical_state().components)
        .map(|(id, _)| id)
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
        game_realm::commitment(sim.canonical_state().components, id)
            .unwrap()
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
        game_cargo::inventory(sim.canonical_state().components, c.food_b)
            .expect("stock exists")
            .quantity,
        Quantity(0)
    );
    let total = game_cargo::inventory(sim.canonical_state().components, c.food_a)
        .expect("stock exists")
        .quantity
        .0
        + game_cargo::inventory(sim.canonical_state().components, c.food_b)
            .expect("stock exists")
            .quantity
            .0
        + game_cargo::inventory(sim.canonical_state().components, cargo)
            .expect("stock exists")
            .quantity
            .0;
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
        game_cargo::inventory(sim.canonical_state().components, c.food_b)
            .expect("stock exists")
            .quantity,
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
        game_cargo::inventory(sim.canonical_state().components, c.food_a)
            .expect("stock exists")
            .quantity
            .0
            + game_cargo::inventory(sim.canonical_state().components, c.food_b)
                .expect("stock exists")
                .quantity
                .0
            + game_cargo::inventory(sim.canonical_state().components, cargo)
                .expect("stock exists")
                .quantity
                .0,
        total
    );
    let r = &game_realm::commitment(sim.canonical_state().components, id)
        .unwrap()
        .revisions[0];
    assert_eq!(r.backed[&0].performed, Quantity(1_000));
    assert_eq!(r.backed[&0].progress, Progress::Fulfilled);
    assert_eq!(r.backed[&1].performed, Quantity(0));
    assert_ne!(r.status(), Status::Fulfilled);
    let debtor = game_agreements::agreement(sim.canonical_state().components, agreement)
        .unwrap()
        .obligations
        .iter()
        .find(|o| {
            game_agreements::obligation(sim.canonical_state().components, **o)
                .unwrap()
                .term
                .parties()[0]
                == c.mayor
        })
        .copied()
        .unwrap();
    travel(&mut sim, ruler, AreaId(1));
    let due = game_agreements::obligation(sim.canonical_state().components, debtor)
        .unwrap()
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
        game_realm::commitment(sim.canonical_state().components, id)
            .unwrap()
            .revisions[0]
            .backed[&1]
            .progress,
        Progress::Defaulted
    );
    assert!(
        game_agreements::obligation(sim.canonical_state().components, debtor)
            .unwrap()
            .defaulted
    );
    assert_eq!(
        game_agreements::obligation(sim.canonical_state().components, debtor)
            .unwrap()
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
        game_realm::talks(sim.canonical_state().components)
            .collect::<Vec<_>>()
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
    let agreement = match game_realm::commitment(sim.canonical_state().components, id)
        .unwrap()
        .revisions[0]
        .pending[&0]
        .owner
    {
        game_realm::PendingOwner::Agreement(id) => id,
        _ => panic!("expected the actual proposed agreement"),
    };
    assert_eq!(
        game_agreements::agreement(sim.canonical_state().components, agreement)
            .unwrap()
            .status,
        AgreementStatus::Proposed
    );
    assert!(
        game_agreements::agreement(sim.canonical_state().components, agreement)
            .unwrap()
            .obligations
            .is_empty()
    );
    assert_eq!(
        game_realm::commitment(sim.canonical_state().components, id)
            .unwrap()
            .current()
            .backed[&0]
            .progress,
        Progress::AwaitingWork
    );
    let stock = game_cargo::inventory(sim.canonical_state().components, c.food_a)
        .expect("stock exists")
        .quantity;
    sim.debug_actor_command(
        c.mayor,
        Command::Agreement {
            action: AgreementAction::Reject { agreement },
        },
    )
    .unwrap();
    sim.advance(SimDuration(1)).unwrap();
    assert_eq!(
        game_realm::commitment(sim.canonical_state().components, id)
            .unwrap()
            .current()
            .backed[&0]
            .progress,
        Progress::Refused
    );
    assert_eq!(
        game_cargo::inventory(sim.canonical_state().components, c.food_a)
            .expect("stock exists")
            .quantity,
        stock
    );
    assert_eq!(
        game_cargo::inventory(sim.canonical_state().components, c.food_b)
            .expect("stock exists")
            .quantity,
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
    let id = game_realm::commitments(sim.canonical_state().components)
        .last()
        .unwrap()
        .0;
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
    let mission = game_missions::missions(sim.canonical_state().components)
        .map(|(id, _)| id)
        .last()
        .unwrap();
    let recruitment = game_missions::mission(sim.canonical_state().components, mission)
        .unwrap()
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
        game_realm::commitment(sim.canonical_state().components, id)
            .unwrap()
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
        game_realm::commitment(sim.canonical_state().components, id)
            .unwrap()
            .current()
            .backed[&0]
            .progress,
        Progress::Defaulted
    );
    assert_eq!(
        game_missions::mission(sim.canonical_state().components, mission)
            .unwrap()
            .proposal
            .objective,
        MissionObjective::Visit {
            destination: AreaId(2)
        }
    );
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    sim.advance(MINUTE).unwrap();
}

fn pending_report_fixture() -> (Simulation, PersonId, RealmCommitmentId) {
    let (mut sim, c) = fixture(91);
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
    assert!(game_realm::talk(sim.canonical_state().components, ruler).is_some());
    (sim, ruler, id)
}

#[test]
fn native_realm_and_stocks_save51_have_one_authority_and_pending_report_replays() {
    let (mut reference, ruler, id) = pending_report_fixture();
    let bytes = reference.save().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["version"], 51);
    let columns = &json["state"]["components"]["columns"];
    let realm_keys = columns
        .as_object()
        .unwrap()
        .keys()
        .filter(|key| key.starts_with("realm/"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        realm_keys,
        [
            "realm/commitment",
            "realm/knowledge",
            "realm/requests",
            "realm/talk"
        ]
    );
    assert_eq!(columns["realm/commitment"].as_array().unwrap().len(), 1);
    assert_eq!(columns["realm/talk"].as_array().unwrap().len(), 1);
    let runtime_keys = json["state"]["state"]["realm"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    assert_eq!(runtime_keys, ["last_event", "rules"]);
    let people = json["state"]["state"]["people"].as_array().unwrap().len();
    assert_eq!(
        columns["characters/stocks"].as_array().unwrap().len(),
        people
    );
    for row in columns["characters/mind"].as_array().unwrap() {
        assert!(row["value"].get("knowledge").is_none());
    }
    reference.advance(MINUTE).unwrap();
    for workers in [1, 2, 4] {
        let mut restored = Simulation::load(
            &bytes,
            Execution {
                workers,
                reverse_jobs: true,
            },
        )
        .unwrap();
        assert_eq!(restored.save().unwrap(), bytes);
        for duration in [17_123, 19_877, 23_000] {
            restored.advance(SimDuration(duration)).unwrap();
        }
        assert_eq!(restored.save().unwrap(), reference.save().unwrap());
        assert!(game_realm::talk(restored.canonical_state().components, ruler).is_none());
        assert!(game_realm::commitment(restored.canonical_state().components, id).is_some());
    }
}

#[test]
fn native_realm_and_stocks_reject_shadow_orphan_identity_shape_and_future_dates() {
    let (sim, ruler, id) = pending_report_fixture();
    let bytes = sim.save().unwrap();
    let base: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let inventory = sim.canonical_state().state.people[0].inventory.0;
    for failure in 0..18 {
        let mut json = base.clone();
        match failure {
            0 => json["version"] = 49.into(),
            1 => json["version"] = 50.into(),
            2 => json["state"]["state"]["realm"]["kernel"] = serde_json::json!({}),
            3 => {
                support::component_id(&mut json, "realm/commitment", id.0)["id"] = (id.0 + 1).into()
            }
            4 => {
                support::component_id(&mut json, "realm/commitment", id.0)["revisions"] =
                    serde_json::json!([])
            }
            5 => support::component(&mut json, "realm/knowledge", ruler)["unknown"] = true.into(),
            6 => json["state"]["components"]["columns"]["realm/knowledge"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({"entity": inventory, "value": {"commitments": {}}})),
            7 => json["state"]["components"]["columns"]["realm/requests"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({"entity": inventory, "value": {"reconsider_after": 0}})),
            8 => {
                json["state"]["components"]["columns"]["realm/talk"][0]["entity"] = inventory.into()
            }
            9 => {
                support::component(&mut json, "realm/talk", ruler)["completes"] =
                    sim.time().0.into()
            }
            10 => {
                support::component(&mut json, "characters/mind", ruler)["knowledge"] =
                    serde_json::json!({})
            }
            11 => {
                support::component(&mut json, "characters/stocks", ruler)["unknown"] = true.into()
            }
            12 => {
                let fact = support::component(&mut json, "characters/stocks", ruler)["known"]
                    .as_object_mut()
                    .unwrap()
                    .values_mut()
                    .next()
                    .expect("ruler has stock knowledge");
                fact["observed_at"] = (sim.time().0 + 1).into();
                fact["learned_at"] = (sim.time().0 + 1).into();
            }
            13 => json["state"]["components"]["columns"]["characters/stocks"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({"entity": inventory, "value": {"known": {}}})),
            15 => json["state"]["state"]["realm"] = serde_json::Value::Null,
            16 => {
                support::component_id(&mut json, "realm/commitment", id.0)["unknown"] = true.into()
            }
            17 => {
                support::component_id(&mut json, "realm/commitment", id.0)["revisions"][0]["unknown"] =
                    true.into()
            }
            _ => json["state"]["components"]["columns"]["characters/stocks"]
                .as_array_mut()
                .unwrap()
                .retain(|row| row["entity"].as_u64() != Some(ruler.0)),
        }
        assert!(
            Simulation::load(&serde_json::to_vec(&json).unwrap(), Execution::default()).is_err(),
            "native corruption {failure} was accepted"
        );
        assert_eq!(sim.save().unwrap(), bytes);
    }
}

struct NativeReportPresence<'a>(&'a Simulation);
impl game_realm::Presence for NativeReportPresence<'_> {
    type Activity = Activity;
    fn actor_reads(&self, actor: PersonId) -> game_ecs::ReadFootprint {
        let mut reads = game_spatial::location_reads(
            self.0.canonical_state().components,
            game_ecs::StableId(actor.0),
        )
        .unwrap();
        reads.protect::<game_physiology::Body>(game_ecs::StableId(actor.0));
        reads.protect::<Activity>(game_ecs::StableId(actor.0));
        reads.protect::<game_lifecycle::Life>(game_ecs::StableId(actor.0));
        reads
    }
    fn inventory_reads(&self, inventory: InventoryId) -> game_ecs::ReadFootprint {
        let mut reads = game_ecs::ReadFootprint::new();
        reads.protect::<game_cargo::Inventory>(game_ecs::StableId(inventory.0));
        reads
    }
    fn actor(&self, actor: PersonId) -> Option<game_realm::ActorFacts> {
        let state = self.0.canonical_state();
        let body = game_physiology::body(state.components, actor)?;
        Some(game_realm::ActorFacts {
            alive: body.vitals.alive,
            health: body.vitals.health,
            independent: game_lifecycle::life(state.components, actor)
                .is_none_or(|life| life.stage != LifeStage::Infant),
            in_combat: false,
            location: game_spatial::resolve(state.components, game_ecs::StableId(actor.0)).ok()?,
            idle_or_rest: matches!(
                state
                    .components
                    .get::<Activity>(game_ecs::StableId(actor.0)),
                Some(Activity::Idle | Activity::Rest)
            ),
            foreign_talking: false,
            controlled: actor == state.state.ruler,
            past_life_controlled: false,
        })
    }
    fn same_place(&self, a: PersonId, b: PersonId) -> bool {
        let store = self.0.canonical_state().components;
        game_spatial::resolve(store, game_ecs::StableId(a.0)).unwrap()
            == game_spatial::resolve(store, game_ecs::StableId(b.0)).unwrap()
    }
    fn people(&self) -> impl Iterator<Item = PersonId> {
        self.0
            .canonical_state()
            .state
            .people
            .iter()
            .map(|person| person.id)
    }
    fn local_people(&self, _: AreaId) -> impl Iterator<Item = PersonId> {
        std::iter::empty()
    }
    fn inventory_area(&self, _: InventoryId) -> Option<AreaId> {
        None
    }
}
#[test]
fn native_report_completion_late_body_or_activity_edits_fail_without_publication() {
    let (sim, speaker, _) = pending_report_fixture();
    let saved = sim.save().unwrap();
    let state = sim.canonical_state();
    let runtime = state.state.realm.as_ref().unwrap();
    let talk = game_realm::talk(state.components, speaker).unwrap();
    let at = talk.completes;
    let recipient = talk.recipient;
    let presence = NativeReportPresence(&sim);
    for failure in 0..2 {
        let plan = runtime
            .prepare_report_completion(state.components, at, speaker, &presence)
            .unwrap();
        assert!(matches!(
            plan.event,
            game_realm::RealmEvent::Reported { .. }
        ));
        let mut batch = game_ecs::Batch::new();
        let (guard, _) = plan
            .stage_into(state.components, at, &presence, Activity::Idle, &mut batch)
            .unwrap();
        if failure == 0 {
            let mut body = game_physiology::body(state.components, recipient)
                .unwrap()
                .clone();
            body.vitals.alive = false;
            batch.insert(game_ecs::StableId(recipient.0), body);
        } else {
            batch.insert(game_ecs::StableId(recipient.0), Activity::Idle);
        }
        assert!(guard.validate(&batch).is_err());
        assert_eq!(sim.save().unwrap(), saved);
    }
    let plan = runtime
        .prepare_report_completion(state.components, at, speaker, &presence)
        .unwrap();
    let mut batch = game_ecs::Batch::new();
    let (guard, _) = plan
        .stage_into(state.components, at, &presence, Activity::Idle, &mut batch)
        .unwrap();
    let other = state
        .state
        .people
        .iter()
        .find(|p| p.id != recipient && p.id != speaker)
        .unwrap()
        .id;
    batch.insert(
        game_ecs::StableId(other.0),
        game_characters::decisions::stocks(state.components, other)
            .unwrap()
            .clone(),
    );
    guard.validate(&batch).unwrap();
    assert_eq!(sim.save().unwrap(), saved);
}
