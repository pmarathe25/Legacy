use super::*;
use crate::policy_fixtures::*;
fn setup() -> (World, AuthorityFacts) {
    let mut state = world();
    let mut institution = institution(
        10,
        vec![InstitutionalRule {
            effect: SocialEffect::Access {
                inventory: InventoryId(4),
                allowed: true,
            },
            ..labor()
        }],
        InstitutionStatus::Active,
    );
    institution.members = BTreeSet::from([PersonId(1), PersonId(2)]);
    institution.charter.offices[0]
        .powers
        .insert(OfficePower::HearAccessCases);
    institution
        .office_support
        .insert(0, BTreeSet::from([PersonId(2)]));
    institution.concepts = vec![ConceptId(21)];
    state
        .concepts
        .insert(ConceptId(21), institution.charter.rules[0].clone());
    state.institutions.insert(institution.id, institution);
    state.cases.insert(
        CaseId(30),
        case(30, 1, SocialCaseKind::Exclusion, SocialOutcome::Unresolved),
    );
    state
        .memories
        .entry(PersonId(2))
        .or_default()
        .cases
        .insert(CaseId(30));
    state.memories.entry(PersonId(1)).or_default();
    (
        state,
        AuthorityFacts {
            alive: BTreeSet::from([PersonId(1), PersonId(2)]),
        },
    )
}
#[test]
fn hearing_grant_is_scoped_and_competing_prepared_decision_is_rejected_atomically() {
    let (mut state, authority) = setup();
    let observers = BTreeMap::new();
    let action = InstitutionAction::HearCase {
        institution: InstitutionId(10),
        office: 0,
        case: CaseId(30),
        share: true,
    };
    let facts = || CaseCommandFacts {
        actor: PersonId(2),
        area: AreaId(3),
        now: SimTime(0),
        recipient_area: None,
        authority: &authority,
        refusal: None,
        refusal_observers: &observers,
    };
    let first = state.prepare_case_command(&action, facts()).unwrap();
    let stale = state.prepare_case_command(&action, facts()).unwrap();
    first.commit(&mut state).unwrap();
    assert!(state.institutions[&InstitutionId(10)].grants[&ConceptId(21)].contains(&PersonId(1)));
    assert!(state.heard[&CaseId(30)]);
    assert!(stale.commit(&mut state).is_err());
    assert_eq!(state.heard.len(), 1);
    assert_eq!(state.cases.len(), 1);
}
#[test]
fn refusal_uses_supplied_case_id_and_private_witnesses_but_judge_remembers_decision() {
    let (mut state, authority) = setup();
    let observers = BTreeMap::from([(PersonId(1), 500)]);
    let action = InstitutionAction::HearCase {
        institution: InstitutionId(10),
        office: 0,
        case: CaseId(30),
        share: false,
    };
    let plan = state
        .prepare_case_command(
            &action,
            CaseCommandFacts {
                actor: PersonId(2),
                area: AreaId(3),
                now: SimTime(10),
                recipient_area: None,
                authority: &authority,
                refusal: Some(CaseId(31)),
                refusal_observers: &observers,
            },
        )
        .unwrap();
    let effect = plan.commit(&mut state).unwrap();
    assert_eq!(effect.recorded, Some((CaseId(31), PersonId(1))));
    assert_eq!(state.cases[&CaseId(31)].outcome, SocialOutcome::Refused);
    assert!(state.memories[&PersonId(2)].cases.contains(&CaseId(31)));
    assert!(state.institutions[&InstitutionId(10)].grants.is_empty());
}
#[test]
fn telling_requires_hearing_contact_personal_knowledge_and_keeps_case_contents() {
    let (mut state, authority) = setup();
    let observers = BTreeMap::new();
    let action = InstitutionAction::TellCase {
        recipient: PersonId(1),
        case: CaseId(30),
    };
    assert!(
        state
            .prepare_case_command(
                &action,
                CaseCommandFacts {
                    actor: PersonId(2),
                    area: AreaId(3),
                    now: SimTime(10),
                    recipient_area: Some(AreaId(4)),
                    authority: &authority,
                    refusal: None,
                    refusal_observers: &observers
                }
            )
            .is_err()
    );
    let original = state.cases[&CaseId(30)].clone();
    state
        .prepare_case_command(
            &action,
            CaseCommandFacts {
                actor: PersonId(2),
                area: AreaId(3),
                now: SimTime(10),
                recipient_area: Some(AreaId(3)),
                authority: &authority,
                refusal: None,
                refusal_observers: &observers,
            },
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(state.cases[&CaseId(30)], original);
    assert!(state.memories[&PersonId(1)].cases.contains(&CaseId(30)));
}
