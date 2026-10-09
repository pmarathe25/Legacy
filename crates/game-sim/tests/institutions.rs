use game_api::*;
use game_sim::{
    decisions::{Dream, Personality},
    *,
};
use std::collections::BTreeSet;

fn fixture(workers: usize) -> Simulation {
    let mut sim = Simulation::institution_fixture(
        7,
        Execution {
            workers,
            reverse_jobs: true,
        },
    )
    .unwrap();
    let people: Vec<_> = sim.canonical_state().people.iter().map(|p| p.id).collect();
    for actor in people {
        sim.debug_set_personality(actor, Personality::restful(), Dream::Comfort)
            .unwrap();
    }
    let owner = owner(&sim);
    sim.debug_set_personality(
        owner,
        Personality {
            empathy: 900,
            ..Personality::restful()
        },
        Dream::SupportOthers,
    )
    .unwrap();
    sim
}
fn ruler(sim: &Simulation) -> PersonId {
    sim.canonical_state().ruler
}
fn owner(sim: &Simulation) -> PersonId {
    sim.canonical_state().people[3].id
}
fn food(sim: &Simulation) -> InventoryId {
    sim.canonical_state()
        .inventories
        .values()
        .find(|i| i.location == InventoryLocation::Area(AreaId(1)) && i.good == Good::Food)
        .unwrap()
        .id
}
fn site(sim: &Simulation) -> SiteId {
    sim.canonical_state()
        .physical
        .as_ref()
        .unwrap()
        .sites
        .values()
        .find(|s| s.area == AreaId(1))
        .unwrap()
        .id
}
fn command(sim: &mut Simulation, command: Command) -> Result<CommandReceipt, Error> {
    sim.submit(Submission {
        caller_sequence: sim.next_caller_sequence().unwrap(),
        command,
    })
}
fn action(sim: &mut Simulation, action: InstitutionAction) -> Result<CommandReceipt, Error> {
    command(sim, Command::Institution { action })
}
fn basic_rule(sim: &Simulation, allowed: bool) -> InstitutionalRule {
    InstitutionalRule {
        subjects: Subjects::named(ruler(sim)),
        when: RuleExpression::All(vec![
            RuleExpression::Fact(SocialCondition::Local),
            RuleExpression::Fact(SocialCondition::Member),
        ]),
        effect: SocialEffect::Access {
            inventory: food(sim),
            allowed,
        },
        authority: 0,
    }
}
fn charter(sim: &Simulation, rules: Vec<InstitutionalRule>) -> InstitutionCharter {
    InstitutionCharter {
        purpose: "Our chosen arrangement".into(),
        jurisdiction: AreaId(1),
        founders: BTreeSet::from([ruler(sim), owner(sim)]),
        open_membership: false,
        procedure: DecisionProcedure {
            proposers: Subjects::members(),
            voters: Subjects::members(),
            minimum_support: 2,
        },
        offices: vec![OfficeMandate {
            slot: 0,
            holder: ruler(sim),
            inventories: BTreeSet::from([food(sim)]),
            sites: BTreeSet::from([site(sim)]),
            powers: BTreeSet::from([
                OfficePower::Access,
                OfficePower::Labor,
                OfficePower::Delivery,
                OfficePower::HearAccessCases,
            ]),
            minimum_support: 2,
        }],
        rules,
        contributions: Vec::new(),
        evidence: BTreeSet::new(),
    }
}
fn found(sim: &mut Simulation, charter: InstitutionCharter) -> InstitutionId {
    let founders = charter.founders.clone();
    action(
        sim,
        InstitutionAction::Found {
            charter,
            origin: None,
        },
    )
    .unwrap();
    let id = sim.institution_view(ruler(sim)).first().unwrap().id;
    for actor in founders {
        if actor != ruler(sim) {
            sim.debug_institution_action(actor, InstitutionAction::Join { institution: id })
                .unwrap();
        }
    }
    id
}
fn can_eat(sim: &Simulation) -> bool {
    sim.observe()
        .physical
        .unwrap()
        .local_stocks
        .iter()
        .find(|s| s.id == food(sim))
        .unwrap()
        .can_access
}

