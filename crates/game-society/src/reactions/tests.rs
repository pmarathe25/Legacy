use super::*;
use crate::policy_fixtures::*;
#[test]
fn need_bands_and_known_food_exclusion_are_personal_bounded_and_not_renewed_without_change() {
    for (hunger, expected) in [(119_999, 0), (120_000, 1), (300_000, 2), (540_000, 3)] {
        assert_eq!(need_band(hunger), expected);
    }
    let mut state = world();
    let subject = SubjectFacts {
        actor: PersonId(1),
        area: Some(AreaId(3)),
        hunger: 120_000,
    };
    let known = BTreeMap::from([(InventoryId(4), stock(2_000))]);
    let owners = BTreeMap::from([(InventoryId(4), PersonId(2))]);
    let reactions = state.exclusion_reactions(subject, &known, &owners, SimTime(0));
    assert_eq!(reactions.len(), 1);
    state
        .commit_case_reaction(
            reactions[0].clone(),
            CaseId(30),
            &BTreeMap::from([(PersonId(1), 500)]),
        )
        .unwrap();
    assert!(
        state
            .exclusion_reactions(subject, &known, &owners, SimTime(60_000))
            .is_empty()
    );
    assert_eq!(
        state
            .exclusion_reactions(
                SubjectFacts {
                    hunger: 300_000,
                    ..subject
                },
                &known,
                &owners,
                SimTime(60_000)
            )
            .len(),
        1
    );
    let mut available = known;
    available.get_mut(&InventoryId(4)).unwrap().accessible = true;
    assert!(
        state
            .exclusion_reactions(subject, &available, &owners, SimTime(60_000))
            .is_empty()
    );
}
#[test]
fn one_real_labor_receipt_and_output_are_not_interpreted_as_two_contributions() {
    let mut state = world();
    let actor = PersonId(1);
    let counterpart = PersonId(2);
    let asset = CaseAsset {
        area: Some(AreaId(3)),
        inventory: InventoryId(4),
        good: Good::Food,
    };
    let mut ledger = game_agreements::State::default();
    let term = game_agreements::AgreementTerm::Labor {
        debtor: actor,
        creditor: counterpart,
        site: SiteId(20),
        millis: 60_000,
        deadline: SimTime(180_000),
    };
    ledger
        .prepare_institutional_duties(
            &[game_agreements::InstitutionDuty {
                institution: InstitutionId(10),
                agreement: AgreementId(40),
                obligation: ObligationId(41),
                term,
            }],
            SimTime(0),
        )
        .unwrap()
        .commit(&mut ledger)
        .unwrap();
    ledger
        .prepare_labor(
            SimTime(0),
            SimTime(60_000),
            &[game_agreements::WorkAssignment {
                actor,
                target: game_agreements::WorkTarget::Site(SiteId(20)),
            }],
        )
        .unwrap()
        .commit(&mut ledger)
        .unwrap();
    assert_eq!(ledger.obligations[&ObligationId(41)].performed, 60_000);
    let receipts = [
        SocialReceipt::Produced {
            actor,
            site: SiteId(20),
            asset,
            custodian: Some(counterpart),
        },
        SocialReceipt::Performed {
            actor,
            counterpart,
            agreement: AgreementId(40),
            target: CooperationTarget::Labor {
                site: SiteId(20),
                asset,
            },
        },
    ];
    let index = ReceiptIndex::new(&receipts);
    let facts = SubjectFacts {
        actor,
        area: Some(AreaId(3)),
        hunger: 0,
    };
    assert!(
        state
            .receipt_reaction(&receipts[0], facts, SimTime(60_000), &index)
            .is_none()
    );
    let reaction = state
        .receipt_reaction(&receipts[1], facts, SimTime(60_000), &index)
        .unwrap();
    state
        .commit_case_reaction(reaction, CaseId(30), &BTreeMap::from([(actor, 500)]))
        .unwrap();
    assert_eq!(
        state.known_case_count(actor, actor, SocialCaseKind::Cooperation),
        1
    );
}
#[test]
fn received_support_clears_only_its_exclusion_once_and_stale_guard_is_inert() {
    let mut state = world();
    state.cases.insert(
        CaseId(30),
        case(30, 1, SocialCaseKind::Exclusion, SocialOutcome::Unresolved),
    );
    state
        .exclusions
        .insert(PersonId(1), BTreeMap::from([(InventoryId(4), CaseId(30))]));
    let receipt = SocialReceipt::SupportReceived {
        actor: PersonId(1),
        source: InventoryId(4),
    };
    let index = ReceiptIndex::new(&[receipt]);
    let facts = SubjectFacts {
        actor: PersonId(1),
        area: Some(AreaId(3)),
        hunger: 300_000,
    };
    let reaction = state
        .receipt_reaction(&receipt, facts, SimTime(60_000), &index)
        .unwrap();
    let stale = reaction.clone();
    state
        .commit_case_reaction(reaction, CaseId(31), &BTreeMap::from([(PersonId(1), 500)]))
        .unwrap();
    assert!(
        state
            .receipt_reaction(&receipt, facts, SimTime(60_000), &index)
            .is_none()
    );
    assert!(
        state
            .commit_case_reaction(stale, CaseId(32), &BTreeMap::new())
            .is_err()
    );
    assert!(!state.cases.contains_key(&CaseId(32)));
    assert_eq!(state.cases[&CaseId(31)].outcome, SocialOutcome::Shared);
}
