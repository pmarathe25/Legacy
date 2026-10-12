use super::*;
use crate::policy_fixtures::*;
use crate::policy_fixtures::{Knowledge, case, institution};
fn alive() -> AuthorityFacts {
    AuthorityFacts {
        alive: refs().people,
    }
}
fn recruits() -> Vec<Recruit> {
    (1..=12)
        .map(|id| Recruit {
            actor: PersonId(id),
            area: Some(AreaId(3)),
            autonomous: true,
            citizen: true,
        })
        .collect()
}
#[test]
fn hearings_require_held_power_locality_personal_case_knowledge_and_ordered_current_authority() {
    let (state, mut store) = world();
    let mut i = institution(10, vec![], InstitutionStatus::Active);
    i.members = BTreeSet::from([PersonId(1), PersonId(2)]);
    i.office_support.insert(0, BTreeSet::from([PersonId(2)]));
    i.charter.offices[0]
        .powers
        .insert(OfficePower::HearAccessCases);
    seed_institution(&mut store, i);
    crate::storage::insert_case(
        &mut store,
        CaseId(30),
        case(30, 1, SocialCaseKind::Exclusion, SocialOutcome::Unresolved),
    )
    .unwrap();
    crate::storage::insert_case(
        &mut store,
        CaseId(31),
        case(31, 1, SocialCaseKind::Exclusion, SocialOutcome::Unresolved),
    )
    .unwrap();
    ensure_memory(&mut store, PersonId(2))
        .cases
        .insert(CaseId(30));
    let judge = HearingActor {
        actor: PersonId(2),
        area: Some(AreaId(3)),
        autonomous: true,
        empathy: 500,
    };
    let action = (
        PersonId(2),
        InstitutionAction::HearCase {
            institution: InstitutionId(10),
            office: 0,
            case: CaseId(30),
            share: true,
        },
    );
    assert_eq!(
        state
            .hearing_intentions(&store, &alive(), &[judge])
            .unwrap(),
        vec![action]
    );
    assert!(
        state
            .hearing_intentions(
                &store,
                &alive(),
                &[HearingActor {
                    area: None,
                    ..judge
                }]
            )
            .unwrap()
            .is_empty()
    );
    assert!(
        state
            .hearing_intentions(
                &store,
                &AuthorityFacts {
                    alive: BTreeSet::from([PersonId(1)])
                },
                &[judge]
            )
            .unwrap()
            .is_empty()
    );
    crate::storage::insert_hearing(&mut store, CaseId(30), Hearing { shared: false }).unwrap();
    assert!(
        state
            .hearing_intentions(&store, &alive(), &[judge])
            .unwrap()
            .is_empty()
    );
    crate::storage::remove_hearing(&mut store, CaseId(30)).unwrap();
    for id in 32..=95 {
        crate::storage::insert_case(
            &mut store,
            CaseId(id),
            case(
                id,
                1,
                SocialCaseKind::Cooperation,
                SocialOutcome::Contributed,
            ),
        )
        .unwrap();
        crate::storage::knowledge_mut(&mut store, PersonId(2))
            .unwrap()
            .cases
            .insert(CaseId(id));
    }
    assert!(
        state
            .hearing_intentions(&store, &alive(), &[judge])
            .unwrap()
            .is_empty()
    );
}
#[test]
fn ordered_default_reform_precedes_later_join_and_invalid_deeper_rule_falls_through() {
    let mut known = Knowledge::default();
    known.stocks.insert(InventoryId(4), stock(2_000));
    let v = voter(&known);
    let refs = refs();
    let alive = alive();
    let roster = recruits();
    let access = InstitutionalRule {
        effect: SocialEffect::Access {
            inventory: InventoryId(4),
            allowed: true,
        },
        ..labor()
    };
    let (state, mut store) = world();
    let mut i = institution(10, vec![access], InstitutionStatus::Active);
    i.members.insert(PersonId(1));
    seed_institution(&mut store, i);
    crate::storage::insert_institution(
        &mut store,
        InstitutionId(11),
        institution(11, vec![], InstitutionStatus::Active),
    )
    .unwrap();
    for id in [30, 31] {
        crate::storage::insert_case(
            &mut store,
            CaseId(id),
            case(id, 1, SocialCaseKind::Default, SocialOutcome::Defaulted),
        )
        .unwrap();
        ensure_memory(&mut store, PersonId(1))
            .cases
            .insert(CaseId(id));
    }
    let decide = |state: &Runtime, store: &Store| {
        state.discovery_decision(
            store,
            DiscoveryFacts {
                voter: &v,
                now: SimTime(0),
                authority: &alive,
                references: &refs,
                sites: &[],
                recruits: &roster,
            },
        )
    };
    let first = decide(&state, &store).unwrap();
    assert!(matches!(
        first.action,
        InstitutionAction::Propose {
            institution: InstitutionId(10),
            change: InstitutionChange::ReplaceRule { .. }
        }
    ));
    state
        .record_discovery_attempt(&mut store, PersonId(1), &first, false)
        .unwrap();
    assert_eq!(
        crate::storage::requests(&store, PersonId(1))
            .unwrap()
            .search_after,
        SimTime(300_000)
    );
    assert!(decide(&state, &store).is_none());
    crate::storage::requests_mut(&mut store, PersonId(1))
        .unwrap()
        .search_after = SimTime(0);
    let mut expression = RuleExpression::Fact(SocialCondition::Member);
    for _ in 0..4 {
        expression = RuleExpression::Not(Box::new(expression));
    }
    crate::storage::institution_mut(&mut store, InstitutionId(10))
        .unwrap()
        .charter
        .rules[0]
        .when = expression;
    assert!(matches!(
        decide(&state, &store).unwrap().action,
        InstitutionAction::Join {
            institution: InstitutionId(11)
        }
    ));
}
#[test]
fn perceived_exclusion_creates_valid_charter_and_previous_public_commit_blocks_duplicate_founding()
{
    let mut known = Knowledge::default();
    known.stocks.insert(InventoryId(4), stock(4_000));
    let mut v = voter(&known);
    v.empathy = 900;
    let refs = refs();
    let alive = alive();
    let roster = recruits();
    let sites = [LivelihoodSite {
        site: SiteId(20),
        area: AreaId(3),
        accessible: true,
    }];
    let (state, mut store) = world();
    for actor in &refs.people {
        ensure_memory(&mut store, *actor);
    }
    crate::storage::insert_case(
        &mut store,
        CaseId(30),
        case(30, 1, SocialCaseKind::Exclusion, SocialOutcome::Unresolved),
    )
    .unwrap();
    ensure_memory(&mut store, PersonId(1))
        .cases
        .insert(CaseId(30));
    let decision = state
        .discovery_decision(
            &store,
            DiscoveryFacts {
                voter: &v,
                now: SimTime(0),
                authority: &alive,
                references: &refs,
                sites: &sites,
                recruits: &roster,
            },
        )
        .unwrap();
    let InstitutionAction::Found { ref charter, .. } = decision.action else {
        panic!("exclusion produces a charter")
    };
    assert_eq!(
        charter.founders,
        BTreeSet::from([
            PersonId(1),
            PersonId(2),
            PersonId(3),
            PersonId(4),
            PersonId(5),
            PersonId(6),
            PersonId(7),
            PersonId(8)
        ])
    );
    assert_eq!(charter.procedure.minimum_support, 5);
    assert_eq!(decision.failure, FailureHandling::Propagate);
    state
        .prepare_command(
            &store,
            &decision.action,
            &CommandFacts {
                actor: PersonId(1),
                area: AreaId(3),
                now: SimTime(0),
                references: &refs,
                authority: &alive,
                external_permit: None,
            },
            CommandIds { first: 100 },
        )
        .unwrap()
        .commit(&state, &mut store)
        .unwrap();
    state
        .record_discovery_attempt(&mut store, PersonId(1), &decision, true)
        .unwrap();
    crate::storage::insert_case(
        &mut store,
        CaseId(31),
        case(31, 3, SocialCaseKind::Exclusion, SocialOutcome::Unresolved),
    )
    .unwrap();
    ensure_memory(&mut store, PersonId(3))
        .cases
        .insert(CaseId(31));
    let next = VoterFacts {
        actor: PersonId(3),
        ..v
    };
    assert!(
        state
            .discovery_decision(
                &store,
                DiscoveryFacts {
                    voter: &next,
                    now: SimTime(0),
                    authority: &alive,
                    references: &refs,
                    sites: &sites,
                    recruits: &roster
                }
            )
            .is_none()
    );
    // The preceding actor's actual command is visible to the next preparation.
    assert_eq!(crate::storage::institutions(&store).count(), 1);
}