#[test]
fn people_construct_and_adopt_rules_from_exclusion_then_do_real_work() {
    let mut sim = fixture(4);
    assert!(
        sim.canonical_state()
            .institutions
            .as_ref()
            .unwrap()
            .institutions
            .is_empty()
    );
    sim.advance(SimDuration(360_000)).unwrap();
    let world = sim.canonical_state().institutions.as_ref().unwrap();
    let institution = world
        .institutions
        .values()
        .find(|i| i.status == InstitutionStatus::Active)
        .expect("people founded an arrangement");
    assert!(
        institution
            .charter
            .founders
            .iter()
            .all(|id| *id != ruler(&sim))
    );
    assert!(!institution.charter.evidence.is_empty());
    assert!(
        institution
            .charter
            .rules
            .iter()
            .any(|r| matches!(r.effect, SocialEffect::Labor { .. }))
    );
    assert!(
        institution
            .charter
            .rules
            .iter()
            .any(|r| matches!(r.effect, SocialEffect::Access { allowed: true, .. }))
    );
    assert!(institution.grants.values().any(|actors| !actors.is_empty()));
    let duties: BTreeSet<_> = institution
        .applied
        .values()
        .flat_map(|actors| actors.values().copied())
        .collect();
    assert!(duties.iter().any(|id| {
        sim.canonical_state()
            .agreements
            .as_ref()
            .unwrap()
            .agreements[id]
            .status
            == AgreementStatus::Fulfilled
    }));
    assert!(
        sim.canonical_state()
            .events
            .iter()
            .any(|e| matches!(e.kind, EventKind::Produced { .. }))
    );
    assert!(
        world
            .memories
            .values()
            .any(|m| m.values.values().any(|v| v.experiences > 0))
    );
}

#[test]
fn ordered_rules_change_access_while_other_permissions_remain_independent() {
    for last_allowed in [false, true] {
        let mut sim = fixture(1);
        let allow = basic_rule(&sim, true);
        let deny = basic_rule(&sim, false);
        let rules = if last_allowed {
            vec![deny, allow]
        } else {
            vec![allow, deny]
        };
        let charter = charter(&sim, rules);
        found(&mut sim, charter);
        let before = sim.canonical_state().inventories[&food(&sim)].quantity;
        sim.advance(MINUTE).unwrap();
        assert_eq!(can_eat(&sim), last_allowed);
        assert!(
            sim.canonical_state().inventories[&food(&sim)]
                .access
                .contains(&owner(&sim))
        );
        if last_allowed {
            let source = food(&sim);
            command(&mut sim, Command::Eat { source }).unwrap();
            sim.advance(MINUTE).unwrap();
            assert_eq!(
                sim.canonical_state().inventories[&source].quantity.0,
                before.0 - 1_000
            );
        } else {
            let before = sim.canonical_state().clone();
            let source = food(&sim);
            assert_eq!(
                command(&mut sim, Command::Eat { source }),
                Err(Error::Unavailable)
            );
            assert_eq!(sim.canonical_state(), &before);
        }
    }
}

#[test]
fn withdrawing_support_removes_office_power_without_removing_the_person() {
    let mut sim = fixture(1);
    let rule = basic_rule(&sim, true);
    let charter = charter(&sim, vec![rule]);
    let id = found(&mut sim, charter);
    sim.advance(MINUTE).unwrap();
    assert!(can_eat(&sim));
    sim.debug_institution_action(
        owner(&sim),
        InstitutionAction::SupportOffice {
            institution: id,
            office: 0,
            support: false,
        },
    )
    .unwrap();
    assert!(!can_eat(&sim));
    assert!(!sim.institution_view(ruler(&sim))[0].offices[0].effective);
    assert_eq!(sim.canonical_state().people[0].role, Role::Ruler);
    assert_eq!(sim.canonical_state().ruler, ruler(&sim));
    command(&mut sim, Command::Rest).unwrap();
    sim.debug_institution_action(
        owner(&sim),
        InstitutionAction::SupportOffice {
            institution: id,
            office: 0,
            support: true,
        },
    )
    .unwrap();
    assert!(can_eat(&sim));
    action(&mut sim, InstitutionAction::Leave { institution: id }).unwrap();
    assert!(!can_eat(&sim));
    assert!(
        sim.canonical_state().inventories[&food(&sim)]
            .access
            .contains(&owner(&sim))
    );
}

#[test]
fn founding_contributions_are_debt_until_physical_delivery_and_survive_exit() {
    let mut sim = fixture(1);
    let source = food(&sim);
    let actor = ruler(&sim);
    let recipient = owner(&sim);
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
    let rule = basic_rule(&sim, true);
    let mut charter = charter(&sim, vec![rule]);
    charter.contributions.push(AgreementTerm::Delivery {
        debtor: actor,
        creditor: recipient,
        source,
        destination,
        quantity: Quantity(2_000),
        deadline: SimTime(600_000),
    });
    let id = found(&mut sim, charter);
    let agreement = sim.institution_view(actor)[0].funding[0];
    assert_eq!(
        sim.canonical_state().inventories[&destination].quantity,
        Quantity(0)
    );
    assert_eq!(
        sim.canonical_state()
            .agreements
            .as_ref()
            .unwrap()
            .agreements[&agreement]
            .status,
        AgreementStatus::Active
    );
    sim.advance(MINUTE).unwrap();
    command(
        &mut sim,
        Command::Load {
            source,
            destination: cargo,
            quantity: Quantity(2_000),
        },
    )
    .unwrap();
    sim.advance(MINUTE).unwrap();
    action(&mut sim, InstitutionAction::Leave { institution: id }).unwrap();
    command(
        &mut sim,
        Command::Load {
            source: cargo,
            destination,
            quantity: Quantity(2_000),
        },
    )
    .unwrap();
    sim.advance(MINUTE).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&destination].quantity,
        Quantity(2_000)
    );
    assert_eq!(
        sim.canonical_state()
            .agreements
            .as_ref()
            .unwrap()
            .agreements[&agreement]
            .status,
        AgreementStatus::Fulfilled
    );
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
}

