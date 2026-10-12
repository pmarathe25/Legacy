use super::*;
use crate::policy_fixtures::*;
use crate::policy_fixtures::{case, institution};
fn setup() -> (Runtime, Store, AuthorityFacts) {
    let (state, mut store) = world();
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
    crate::storage::insert_concept(
        &mut store,
        ConceptId(21),
        crate::storage::Concept {
            rule: institution.charter.rules[0].clone(),
        },
    )
    .unwrap();
    seed_institution(&mut store, institution);
    crate::storage::insert_case(
        &mut store,
        CaseId(30),
        case(30, 1, SocialCaseKind::Exclusion, SocialOutcome::Unresolved),
    )
    .unwrap();
    ensure_memory(&mut store, PersonId(2))
        .cases
        .insert(CaseId(30));
    ensure_memory(&mut store, PersonId(1));
    (
        state,
        store,
        AuthorityFacts {
            alive: BTreeSet::from([PersonId(1), PersonId(2)]),
        },
    )
}
#[test]
fn hearing_grant_is_scoped_and_competing_prepared_decision_is_rejected_atomically() {
    let (state, mut store, authority) = setup();
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
    let first = state
        .prepare_case_command(&store, &action, facts())
        .unwrap();
    let stale = state
        .prepare_case_command(&store, &action, facts())
        .unwrap();
    first.commit(&state, &mut store).unwrap();
    assert!(
        crate::storage::institution(&store, InstitutionId(10))
            .unwrap()
            .grants[&ConceptId(21)]
            .contains(&PersonId(1))
    );
    assert!(crate::storage::hearing(&store, CaseId(30)).unwrap().shared);
    assert!(stale.commit(&state, &mut store).is_err());
    assert_eq!(crate::storage::hearings(&store).count(), 1);
    assert_eq!(crate::storage::cases(&store).count(), 1);
}
#[test]
fn refusal_uses_supplied_case_id_and_private_witnesses_but_judge_remembers_decision() {
    let (state, mut store, authority) = setup();
    let observers = BTreeMap::from([(PersonId(1), 500)]);
    let action = InstitutionAction::HearCase {
        institution: InstitutionId(10),
        office: 0,
        case: CaseId(30),
        share: false,
    };
    let plan = state
        .prepare_case_command(
            &store,
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
    let effect = plan.commit(&state, &mut store).unwrap();
    assert_eq!(effect.recorded, Some((CaseId(31), PersonId(1))));
    assert_eq!(
        crate::storage::case(&store, CaseId(31)).unwrap().outcome,
        SocialOutcome::Refused
    );
    assert!(
        crate::storage::knowledge(&store, PersonId(2))
            .unwrap()
            .cases
            .contains(&CaseId(31))
    );
    assert!(
        crate::storage::institution(&store, InstitutionId(10))
            .unwrap()
            .grants
            .is_empty()
    );
}
#[test]
fn telling_requires_hearing_contact_personal_knowledge_and_keeps_case_contents() {
    let (state, mut store, authority) = setup();
    let observers = BTreeMap::new();
    let action = InstitutionAction::TellCase {
        recipient: PersonId(1),
        case: CaseId(30),
    };
    assert!(
        state
            .prepare_case_command(
                &store,
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
    let original = crate::storage::case(&store, CaseId(30)).unwrap().clone();
    state
        .prepare_case_command(
            &store,
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
        .commit(&state, &mut store)
        .unwrap();
    assert_eq!(crate::storage::case(&store, CaseId(30)).unwrap(), &original);
    assert!(
        crate::storage::knowledge(&store, PersonId(1))
            .unwrap()
            .cases
            .contains(&CaseId(30))
    );
}