#[test]
fn declared_duty_uses_physiological_budget_and_individual_willingness_without_duplicate_work() {
    let (state, mut store) = world();
    let mut institution = institution(10, vec![labor()], InstitutionStatus::Active);
    institution.applied.insert(
        ConceptId(21),
        BTreeMap::from([(PersonId(1), AgreementId(40))]),
    );
    seed_institution(&mut store, institution);
    game_agreements::Ledger::new(&store)
        .prepare_institutional_duties(
            &[game_agreements::InstitutionDuty {
                institution: InstitutionId(10),
                agreement: AgreementId(40),
                obligation: ObligationId(41),
                term: game_agreements::AgreementTerm::Labor {
                    debtor: PersonId(1),
                    creditor: PersonId(2),
                    site: SiteId(20),
                    millis: 60_000,
                    deadline: SimTime(180_000),
                },
            }],
            SimTime(0),
        )
        .unwrap()
        .commit(&mut store)
        .unwrap();
    let sites = [LivelihoodSite {
        site: SiteId(20),
        area: AreaId(3),
        accessible: true,
    }];
    let worker = DutyWorker {
        actor: PersonId(1),
        autonomous: true,
        area: Some(AreaId(3)),
        idle: true,
        work_budget: SimDuration(60_000),
        discipline: 500,
        empathy: 0,
        sites: &sites,
    };
    assert_eq!(
        state.duty_work_intentions(&store, &[worker]).unwrap(),
        vec![DutyWork {
            actor: PersonId(1),
            site: SiteId(20)
        }]
    );
    let unwilling = DutyWorker {
        actor: PersonId(1),
        autonomous: true,
        area: Some(AreaId(3)),
        idle: true,
        work_budget: SimDuration(60_000),
        discipline: 499,
        empathy: 499,
        sites: &sites,
    };
    assert!(
        state
            .duty_work_intentions(&store, &[unwilling])
            .unwrap()
            .is_empty()
    );
    let exhausted = DutyWorker {
        actor: PersonId(1),
        autonomous: true,
        area: Some(AreaId(3)),
        idle: true,
        work_budget: SimDuration(59_999),
        discipline: 500,
        empathy: 0,
        sites: &sites,
    };
    assert!(
        state
            .duty_work_intentions(&store, &[exhausted])
            .unwrap()
            .is_empty()
    );
}