#[test]
fn actual_contribution_teaches_a_value_that_changes_a_later_vote() {
    let mut sim = fixture(1);
    let worker = sim.canonical_state().people[4].id;
    let beneficiary = owner(&sim);
    let actor = ruler(&sim);
    let site = site(&sim);
    let source = food(&sim);
    let labor = InstitutionalRule {
        subjects: Subjects::named(worker),
        when: RuleExpression::All(vec![
            RuleExpression::Fact(SocialCondition::Member),
            RuleExpression::Fact(SocialCondition::Local),
        ]),
        effect: SocialEffect::Labor {
            site,
            millis: 60_000,
            beneficiary: Beneficiary::Person(beneficiary),
            due_after: SimDuration(180_000),
        },
        authority: 0,
    };
    let access = InstitutionalRule {
        subjects: Subjects::named(worker),
        when: RuleExpression::Fact(SocialCondition::KnownCooperationAtLeast(1)),
        effect: SocialEffect::Access {
            inventory: source,
            allowed: true,
        },
        authority: 0,
    };
    let mut charter = charter(&sim, vec![labor.clone(), access]);
    charter.founders.insert(worker);
    charter.offices[0].holder = beneficiary;
    charter.procedure.proposers = Subjects::named(actor);
    charter.procedure.voters = Subjects::named(worker);
    charter.procedure.minimum_support = 1;
    let id = found(&mut sim, charter);
    sim.advance(SimDuration(120_000)).unwrap();
    assert!(
        sim.canonical_state()
            .institutions
            .as_ref()
            .unwrap()
            .memories[&worker]
            .values
            .values()
            .any(|v| v.weight >= 500)
    );
    let mut unlearned = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    unlearned
        .debug_clear_social_learning(worker, true, false)
        .unwrap();
    for world in [&mut sim, &mut unlearned] {
        action(
            world,
            InstitutionAction::Propose {
                institution: id,
                change: InstitutionChange::AddRule(labor.clone()),
            },
        )
        .unwrap();
        world.advance(MINUTE).unwrap();
    }
    let learned = sim.institution_view(actor)[0].proposals[0].status;
    let unlearned = unlearned.institution_view(actor)[0].proposals[0].status;
    assert_eq!(learned, RuleProposalStatus::Adopted);
    assert_eq!(unlearned, RuleProposalStatus::Rejected);
}

