use super::*;
fn setup() -> (Runtime, Store, ReferenceFacts, AuthorityFacts) {
    let people = BTreeSet::from([PersonId(1), PersonId(2), PersonId(3)]);
    let mut schema = game_ecs::Schema::new();
    crate::storage::register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    for actor in &people {
        insert_knowledge(&mut store, *actor, Knowledge::default()).unwrap();
    }
    (
        Runtime::default(),
        store,
        ReferenceFacts {
            people: people.clone(),
            eligible_locations: people.iter().map(|actor| (*actor, AreaId(4))).collect(),
            stocks: BTreeMap::from([
                (
                    InventoryId(10),
                    StockReference {
                        area: Some(AreaId(4)),
                        custodian: Some(PersonId(1)),
                        good: Good::Food,
                        capacity: Quantity(10_000),
                    },
                ),
                (
                    InventoryId(11),
                    StockReference {
                        area: Some(AreaId(4)),
                        custodian: Some(PersonId(2)),
                        good: Good::Food,
                        capacity: Quantity(10_000),
                    },
                ),
            ]),
            sites: BTreeMap::from([(SiteId(12), AreaId(4))]),
        },
        AuthorityFacts { alive: people },
    )
}
fn charter() -> InstitutionCharter {
    InstitutionCharter {
        purpose: "People choose a local arrangement".into(),
        jurisdiction: AreaId(4),
        founders: BTreeSet::from([PersonId(1), PersonId(2)]),
        open_membership: false,
        procedure: DecisionProcedure {
            proposers: Subjects::members(),
            voters: Subjects::members(),
            minimum_support: 2,
        },
        offices: vec![OfficeMandate {
            slot: 0,
            holder: PersonId(1),
            inventories: BTreeSet::from([InventoryId(10), InventoryId(11)]),
            sites: BTreeSet::from([SiteId(12)]),
            powers: BTreeSet::from([
                OfficePower::Access,
                OfficePower::Delivery,
                OfficePower::Labor,
            ]),
            minimum_support: 2,
        }],
        rules: vec![rule()],
        contributions: vec![AgreementTerm::Delivery {
            debtor: PersonId(1),
            creditor: PersonId(2),
            source: InventoryId(10),
            destination: InventoryId(11),
            quantity: Quantity(1_000),
            deadline: SimTime(1_000_000),
        }],
        evidence: BTreeSet::new(),
    }
}
fn rule() -> InstitutionalRule {
    InstitutionalRule {
        subjects: Subjects::members(),
        when: RuleExpression::Fact(SocialCondition::Local),
        effect: SocialEffect::Access {
            inventory: InventoryId(10),
            allowed: true,
        },
        authority: 0,
    }
}
fn prepare(
    world: &Runtime,
    store: &Store,
    refs: &ReferenceFacts,
    alive: &AuthorityFacts,
    actor: u64,
    action: InstitutionAction,
    first: u64,
) -> Result<PreparedCommand, Error> {
    world.prepare_command(
        store,
        &action,
        &CommandFacts {
            actor: PersonId(actor),
            area: AreaId(4),
            now: SimTime(1),
            references: refs,
            authority: alive,
            external_permit: None,
        },
        CommandIds { first },
    )
}
fn founded() -> (Runtime, Store, ReferenceFacts, AuthorityFacts) {
    let (world, mut store, refs, alive) = setup();
    prepare(
        &world,
        &store,
        &refs,
        &alive,
        1,
        InstitutionAction::Found {
            charter: charter(),
            origin: None,
        },
        100,
    )
    .unwrap()
    .commit(&world, &mut store)
    .unwrap();
    prepare(
        &world,
        &store,
        &refs,
        &alive,
        2,
        InstitutionAction::Join {
            institution: InstitutionId(100),
        },
        102,
    )
    .unwrap()
    .commit(&world, &mut store)
    .unwrap();
    (world, store, refs, alive)
}
#[test]
fn founding_requires_actual_personal_acceptance_and_requests_only_promised_funding() {
    let (world, mut store, mut refs, alive) = setup();
    let proposal = prepare(
        &world,
        &store,
        &refs,
        &alive,
        1,
        InstitutionAction::Found {
            charter: charter(),
            origin: None,
        },
        100,
    )
    .unwrap();
    assert!(crate::storage::institutions(&store).next().is_none());
    assert_eq!(proposal.next_id, 102);
    proposal.commit(&world, &mut store).unwrap();
    assert_eq!(
        crate::storage::institution(&store, InstitutionId(100))
            .unwrap()
            .status,
        InstitutionStatus::Proposed
    );
    assert_eq!(
        crate::storage::institution(&store, InstitutionId(100))
            .unwrap()
            .members,
        BTreeSet::from([PersonId(1)])
    );
    refs.eligible_locations.insert(PersonId(1), AreaId(5));
    let before = store.snapshot().unwrap();
    assert!(
        prepare(
            &world,
            &store,
            &refs,
            &alive,
            2,
            InstitutionAction::Join {
                institution: InstitutionId(100)
            },
            102
        )
        .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
    refs.eligible_locations.insert(PersonId(1), AreaId(4));
    let activation = prepare(
        &world,
        &store,
        &refs,
        &alive,
        2,
        InstitutionAction::Join {
            institution: InstitutionId(100),
        },
        102,
    )
    .unwrap();
    assert_eq!(activation.next_id, 104);
    assert!(matches!(
        activation.effects.first(),
        Some(CommandEffect::Funding {
            agreement: AgreementId(102),
            obligation: ObligationId(103),
            ..
        })
    ));
    activation.commit(&world, &mut store).unwrap();
    let institution = &crate::storage::institution(&store, InstitutionId(100)).unwrap();
    assert_eq!(institution.status, InstitutionStatus::Active);
    assert_eq!(institution.funding, vec![AgreementId(102)]);
    assert_eq!(institution.office_support[&0], institution.members);
}
#[test]
fn membership_and_departure_follow_the_particular_charter_and_remove_support_and_grants() {
    let (world, mut store, refs, alive) = founded();
    assert!(
        prepare(
            &world,
            &store,
            &refs,
            &alive,
            3,
            InstitutionAction::Join {
                institution: InstitutionId(100)
            },
            104
        )
        .is_err()
    );
    crate::storage::institution_mut(&mut store, InstitutionId(100))
        .unwrap()
        .invited
        .insert(PersonId(3));
    prepare(
        &world,
        &store,
        &refs,
        &alive,
        3,
        InstitutionAction::Join {
            institution: InstitutionId(100),
        },
        104,
    )
    .unwrap()
    .commit(&world, &mut store)
    .unwrap();
    crate::storage::institution_mut(&mut store, InstitutionId(100))
        .unwrap()
        .grants
        .insert(ConceptId(101), BTreeSet::from([PersonId(3)]));
    prepare(
        &world,
        &store,
        &refs,
        &alive,
        3,
        InstitutionAction::Leave {
            institution: InstitutionId(100),
        },
        104,
    )
    .unwrap()
    .commit(&world, &mut store)
    .unwrap();
    let institution = &crate::storage::institution(&store, InstitutionId(100)).unwrap();
    assert!(!institution.members.contains(&PersonId(3)));
    assert!(!institution.office_support[&0].contains(&PersonId(3)));
    assert!(!institution.grants[&ConceptId(101)].contains(&PersonId(3)));
    let mut open_store = store.clone();
    crate::storage::institution_mut(&mut open_store, InstitutionId(100))
        .unwrap()
        .charter
        .open_membership = true;
    crate::storage::institution_mut(&mut open_store, InstitutionId(100))
        .unwrap()
        .invited
        .clear();
    assert!(
        prepare(
            &world,
            &open_store,
            &refs,
            &alive,
            3,
            InstitutionAction::Join {
                institution: InstitutionId(100)
            },
            104
        )
        .is_ok()
    );
}
#[test]
fn a_declining_founder_rejects_the_proposal_without_obligating_anybody() {
    let (world, mut store, refs, alive) = setup();
    prepare(
        &world,
        &store,
        &refs,
        &alive,
        1,
        InstitutionAction::Found {
            charter: charter(),
            origin: None,
        },
        100,
    )
    .unwrap()
    .commit(&world, &mut store)
    .unwrap();
    prepare(
        &world,
        &store,
        &refs,
        &alive,
        2,
        InstitutionAction::Decline {
            institution: InstitutionId(100),
        },
        102,
    )
    .unwrap()
    .commit(&world, &mut store)
    .unwrap();
    assert_eq!(
        crate::storage::institution(&store, InstitutionId(100))
            .unwrap()
            .status,
        InstitutionStatus::Rejected
    );
    assert!(
        crate::storage::institution(&store, InstitutionId(100))
            .unwrap()
            .funding
            .is_empty()
    );
    assert!(
        prepare(
            &world,
            &store,
            &refs,
            &alive,
            2,
            InstitutionAction::Join {
                institution: InstitutionId(100)
            },
            102
        )
        .is_err()
    );
}
#[test]
fn a_vote_and_resolution_commit_atomically_and_invalidate_competing_proposals() {
    let (world, mut store, refs, alive) = founded();
    for first in [104, 105] {
        prepare(
            &world,
            &store,
            &refs,
            &alive,
            1,
            InstitutionAction::Propose {
                institution: InstitutionId(100),
                change: InstitutionChange::AddRule(rule()),
            },
            first,
        )
        .unwrap()
        .commit(&world, &mut store)
        .unwrap();
    }
    let vote = InstitutionAction::Vote {
        proposal: RuleProposalId(104),
        support: true,
    };
    prepare(&world, &store, &refs, &alive, 1, vote.clone(), 106)
        .unwrap()
        .commit(&world, &mut store)
        .unwrap();
    assert_eq!(
        crate::storage::proposal(&store, RuleProposalId(104))
            .unwrap()
            .status,
        RuleProposalStatus::Proposed
    );
    let before = store.snapshot().unwrap();
    let adoption = prepare(&world, &store, &refs, &alive, 2, vote.clone(), 106).unwrap();
    assert_eq!(store.snapshot().unwrap(), before);
    assert_eq!(adoption.next_id, 107);
    adoption.commit(&world, &mut store).unwrap();
    assert_eq!(
        crate::storage::proposal(&store, RuleProposalId(104))
            .unwrap()
            .votes
            .len(),
        2
    );
    assert_eq!(
        crate::storage::proposal(&store, RuleProposalId(104))
            .unwrap()
            .status,
        RuleProposalStatus::Adopted
    );
    assert_eq!(
        crate::storage::proposal(&store, RuleProposalId(105))
            .unwrap()
            .status,
        RuleProposalStatus::Rejected
    );
    assert_eq!(
        crate::storage::institution(&store, InstitutionId(100))
            .unwrap()
            .revision,
        1
    );
    assert!(prepare(&world, &store, &refs, &alive, 2, vote, 107).is_err());
}
#[test]
fn stale_membership_or_proposal_preparation_never_partially_installs_a_vote() {
    let (world, mut store, refs, alive) = founded();
    prepare(
        &world,
        &store,
        &refs,
        &alive,
        1,
        InstitutionAction::Propose {
            institution: InstitutionId(100),
            change: InstitutionChange::Invite(PersonId(3)),
        },
        104,
    )
    .unwrap()
    .commit(&world, &mut store)
    .unwrap();
    let pending = prepare(
        &world,
        &store,
        &refs,
        &alive,
        1,
        InstitutionAction::Vote {
            proposal: RuleProposalId(104),
            support: true,
        },
        105,
    )
    .unwrap();
    crate::storage::institution_mut(&mut store, InstitutionId(100))
        .unwrap()
        .members
        .remove(&PersonId(2));
    let before = store.snapshot().unwrap();
    assert_eq!(pending.commit(&world, &mut store), Err(Error::Unavailable));
    assert_eq!(store.snapshot().unwrap(), before);
    assert!(
        crate::storage::proposal(&store, RuleProposalId(104))
            .unwrap()
            .votes
            .is_empty()
    );
}
#[test]
fn referenced_assets_rule_bounds_and_unknown_named_people_are_checked_without_a_simulation() {
    let (world, store, mut refs, alive) = setup();
    let mut charter = charter();
    charter.rules[0].when = RuleExpression::Fact(SocialCondition::KnownStockAtLeast {
        inventory: InventoryId(10),
        quantity: Quantity(1),
    });
    refs.stocks.get_mut(&InventoryId(10)).unwrap().area = Some(AreaId(5));
    assert!(
        prepare(
            &world,
            &store,
            &refs,
            &alive,
            1,
            InstitutionAction::Found {
                charter: charter.clone(),
                origin: None
            },
            100
        )
        .is_err()
    );
    refs.stocks.get_mut(&InventoryId(10)).unwrap().area = Some(AreaId(4));
    charter.rules[0].subjects = Subjects::named(PersonId(99));
    assert!(
        prepare(
            &world,
            &store,
            &refs,
            &alive,
            1,
            InstitutionAction::Found {
                charter: charter.clone(),
                origin: None
            },
            100
        )
        .is_err()
    );
    charter.rules[0].subjects = Subjects::members();
    charter.rules[0].when =
        RuleExpression::All(vec![RuleExpression::Fact(SocialCondition::Local); 9]);
    assert!(
        prepare(
            &world,
            &store,
            &refs,
            &alive,
            1,
            InstitutionAction::Found {
                charter: charter.clone(),
                origin: None
            },
            100
        )
        .is_err()
    );
    charter.rules[0].when = RuleExpression::Fact(SocialCondition::Local);
    assert!(
        prepare(
            &world,
            &store,
            &refs,
            &alive,
            1,
            InstitutionAction::Found {
                charter,
                origin: None
            },
            u64::MAX
        )
        .is_err()
    );
    assert!(crate::storage::institutions(&store).next().is_none());
}
#[test]
fn office_support_is_a_personal_choice_and_liveness_only_changes_effective_authority() {
    let (world, mut store, refs, mut alive) = founded();
    let action = InstitutionAction::SupportOffice {
        institution: InstitutionId(100),
        office: 0,
        support: false,
    };
    prepare(&world, &store, &refs, &alive, 2, action.clone(), 104)
        .unwrap()
        .commit(&world, &mut store)
        .unwrap();
    assert!(prepare(&world, &store, &refs, &alive, 2, action, 104).is_err());
    let institution = &crate::storage::institution(&store, InstitutionId(100)).unwrap();
    assert!(!institution.office_effective(&institution.charter.offices[0], &alive));
    prepare(
        &world,
        &store,
        &refs,
        &alive,
        2,
        InstitutionAction::SupportOffice {
            institution: InstitutionId(100),
            office: 0,
            support: true,
        },
        104,
    )
    .unwrap()
    .commit(&world, &mut store)
    .unwrap();
    alive.alive.remove(&PersonId(1));
    let institution = &crate::storage::institution(&store, InstitutionId(100)).unwrap();
    assert!(!institution.office_effective(&institution.charter.offices[0], &alive));
    assert!(institution.members.contains(&PersonId(1)));
}
#[test]
fn external_changes_need_a_matching_owner_permit_and_remain_external_effects() {
    let (world, mut store, refs, alive) = founded();
    let change = InstitutionChange::RatifyRegional {
        commitment: RealmCommitmentId(44),
        revision: 2,
        support: true,
    };
    let action = InstitutionAction::Propose {
        institution: InstitutionId(100),
        change: change.clone(),
    };
    assert!(prepare(&world, &store, &refs, &alive, 1, action.clone(), 104).is_err());
    let mut permit = ExternalChangePermit {
        actor: PersonId(1),
        institution: InstitutionId(100),
        institution_revision: 0,
        change,
    };
    let facts = |permit: &ExternalChangePermit| {
        world.prepare_command(
            &store,
            &action,
            &CommandFacts {
                actor: PersonId(1),
                area: AreaId(4),
                now: SimTime(1),
                references: &refs,
                authority: &alive,
                external_permit: Some(permit),
            },
            CommandIds { first: 104 },
        )
    };
    permit.actor = PersonId(2);
    assert!(facts(&permit).is_err());
    permit.actor = PersonId(1);
    let plan = facts(&permit).unwrap();
    plan.commit(&world, &mut store).unwrap();
    assert!(
        crate::storage::proposal(&store, RuleProposalId(104))
            .unwrap()
            .votes
            .is_empty()
    );
}

#[test]
fn an_adoption_cannot_leave_a_new_concurrent_proposal_at_the_old_revision() {
    let (world, mut store, refs, alive) = founded();
    prepare(
        &world,
        &store,
        &refs,
        &alive,
        1,
        InstitutionAction::Propose {
            institution: InstitutionId(100),
            change: InstitutionChange::Invite(PersonId(3)),
        },
        104,
    )
    .unwrap()
    .commit(&world, &mut store)
    .unwrap();
    prepare(
        &world,
        &store,
        &refs,
        &alive,
        1,
        InstitutionAction::Vote {
            proposal: RuleProposalId(104),
            support: true,
        },
        105,
    )
    .unwrap()
    .commit(&world, &mut store)
    .unwrap();
    let adoption = prepare(
        &world,
        &store,
        &refs,
        &alive,
        2,
        InstitutionAction::Vote {
            proposal: RuleProposalId(104),
            support: true,
        },
        105,
    )
    .unwrap();
    prepare(
        &world,
        &store,
        &refs,
        &alive,
        1,
        InstitutionAction::Propose {
            institution: InstitutionId(100),
            change: InstitutionChange::Dissolve,
        },
        105,
    )
    .unwrap()
    .commit(&world, &mut store)
    .unwrap();
    let before = store.snapshot().unwrap();
    assert_eq!(adoption.commit(&world, &mut store), Err(Error::Unavailable));
    assert_eq!(store.snapshot().unwrap(), before);
}

#[test]
fn a_new_faction_preparation_rechecks_membership_in_its_chosen_origin() {
    let (world, mut store, refs, alive) = founded();
    let fork = prepare(
        &world,
        &store,
        &refs,
        &alive,
        1,
        InstitutionAction::Found {
            charter: charter(),
            origin: Some(InstitutionId(100)),
        },
        104,
    )
    .unwrap();
    prepare(
        &world,
        &store,
        &refs,
        &alive,
        2,
        InstitutionAction::Leave {
            institution: InstitutionId(100),
        },
        104,
    )
    .unwrap()
    .commit(&world, &mut store)
    .unwrap();
    let before = store.snapshot().unwrap();
    assert_eq!(fork.commit(&world, &mut store), Err(Error::Unavailable));
    assert_eq!(store.snapshot().unwrap(), before);
}

#[test]
fn an_adopted_deposition_changes_only_the_named_office_and_actual_voter_support() {
    let (world, mut store, refs, alive) = founded();
    crate::storage::institution_mut(&mut store, InstitutionId(100))
        .unwrap()
        .invited
        .insert(PersonId(3));
    prepare(
        &world,
        &store,
        &refs,
        &alive,
        3,
        InstitutionAction::Join {
            institution: InstitutionId(100),
        },
        104,
    )
    .unwrap()
    .commit(&world, &mut store)
    .unwrap();
    assert!(
        crate::storage::institution(&store, InstitutionId(100))
            .unwrap()
            .office_support[&0]
            .contains(&PersonId(3))
    );
    let change = InstitutionChange::Adjudicate {
        case: EnforcementCaseId(50),
        remedy: EnforcementRemedy::Depose {
            office: 0,
            replacement: PersonId(2),
        },
    };
    let mut action = InstitutionAction::Propose {
        institution: InstitutionId(100),
        change: change.clone(),
    };
    for (actor, first) in [(1, 104), (1, 105), (2, 105)] {
        let permit = ExternalChangePermit {
            actor: PersonId(actor),
            institution: InstitutionId(100),
            institution_revision: 0,
            change: change.clone(),
        };
        let plan = world
            .prepare_command(
                &store,
                &action,
                &CommandFacts {
                    actor: PersonId(actor),
                    area: AreaId(4),
                    now: SimTime(1),
                    references: &refs,
                    authority: &alive,
                    external_permit: Some(&permit),
                },
                CommandIds { first },
            )
            .unwrap();
        if actor == 2 {
            let before = &crate::storage::institution(&store, InstitutionId(100)).unwrap();
            let members = before.members.clone();
            let funding = before.funding.clone();
            let grants = before.grants.clone();
            assert_eq!(before.charter.offices[0].holder, PersonId(1));
            assert!(plan.effects.iter().any(|effect| matches!(
                effect,
                CommandEffect::ExternalOutcome {
                    change: InstitutionChange::Adjudicate { .. },
                    ..
                }
            )));
            plan.commit(&world, &mut store).unwrap();
            let after = &crate::storage::institution(&store, InstitutionId(100)).unwrap();
            assert_eq!(after.charter.offices[0].holder, PersonId(2));
            assert_eq!(
                after.office_support[&0],
                BTreeSet::from([PersonId(1), PersonId(2)])
            );
            assert_eq!(after.members, members);
            assert_eq!(after.funding, funding);
            assert_eq!(after.grants, grants);
        } else {
            plan.commit(&world, &mut store).unwrap();
        }
        action = InstitutionAction::Vote {
            proposal: RuleProposalId(104),
            support: true,
        };
    }
}
#[test]
fn rejected_native_funding_cannot_publish_the_institutions_accepted_charter() {
    let (world, opening, refs, alive) = setup();
    let mut schema = game_ecs::Schema::new();
    crate::register_components(&mut schema).unwrap();
    schema
        .register::<game_agreements::Agreement>(
            "agreements/agreement",
            game_agreements::Agreement::validate,
        )
        .unwrap();
    schema
        .register::<game_agreements::Obligation>("agreements/obligation", |_| {
            Err(Error::Unavailable)
        })
        .unwrap();
    schema
        .register::<game_agreements::Amendment>(
            "agreements/amendment",
            game_agreements::Amendment::validate,
        )
        .unwrap();
    schema
        .register::<game_agreements::Knowledge>(
            "agreements/knowledge",
            game_agreements::Knowledge::validate,
        )
        .unwrap();
    schema
        .register::<game_agreements::Requests>("agreements/requests", |_| Ok(()))
        .unwrap();
    schema
        .register::<game_agreements::PendingWork>("agreements/pending_work", |_| Ok(()))
        .unwrap();
    schema
        .register::<game_agreements::PendingReport>(
            "agreements/report",
            game_agreements::PendingReport::validate,
        )
        .unwrap();
    let mut snapshot = opening.snapshot().unwrap();
    for key in schema.keys() {
        snapshot.columns.entry(key.into()).or_default();
    }
    let mut store = Store::restore(schema, snapshot).unwrap();
    prepare(
        &world,
        &store,
        &refs,
        &alive,
        1,
        InstitutionAction::Found {
            charter: charter(),
            origin: None,
        },
        100,
    )
    .unwrap()
    .commit(&world, &mut store)
    .unwrap();
    let command = prepare(
        &world,
        &store,
        &refs,
        &alive,
        2,
        InstitutionAction::Join {
            institution: InstitutionId(100),
        },
        102,
    )
    .unwrap();
    let duties = command
        .effects
        .iter()
        .filter_map(|effect| {
            if let CommandEffect::Funding {
                institution,
                agreement,
                obligation,
                term,
            } = effect
            {
                Some(game_agreements::InstitutionDuty {
                    institution: *institution,
                    agreement: *agreement,
                    obligation: *obligation,
                    term: term.clone(),
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let funding = game_agreements::Ledger::new(&store)
        .prepare_institutional_duties(&duties, SimTime(1))
        .unwrap();
    let before = store.clone();
    let runtime_before = world.clone();
    let mut batch = game_ecs::Batch::new();
    funding
        .check(&game_agreements::Ledger::new(&store))
        .unwrap();
    command.stage_into(&world, &store, &mut batch).unwrap();
    funding.stage(&mut batch);
    assert_eq!(store.apply_batch(batch), Err(Error::Unavailable));
    assert_eq!(store, before);
    assert_eq!(world, runtime_before);
    assert_eq!(
        crate::storage::institution(&store, InstitutionId(100))
            .unwrap()
            .status,
        InstitutionStatus::Proposed
    );
    assert!(
        crate::storage::institution(&store, InstitutionId(100))
            .unwrap()
            .funding
            .is_empty()
    );
    assert!(game_agreements::agreement(&store, AgreementId(102)).is_none());
    assert!(!store.contains(game_ecs::StableId(103)));
}
