use super::*;
use std::collections::BTreeSet;
#[derive(Default)]
struct Knowledge {
    stocks: BTreeMap<InventoryId, StockKnowledge>,
    custodians: BTreeMap<InventoryId, PersonId>,
    trust: BTreeMap<PersonId, i32>,
}
impl VotingKnowledge for Knowledge {
    fn stock(&self, id: InventoryId) -> Option<&StockKnowledge> {
        self.stocks.get(&id)
    }
    fn custodian(&self, id: InventoryId) -> Option<PersonId> {
        self.custodians.get(&id).copied()
    }
    fn trust(&self, id: PersonId) -> i32 {
        self.trust.get(&id).copied().unwrap_or(0)
    }
}
fn voter(knowledge: &Knowledge) -> VoterFacts<'_> {
    VoterFacts {
        actor: PersonId(1),
        local_area: Some(AreaId(3)),
        autonomous: true,
        empathy: 500,
        supports_others: false,
        knowledge,
    }
}
fn world() -> World {
    World {
        institutions: BTreeMap::new(),
        proposals: BTreeMap::new(),
        concepts: BTreeMap::new(),
        cases: BTreeMap::new(),
        memories: BTreeMap::new(),
        last_event: 0,
        exclusions: BTreeMap::new(),
        heard: BTreeMap::new(),
    }
}
fn labor() -> InstitutionalRule {
    InstitutionalRule {
        subjects: Subjects::named(PersonId(1)),
        when: RuleExpression::Fact(SocialCondition::Member),
        effect: SocialEffect::Labor {
            site: SiteId(20),
            millis: 60_000,
            beneficiary: Beneficiary::Person(PersonId(2)),
            due_after: SimDuration(180_000),
        },
        authority: 0,
    }
}
fn charter(rules: Vec<InstitutionalRule>) -> InstitutionCharter {
    InstitutionCharter {
        purpose: "Chosen work".into(),
        jurisdiction: AreaId(3),
        founders: BTreeSet::from([PersonId(1), PersonId(2)]),
        open_membership: true,
        procedure: DecisionProcedure {
            proposers: Subjects::members(),
            voters: Subjects::members(),
            minimum_support: 1,
        },
        offices: vec![OfficeMandate {
            slot: 0,
            holder: PersonId(2),
            inventories: BTreeSet::from([InventoryId(4)]),
            sites: BTreeSet::from([SiteId(20)]),
            powers: BTreeSet::from([OfficePower::Access, OfficePower::Labor]),
            minimum_support: 1,
        }],
        rules,
        contributions: Vec::new(),
        evidence: BTreeSet::new(),
    }
}
fn institution(id: u64, rules: Vec<InstitutionalRule>, status: InstitutionStatus) -> Institution {
    Institution {
        id: InstitutionId(id),
        origin: None,
        charter: charter(rules),
        members: BTreeSet::new(),
        declined: BTreeSet::new(),
        invited: BTreeSet::new(),
        status,
        concepts: Vec::new(),
        office_support: BTreeMap::new(),
        grants: BTreeMap::new(),
        applied: BTreeMap::new(),
        funding: Vec::new(),
        revision: 0,
    }
}
fn stock(amount: u64) -> StockKnowledge {
    StockKnowledge {
        inventory: InventoryId(4),
        area: AreaId(3),
        good: Good::Food,
        quantity: Quantity(amount),
        accessible: false,
        observed_at: SimTime(0),
        learned_at: SimTime(0),
        source: KnowledgeSource::Observed(PersonId(1)),
    }
}
#[test]
fn access_uses_personal_stock_threshold_and_actual_custodian_fact() {
    let mut known = Knowledge::default();
    let state = world();
    let c = charter(vec![]);
    let rule = InstitutionalRule {
        effect: SocialEffect::Access {
            inventory: InventoryId(4),
            allowed: true,
        },
        ..labor()
    };
    assert_eq!(state.rule_vote_score(&voter(&known), &c, &rule), 0);
    known.stocks.insert(InventoryId(4), stock(999));
    assert_eq!(state.rule_vote_score(&voter(&known), &c, &rule), 0);
    known.stocks.insert(InventoryId(4), stock(1_000));
    assert_eq!(state.rule_vote_score(&voter(&known), &c, &rule), 1_200);
    known.custodians.insert(InventoryId(4), PersonId(1));
    assert_eq!(state.rule_vote_score(&voter(&known), &c, &rule), 900);
    let denied = InstitutionalRule {
        effect: SocialEffect::Access {
            inventory: InventoryId(4),
            allowed: false,
        },
        ..rule
    };
    assert_eq!(state.rule_vote_score(&voter(&known), &c, &denied), -1_200);
}
#[test]
fn labor_delivery_and_office_beneficiaries_keep_their_distinct_costs() {
    let known = Knowledge::default();
    let state = world();
    let mut c = charter(vec![]);
    let rule = labor();
    assert_eq!(state.rule_vote_score(&voter(&known), &c, &rule), -500);
    assert_eq!(
        state.rule_vote_score(
            &VoterFacts {
                supports_others: true,
                ..voter(&known)
            },
            &c,
            &rule
        ),
        -200
    );
    c.offices[0].holder = PersonId(1);
    let own = InstitutionalRule {
        when: RuleExpression::Fact(SocialCondition::Member),
        subjects: Subjects {
            members: false,
            named: BTreeSet::new(),
            offices: BTreeSet::from([0]),
        },
        effect: SocialEffect::Labor {
            site: SiteId(20),
            millis: 60_000,
            beneficiary: Beneficiary::Office(0),
            due_after: MINUTE,
        },
        authority: 0,
    };
    assert_eq!(state.rule_vote_score(&voter(&known), &c, &own), 200);
    let delivery = InstitutionalRule {
        effect: SocialEffect::Delivery {
            source: InventoryId(4),
            destination: InventoryId(5),
            quantity: Quantity(u64::MAX),
            beneficiary: Beneficiary::Person(PersonId(2)),
            due_after: MINUTE,
        },
        ..labor()
    };
    assert_eq!(
        state.rule_vote_score(&voter(&known), &c, &delivery),
        -100_000
    );
}
#[test]
fn learning_uses_only_latest32_effect_matches_and_clamps_in_both_directions() {
    let mut state = world();
    let rule = labor();
    let actor = PersonId(1);
    for id in 1..=33 {
        state.concepts.insert(ConceptId(id), rule.clone());
        state.memories.entry(actor).or_default().values.insert(
            ConceptId(id),
            ConceptValue {
                weight: if id == 1 { 2_000 } else { 1 },
                experiences: 1,
            },
        );
    }
    assert_eq!(state.learned_rule_score(actor, &rule), 32);
    let changed_amount = InstitutionalRule {
        effect: SocialEffect::Labor {
            site: SiteId(20),
            millis: 120_000,
            beneficiary: Beneficiary::Person(actor),
            due_after: MINUTE,
        },
        ..rule.clone()
    };
    assert_eq!(state.learned_rule_score(actor, &changed_amount), 32);
    let unrelated = InstitutionalRule {
        effect: SocialEffect::Access {
            inventory: InventoryId(4),
            allowed: true,
        },
        ..rule.clone()
    };
    assert_eq!(state.learned_rule_score(actor, &unrelated), 0);
    for value in state.memories.get_mut(&actor).unwrap().values.values_mut() {
        value.weight = 2_000;
    }
    assert_eq!(state.learned_rule_score(actor, &rule), 4_000);
    for value in state.memories.get_mut(&actor).unwrap().values.values_mut() {
        value.weight = -2_000;
    }
    assert_eq!(state.learned_rule_score(actor, &rule), -4_000);
}
#[test]
fn charter_trust_and_promised_contributions_preserve_signed_scores() {
    let mut known = Knowledge::default();
    known.trust.insert(PersonId(2), 150);
    let state = world();
    let mut c = charter(vec![]);
    c.contributions.push(AgreementTerm::Labor {
        debtor: PersonId(1),
        creditor: PersonId(2),
        site: SiteId(20),
        millis: 60_000,
        deadline: SimTime(60_000),
    });
    assert_eq!(state.charter_vote_score(&voter(&known), &c), -350);
    c.contributions[0] = AgreementTerm::Delivery {
        debtor: PersonId(2),
        creditor: PersonId(1),
        source: InventoryId(4),
        destination: InventoryId(5),
        quantity: Quantity(700),
        deadline: SimTime(60_000),
    };
    assert_eq!(state.charter_vote_score(&voter(&known), &c), 850);
}
#[test]
fn founder_replies_are_local_autonomous_unanswered_and_ordered_without_mutation() {
    let known = Knowledge::default();
    let mut state = world();
    state.institutions.insert(
        InstitutionId(11),
        institution(11, vec![labor()], InstitutionStatus::Proposed),
    );
    state.institutions.insert(
        InstitutionId(10),
        institution(10, vec![], InstitutionStatus::Proposed),
    );
    let before = state.clone();
    let replies = state.formation_replies(&[voter(&known)]).unwrap();
    assert_eq!(
        replies,
        vec![
            (
                PersonId(1),
                InstitutionAction::Join {
                    institution: InstitutionId(10)
                }
            ),
            (
                PersonId(1),
                InstitutionAction::Decline {
                    institution: InstitutionId(11)
                }
            )
        ]
    );
    assert_eq!(state, before);
    assert!(
        state
            .formation_replies(&[VoterFacts {
                local_area: None,
                ..voter(&known)
            }])
            .unwrap()
            .is_empty()
    );
    assert!(
        state
            .formation_replies(&[VoterFacts {
                autonomous: false,
                ..voter(&known)
            }])
            .unwrap()
            .is_empty()
    );
    assert!(
        state
            .formation_replies(&[voter(&known), voter(&known)])
            .is_err()
    );
    state
        .institutions
        .get_mut(&InstitutionId(10))
        .unwrap()
        .members
        .insert(PersonId(1));
    state
        .institutions
        .get_mut(&InstitutionId(11))
        .unwrap()
        .declined
        .insert(PersonId(1));
    assert!(
        state
            .formation_replies(&[voter(&known)])
            .unwrap()
            .is_empty()
    );
}
#[test]
fn real_institutional_labor_and_remembered_contribution_change_later_founder_response() {
    let known = Knowledge::default();
    let mut society = world();
    let actor = PersonId(1);
    let beneficiary = PersonId(2);
    let rule = labor();
    let mut active = institution(10, vec![rule.clone()], InstitutionStatus::Active);
    active.members = BTreeSet::from([actor, beneficiary]);
    active.concepts = vec![ConceptId(21)];
    active
        .office_support
        .insert(0, BTreeSet::from([beneficiary]));
    society.concepts.insert(ConceptId(21), rule.clone());
    society.institutions.insert(active.id, active);
    society.institutions.insert(
        InstitutionId(11),
        institution(11, vec![rule], InstitutionStatus::Proposed),
    );
    assert_eq!(
        society.formation_replies(&[voter(&known)]).unwrap(),
        vec![(
            actor,
            InstitutionAction::Decline {
                institution: InstitutionId(11)
            }
        )]
    );
    let authority = AuthorityFacts {
        alive: BTreeSet::from([actor, beneficiary]),
    };
    let stocks = BTreeMap::new();
    let intent = society
        .rule_intent(
            &society.institutions[&InstitutionId(10)],
            0,
            &ConditionInput {
                observer: beneficiary,
                subject: actor,
                subject_local: true,
                own_hunger: 0,
                known_stocks: &stocks,
            },
            &authority,
            true,
            SimTime(0),
        )
        .unwrap();
    let RuleIntent::Duty {
        institution,
        concept,
        ref term,
        ..
    } = intent
    else {
        panic!("labor rule creates a duty")
    };
    let mut agreements = game_agreements::State::default();
    agreements
        .prepare_institutional_duties(
            &[game_agreements::InstitutionDuty {
                institution,
                agreement: AgreementId(40),
                obligation: ObligationId(41),
                term: term.clone(),
            }],
            SimTime(0),
        )
        .unwrap()
        .commit(&mut agreements)
        .unwrap();
    society.register_duty(&intent, AgreementId(40)).unwrap();
    agreements
        .prepare_labor(
            SimTime(0),
            SimTime(60_000),
            &[game_agreements::WorkAssignment {
                actor,
                target: game_agreements::WorkTarget::Site(SiteId(20)),
            }],
        )
        .unwrap()
        .commit(&mut agreements)
        .unwrap();
    assert_eq!(agreements.obligations[&ObligationId(41)].performed, 60_000);
    society.remember_case(
        SocialCaseView {
            id: CaseId(30),
            kind: SocialCaseKind::Cooperation,
            subject: actor,
            counterpart: beneficiary,
            area: AreaId(3),
            inventory: None,
            good: Good::Timber,
            need_band: 0,
            reciprocal: false,
            outcome: SocialOutcome::Contributed,
            observed_at: SimTime(60_000),
        },
        Some(concept),
        &BTreeMap::from([(actor, 500)]),
    );
    assert_eq!(society.memories[&actor].values[&concept].weight, 500);
    assert_eq!(
        society.charter_vote_score(
            &voter(&known),
            &society.institutions[&InstitutionId(11)].charter
        ),
        0
    );
    assert_eq!(
        society.formation_replies(&[voter(&known)]).unwrap(),
        vec![(
            actor,
            InstitutionAction::Join {
                institution: InstitutionId(11)
            }
        )]
    );
}