#[test]
fn a_relevant_known_case_changes_judgment_and_an_unknown_case_cannot() {
    let mut sim = fixture(1);
    let owner = owner(&sim);
    let previous = sim.canonical_state().people[4].id;
    let subject = sim.canonical_state().people[5].id;
    let judge = sim.canonical_state().people[1].id;
    let source = food(&sim);
    sim.debug_set_personality(
        owner,
        Personality {
            empathy: 100,
            ..Personality::restful()
        },
        Dream::Comfort,
    )
    .unwrap();
    let rule = InstitutionalRule {
        subjects: Subjects::members(),
        when: RuleExpression::Fact(SocialCondition::KnownCooperationAtLeast(64)),
        effect: SocialEffect::Access {
            inventory: source,
            allowed: true,
        },
        authority: 0,
    };
    let mut charter = charter(&sim, vec![rule]);
    charter.founders.extend([previous, subject, judge]);
    charter.offices[0].holder = judge;
    let id = found(&mut sim, charter);
    sim.debug_move_actor(judge, AreaId(2)).unwrap();
    sim.advance(SimDuration(120_000)).unwrap();
    sim.debug_agreement_action(
        owner,
        AgreementAction::Propose {
            parties: BTreeSet::from([owner, previous]),
            terms: vec![AgreementTerm::Access {
                grantor: owner,
                grantee: previous,
                inventory: source,
                allowed: true,
            }],
            procedure: None,
        },
    )
    .unwrap();
    let agreement = sim.agreement_view(owner).last().unwrap().id;
    sim.debug_agreement_action(previous, AgreementAction::Accept { agreement })
        .unwrap();
    sim.debug_actor_command(previous, Command::Eat { source })
        .unwrap();
    sim.debug_move_actor(judge, AreaId(1)).unwrap();
    sim.advance(SimDuration(120_000)).unwrap();
    let prior = sim
        .social_case_view(previous)
        .iter()
        .find(|c| c.kind == SocialCaseKind::Exclusion && c.outcome == SocialOutcome::Shared)
        .unwrap()
        .id;
    assert!(!sim.social_case_view(judge).iter().any(|c| c.id == prior));
    let current = sim
        .social_case_view(subject)
        .iter()
        .find(|c| {
            c.kind == SocialCaseKind::Exclusion
                && c.outcome == SocialOutcome::Unresolved
                && c.inventory == Some(source)
        })
        .unwrap()
        .id;
    sim.debug_institution_action(
        subject,
        InstitutionAction::TellCase {
            recipient: judge,
            case: current,
        },
    )
    .unwrap();
    let mut informed = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    informed
        .debug_institution_action(
            previous,
            InstitutionAction::TellCase {
                recipient: judge,
                case: prior,
            },
        )
        .unwrap();
    sim.advance(MINUTE).unwrap();
    informed.advance(MINUTE).unwrap();
    assert!(!sim.canonical_state().institutions.as_ref().unwrap().heard[&current]);
    assert!(
        informed
            .canonical_state()
            .institutions
            .as_ref()
            .unwrap()
            .heard[&current]
    );
    assert!(
        !sim.canonical_state()
            .institutions
            .as_ref()
            .unwrap()
            .institutions[&id]
            .grants
            .values()
            .any(|actors| actors.contains(&subject))
    );
    assert!(
        informed
            .canonical_state()
            .institutions
            .as_ref()
            .unwrap()
            .institutions[&id]
            .grants
            .values()
            .any(|actors| actors.contains(&subject))
    );
}

#[test]
fn local_observation_keeps_absent_members_information_stale_and_replay_exact() {
    let mut sim = fixture(1);
    let rule = basic_rule(&sim, true);
    let charter = charter(&sim, vec![rule]);
    let id = found(&mut sim, charter);
    let actor = ruler(&sim);
    let member = owner(&sim);
    sim.advance(MINUTE).unwrap();
    sim.debug_move_actor(member, AreaId(2)).unwrap();
    sim.advance(SimDuration(120_000)).unwrap();
    let before = sim.institution_view(member);
    action(
        &mut sim,
        InstitutionAction::SupportOffice {
            institution: id,
            office: 0,
            support: false,
        },
    )
    .unwrap();
    assert_eq!(sim.institution_view(member), before);
    let saved = sim.save().unwrap();
    let mut other = Simulation::load(
        &saved,
        Execution {
            workers: 4,
            reverse_jobs: true,
        },
    )
    .unwrap();
    let snapshot = sim.canonical_state().clone();
    for _ in 0..20 {
        let _ = sim.observe();
        let _ = sim.institution_view(actor);
        let _ = sim.social_case_view(actor);
    }
    assert_eq!(sim.canonical_state(), &snapshot);
    sim.advance(SimDuration(180_000)).unwrap();
    for amount in [17_123, 71_999, 90_878] {
        other.advance(SimDuration(amount)).unwrap();
    }
    assert_eq!(sim.canonical_state(), other.canonical_state());
}

#[test]
fn invalid_scope_is_inert_and_new_factions_require_their_own_consent() {
    let mut sim = fixture(1);
    let rule = basic_rule(&sim, true);
    let mut invalid = charter(&sim, vec![rule.clone()]);
    invalid.offices[0]
        .inventories
        .insert(sim.canonical_state().people[4].inventory);
    let before = sim.canonical_state().clone();
    assert!(
        action(
            &mut sim,
            InstitutionAction::Found {
                charter: invalid,
                origin: None
            }
        )
        .is_err()
    );
    assert_eq!(sim.canonical_state(), &before);
    let original = charter(&sim, vec![rule.clone()]);
    let id = found(&mut sim, original);
    let new = charter(&sim, vec![rule]);
    action(
        &mut sim,
        InstitutionAction::Found {
            charter: new,
            origin: Some(id),
        },
    )
    .unwrap();
    let newest = sim.institution_view(ruler(&sim))[0].clone();
    assert_eq!(newest.origin, Some(id));
    assert_eq!(newest.status, InstitutionStatus::Proposed);
    assert_eq!(
        sim.canonical_state()
            .institutions
            .as_ref()
            .unwrap()
            .institutions[&id]
            .status,
        InstitutionStatus::Active
    );
}
