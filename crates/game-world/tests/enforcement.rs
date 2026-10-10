use game_api::*;
use game_world::{Execution, InventoryLocation, Simulation};
use std::collections::BTreeSet;
fn actor(sim: &Simulation, index: usize) -> PersonId {
    sim.canonical_state().people[index].id
}
fn court(sim: &Simulation) -> InstitutionId {
    *sim.canonical_state()
        .institutions
        .as_ref()
        .unwrap()
        .institutions
        .keys()
        .next()
        .unwrap()
}
fn role(sim: &Simulation) -> PersonId {
    sim.canonical_state().ruler
}
fn travel(sim: &mut Simulation, who: PersonId, destination: AreaId) {
    sim.debug_move_actor(who, destination).unwrap();
    let Location::InTransit { arrives, .. } = sim
        .canonical_state()
        .people
        .iter()
        .find(|p| p.id == who)
        .unwrap()
        .location
    else {
        panic!("traveler did not depart")
    };
    sim.advance(SimDuration(arrives.0 - sim.canonical_state().time.0))
        .unwrap();
}
fn theft(sim: &mut Simulation) -> IncidentId {
    let filer = actor(sim, 3);
    let target = actor(sim, 5);
    travel(sim, filer, AreaId(3));
    travel(sim, target, AreaId(3));
    let source = sim
        .canonical_state()
        .inventories
        .values()
        .find(|i| {
            i.good == Good::Food
                && i.location == InventoryLocation::Area(AreaId(3))
                && i.access == BTreeSet::from([filer])
        })
        .unwrap()
        .id;
    let destination = sim
        .canonical_state()
        .people
        .iter()
        .find(|p| p.id == target)
        .unwrap()
        .inventory;
    sim.debug_property_action(
        target,
        PropertyAction::Take {
            source,
            destination,
            quantity: Quantity(1_000),
        },
    )
    .unwrap();
    sim.advance(MINUTE).unwrap();
    sim.property_view(filer).incidents.last().unwrap().id
}
fn file(sim: &mut Simulation, incident: IncidentId) -> EnforcementCaseId {
    let filer = actor(sim, 3);
    travel(sim, filer, AreaId(1));
    sim.debug_enforcement_action(
        filer,
        EnforcementAction::File {
            institution: court(sim),
            office: 0,
            evidence: EnforcementEvidenceRef::Incident(incident),
        },
    )
    .unwrap();
    sim.advance(MINUTE).unwrap();
    *sim.canonical_state()
        .enforcement
        .as_ref()
        .unwrap()
        .cases
        .keys()
        .last()
        .unwrap()
}
fn decide(sim: &mut Simulation, case: EnforcementCaseId, remedy: EnforcementRemedy) {
    let judge = role(sim);
    let filer = actor(sim, 3);
    let institution = court(sim);
    sim.debug_institution_action(
        judge,
        InstitutionAction::Propose {
            institution,
            change: InstitutionChange::Adjudicate { case, remedy },
        },
    )
    .unwrap();
    let proposal = *sim
        .canonical_state()
        .institutions
        .as_ref()
        .unwrap()
        .proposals
        .keys()
        .last()
        .unwrap();
    for who in [judge, filer] {
        sim.debug_institution_action(
            who,
            InstitutionAction::Vote {
                proposal,
                support: true,
            },
        )
        .unwrap();
    }
}
#[test]
fn unreported_distant_taking_does_not_summon_officers_and_unknown_evidence_is_inert() {
    let mut sim = Simulation::enforcement_fixture(31, Execution::default()).unwrap();
    let judge = role(&sim);
    let filer = actor(&sim, 3);
    let before = sim.canonical_state().clone();
    assert_eq!(
        sim.debug_enforcement_action(
            filer,
            EnforcementAction::File {
                institution: court(&sim),
                office: 0,
                evidence: EnforcementEvidenceRef::Incident(IncidentId(999_999))
            }
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(sim.canonical_state(), &before);
    let incident = theft(&mut sim);
    assert!(
        sim.canonical_state()
            .enforcement
            .as_ref()
            .unwrap()
            .cases
            .is_empty()
    );
    assert!(
        sim.canonical_state()
            .missions
            .as_ref()
            .unwrap()
            .missions
            .is_empty()
    );
    assert!(
        !sim.property_view(judge)
            .incidents
            .iter()
            .any(|i| i.id == incident)
    );
    let case = file(&mut sim, incident);
    assert_eq!(
        sim.canonical_state().enforcement.as_ref().unwrap().known[&judge][&case].status,
        EnforcementStatus::Pending
    );
    assert!(
        sim.canonical_state().enforcement.as_ref().unwrap().known[&judge][&case]
            .contact
            .as_ref()
            .is_some_and(|c| c.area == AreaId(3))
    );
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
fn an_officer_can_refuse_a_funded_order_without_physical_capture_or_spending() {
    let mut sim = Simulation::enforcement_fixture(32, Execution::default()).unwrap();
    let incident = theft(&mut sim);
    let case = file(&mut sim, incident);
    let until = SimTime(sim.canonical_state().time.0 + 600_000);
    decide(&mut sim, case, EnforcementRemedy::Confine { until });
    let officer = actor(&sim, 4);
    let judge = role(&sim);
    let mut personality = game_world::decisions::Personality::restful();
    personality.risk_tolerance = 0;
    personality.empathy = 0;
    personality.aggression = 0;
    sim.debug_set_personality(
        officer,
        personality,
        game_world::decisions::Aspiration::Comfort,
    )
    .unwrap();
    sim.debug_set_relationship(
        officer,
        judge,
        game_world::decisions::Relationship {
            trust: -1_000,
            affection: -1_000,
        },
    )
    .unwrap();
    let proposal = sim
        .observe()
        .available_commands
        .into_iter()
        .find_map(|command| {
            if let Command::Enforcement {
                action: EnforcementAction::Dispatch { case: id, proposal },
            } = command
            {
                if id == case { Some(proposal) } else { None }
            } else {
                None
            }
        })
        .unwrap();
    let source = proposal.supplies[0].source;
    let quantity = sim.canonical_state().inventories[&source].quantity;
    sim.debug_enforcement_action(judge, EnforcementAction::Dispatch { case, proposal })
        .unwrap();
    sim.advance(MINUTE).unwrap();
    let mission = sim.canonical_state().enforcement.as_ref().unwrap().cases[&case]
        .mission
        .unwrap();
    assert!(
        sim.canonical_state().missions.as_ref().unwrap().missions[&mission]
            .members
            .values()
            .all(|p| p.status == CompanionStatus::Refused)
    );
    assert!(
        sim.canonical_state()
            .combat
            .as_ref()
            .unwrap()
            .captivity
            .is_empty()
    );
    assert_eq!(
        sim.canonical_state().inventories[&source].quantity,
        quantity
    );
}
#[test]
fn removing_a_political_office_preserves_personal_control_assets_and_existing_debt() {
    let mut sim = Simulation::enforcement_fixture(33, Execution::default()).unwrap();
    let judge = role(&sim);
    let filer = actor(&sim, 3);
    let institution = court(&sim);
    let source = sim
        .canonical_state()
        .people
        .iter()
        .find(|p| p.id == judge)
        .unwrap()
        .inventory;
    let destination = sim
        .canonical_state()
        .people
        .iter()
        .find(|p| p.id == filer)
        .unwrap()
        .inventory;
    sim.debug_actor_command(
        judge,
        Command::Agreement {
            action: AgreementAction::Propose {
                parties: BTreeSet::from([judge, filer]),
                terms: vec![AgreementTerm::Delivery {
                    debtor: judge,
                    creditor: filer,
                    source,
                    destination,
                    quantity: Quantity(1_000),
                    deadline: SimTime(60_000),
                }],
                procedure: None,
            },
        },
    )
    .unwrap();
    let agreement = *sim
        .canonical_state()
        .agreements
        .as_ref()
        .unwrap()
        .agreements
        .keys()
        .last()
        .unwrap();
    sim.debug_actor_command(
        filer,
        Command::Agreement {
            action: AgreementAction::Accept { agreement },
        },
    )
    .unwrap();
    sim.advance(MINUTE).unwrap();
    let obligation = sim
        .canonical_state()
        .agreements
        .as_ref()
        .unwrap()
        .agreements[&agreement]
        .obligations[0];
    sim.debug_enforcement_action(
        filer,
        EnforcementAction::File {
            institution,
            office: 0,
            evidence: EnforcementEvidenceRef::Default(obligation),
        },
    )
    .unwrap();
    sim.advance(MINUTE).unwrap();
    let case = *sim
        .canonical_state()
        .enforcement
        .as_ref()
        .unwrap()
        .cases
        .keys()
        .last()
        .unwrap();
    let assets = sim.canonical_state().inventories[&source].quantity;
    decide(
        &mut sim,
        case,
        EnforcementRemedy::Depose {
            office: 0,
            replacement: filer,
        },
    );
    assert_eq!(sim.canonical_state().ruler, judge);
    assert_eq!(
        sim.canonical_state()
            .institutions
            .as_ref()
            .unwrap()
            .institutions[&institution]
            .charter
            .offices[0]
            .holder,
        filer
    );
    assert_eq!(sim.canonical_state().inventories[&source].quantity, assets);
    assert_eq!(
        sim.canonical_state()
            .agreements
            .as_ref()
            .unwrap()
            .obligations[&obligation]
            .performed,
        0
    );
    assert!(
        sim.canonical_state()
            .agreements
            .as_ref()
            .unwrap()
            .obligations[&obligation]
            .defaulted
    );
    sim.submit(Submission {
        caller_sequence: 0,
        command: Command::Rest,
    })
    .unwrap();
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
}

fn dispatch_proposal(sim: &mut Simulation, case: EnforcementCaseId) -> MissionProposal {
    sim.observe()
        .available_commands
        .into_iter()
        .find_map(|c| match c {
            Command::Enforcement {
                action: EnforcementAction::Dispatch { case: id, proposal },
            } if id == case => Some(proposal),
            _ => None,
        })
        .expect("a known adopted confinement offers a physical expedition")
}

#[test]
fn chosen_funding_caps_reject_an_order_before_creating_promises_or_recruitment() {
    let mut sim = Simulation::enforcement_fixture(34, Execution::default()).unwrap();
    let incident = theft(&mut sim);
    let case = file(&mut sim, incident);
    let until = SimTime(sim.canonical_state().time.0 + 600_000);
    decide(&mut sim, case, EnforcementRemedy::Confine { until });
    let judge = role(&sim);
    let mut proposal = dispatch_proposal(&mut sim, case);
    proposal.supplies[0].quantity = Quantity(u64::MAX);
    let before = sim.canonical_state().clone();
    assert_eq!(
        sim.debug_enforcement_action(judge, EnforcementAction::Dispatch { case, proposal }),
        Err(Error::Unavailable)
    );
    assert_eq!(sim.canonical_state(), &before);
}

#[test]
fn a_willing_funded_officer_uses_actual_supplies_travel_binding_and_release() {
    let mut sim = Simulation::enforcement_fixture(35, Execution::default()).unwrap();
    let incident = theft(&mut sim);
    let case = file(&mut sim, incident);
    let until = SimTime(sim.canonical_state().time.0 + 1_200_000);
    decide(&mut sim, case, EnforcementRemedy::Confine { until });
    let judge = role(&sim);
    let officer = actor(&sim, 4);
    let target = actor(&sim, 5);
    let mut opponent = game_world::decisions::Personality::restful();
    opponent.aggression = 900;
    opponent.risk_tolerance = 900;
    sim.debug_set_personality(target, opponent, game_world::decisions::Aspiration::Comfort)
        .unwrap();
    let proposal = dispatch_proposal(&mut sim, case);
    let binding_source = proposal
        .supplies
        .iter()
        .find(|s| sim.canonical_state().inventories[&s.source].good == Good::Binding)
        .unwrap()
        .source;
    let original_bindings = sim.canonical_state().inventories[&binding_source].quantity;
    let payment = proposal.compensation[&officer][0].clone();
    sim.debug_enforcement_action(judge, EnforcementAction::Dispatch { case, proposal })
        .unwrap();
    assert!(
        sim.canonical_state()
            .combat
            .as_ref()
            .unwrap()
            .captivity
            .is_empty()
    );
    assert_eq!(
        sim.canonical_state()
            .people
            .iter()
            .find(|p| p.id == officer)
            .unwrap()
            .location,
        Location::AtArea(AreaId(1))
    );
    sim.advance(MINUTE).unwrap();
    let mission = sim.canonical_state().enforcement.as_ref().unwrap().cases[&case]
        .mission
        .unwrap();
    assert_eq!(
        sim.canonical_state().missions.as_ref().unwrap().missions[&mission].members[&officer]
            .status,
        CompanionStatus::Accepted
    );
    let AgreementTerm::Delivery {
        source,
        destination,
        quantity,
        ..
    } = payment
    else {
        panic!("fixture wages are a delivery")
    };
    let cargo = sim
        .canonical_state()
        .people
        .iter()
        .find(|p| p.id == judge)
        .unwrap()
        .inventory;
    for (source, destination) in [(source, cargo), (cargo, destination)] {
        sim.debug_actor_command(
            judge,
            Command::Load {
                source,
                destination,
                quantity,
            },
        )
        .unwrap_or_else(|e| panic!("payment {source:?}->{destination:?}: {e:?}"));
        sim.advance(MINUTE).unwrap();
    }
    for _ in 0..15 {
        let location = sim
            .canonical_state()
            .people
            .iter()
            .find(|p| p.id == officer)
            .unwrap()
            .location
            .clone();
        if let Location::InTransit {
            to: AreaId(3),
            arrives,
            ..
        } = location
        {
            sim.advance(SimDuration(arrives.0 - sim.canonical_state().time.0 - 1))
                .unwrap();
            sim.debug_injure(target, 120_000).unwrap();
            assert!(
                sim.canonical_state()
                    .components
                    .body(target)
                    .expect("body exists")
                    .vitals
                    .health
                    > sim
                        .canonical_state()
                        .combat
                        .as_ref()
                        .unwrap()
                        .rules
                        .incapacitation
            );
            sim.advance(SimDuration(1)).unwrap();
            break;
        }
        sim.advance(MINUTE).unwrap();
    }
    for _ in 0..15 {
        if sim
            .canonical_state()
            .combat
            .as_ref()
            .unwrap()
            .captivity
            .contains_key(&target)
        {
            break;
        }
        sim.advance(MINUTE).unwrap();
    }
    let captive = sim
        .canonical_state()
        .combat
        .as_ref()
        .unwrap()
        .captivity
        .get(&target)
        .unwrap_or_else(|| {
            panic!(
                "not captured: mission={:#?}, target={:#?}, recent events={:#?}",
                sim.canonical_state().missions.as_ref().unwrap().missions[&mission],
                sim.canonical_state().people.iter().find(|p| p.id == target),
                sim.canonical_state()
                    .events
                    .iter()
                    .rev()
                    .take(30)
                    .collect::<Vec<_>>()
            )
        })
        .clone();
    assert_eq!(captive.captor, officer);
    assert_eq!(captive.area, AreaId(3));
    assert_eq!(
        sim.canonical_state().inventories[&binding_source]
            .quantity
            .0,
        original_bindings.0 - 1_000
    );
    assert_eq!(
        sim.canonical_state().inventories[&captive.binding].quantity,
        Quantity(1_000)
    );
    assert!(sim.canonical_state().events.iter().any(
        |e| matches!(e.kind,EventKind::Captured {actor,target:t,..} if actor==officer && t==target)
    ));
    let remaining = until.0 - sim.canonical_state().time.0;
    sim.advance(SimDuration(remaining - 1)).unwrap();
    assert!(
        sim.canonical_state()
            .combat
            .as_ref()
            .unwrap()
            .captivity
            .contains_key(&target)
    );
    let mut restored = Simulation::load(
        &sim.save().unwrap(),
        Execution {
            workers: 4,
            reverse_jobs: true,
        },
    )
    .unwrap();
    assert_eq!(restored.canonical_state(), sim.canonical_state());
    sim.debug_move_actor(judge, AreaId(2)).unwrap();
    restored.debug_move_actor(judge, AreaId(2)).unwrap();
    for _ in 0..8 {
        sim.advance(MINUTE).unwrap();
        for duration in [7_000, 23_000, 30_000] {
            restored.advance(SimDuration(duration)).unwrap();
        }
        assert_eq!(restored.canonical_state(), sim.canonical_state());
        if !sim
            .canonical_state()
            .combat
            .as_ref()
            .unwrap()
            .captivity
            .contains_key(&target)
        {
            break;
        }
    }
    assert!(
        !sim.canonical_state()
            .combat
            .as_ref()
            .unwrap()
            .captivity
            .contains_key(&target)
    );
    assert!(sim.canonical_state().events.iter().any(|e| e.time >= until
        && matches!(e.kind,EventKind::Released {actor,target:t} if actor==officer && t==target)));
    assert_eq!(
        sim.canonical_state().missions.as_ref().unwrap().missions[&mission].status,
        MissionStatus::Returning
    );
    sim.advance(SimDuration(300_000)).unwrap();
    assert_eq!(
        sim.canonical_state().enforcement.as_ref().unwrap().cases[&case].status,
        EnforcementStatus::Completed
    );
    assert_ne!(
        sim.canonical_state().enforcement.as_ref().unwrap().known[&judge][&case].status,
        EnforcementStatus::Completed
    );
    travel(&mut sim, officer, AreaId(2));
    sim.debug_enforcement_action(
        officer,
        EnforcementAction::Report {
            case,
            recipient: judge,
        },
    )
    .unwrap();
    sim.advance(MINUTE).unwrap();
    assert_eq!(
        sim.canonical_state().enforcement.as_ref().unwrap().known[&judge][&case].status,
        EnforcementStatus::Completed
    );
    assert_eq!(
        sim.canonical_state().enforcement.as_ref().unwrap().known[&judge][&case].source,
        KnowledgeSource::Reported(officer)
    );
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
}

#[test]
fn command_time_assault_is_seen_locally_and_arriving_later_does_not_create_a_witness() {
    let mut sim = Simulation::enforcement_fixture(38, Execution::default()).unwrap();
    let remote = actor(&sim, 6);
    let witness = actor(&sim, 3);
    let target = actor(&sim, 5);
    let judge = role(&sim);
    travel(&mut sim, remote, AreaId(2));
    sim.submit(Submission {
        caller_sequence: 0,
        command: Command::Engage { target },
    })
    .unwrap();
    let encounter = sim.observe().combat.unwrap().encounter;
    for _ in 0..3 {
        sim.submit(Submission {
            caller_sequence: sim.next_caller_sequence().unwrap(),
            command: Command::CombatTurn {
                encounter,
                movement: None,
                action: CombatAction::Attack { target },
            },
        })
        .unwrap();
        let strike = sim
            .canonical_state()
            .events
            .iter()
            .rev()
            .find(|e| matches!(e.kind,EventKind::Struck {actor,..} if actor==judge))
            .unwrap()
            .id;
        assert!(
            sim.canonical_state().enforcement.as_ref().unwrap().assaults[&witness]
                .contains_key(&strike)
        );
        assert!(
            !sim.canonical_state()
                .enforcement
                .as_ref()
                .unwrap()
                .assaults
                .get(&remote)
                .is_some_and(|k| k.contains_key(&strike))
        );
        sim.advance(SimDuration(6_000)).unwrap();
    }
    travel(&mut sim, remote, AreaId(1));
    assert!(
        sim.canonical_state()
            .enforcement
            .as_ref()
            .unwrap()
            .assaults
            .get(&remote)
            .is_none_or(|k| k.is_empty())
    );
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
}
