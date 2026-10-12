use super::*;
use crate::policy_fixtures::*;
fn observer(id: u64, area: Option<AreaId>) -> ObserverFacts {
    ObserverFacts {
        actor: PersonId(id),
        area,
        alive: true,
        independent: true,
        conversation_allowed: true,
        empathy: 500,
    }
}
#[test]
fn case_witness_limit_keeps_roster_order_and_subject_even_when_absent_or_dead() {
    let mut roster = (1..=100)
        .map(|id| observer(id, Some(AreaId(3))))
        .collect::<Vec<_>>();
    roster[99].alive = false;
    roster[99].area = None;
    let index = ObservationIndex::new(&roster).unwrap();
    let c = case(30, 100, SocialCaseKind::Default, SocialOutcome::Defaulted);
    let public = index.case_observers(&c, true).unwrap();
    assert_eq!(public.len(), 65);
    assert!(public.contains_key(&PersonId(64)));
    assert!(!public.contains_key(&PersonId(65)));
    assert!(public.contains_key(&PersonId(100)));
    let private = index.case_observers(&c, false).unwrap();
    assert_eq!(
        private.keys().copied().collect::<Vec<_>>(),
        vec![PersonId(2), PersonId(100)]
    );
    assert!(ObservationIndex::new(&[roster[0], roster[0]]).is_err());
}
#[test]
fn local_publication_preserves_absent_memories_and_rejects_stale_plan_before_any_write() {
    let mut state = world();
    let mut i = institution(10, vec![], InstitutionStatus::Active);
    i.members = BTreeSet::from([PersonId(1), PersonId(2)]);
    i.office_support.insert(0, BTreeSet::from([PersonId(2)]));
    state.institutions.insert(i.id, i);
    let roster = [
        observer(1, Some(AreaId(3))),
        observer(2, Some(AreaId(3))),
        observer(3, Some(AreaId(3))),
    ];
    let index = ObservationIndex::new(&roster).unwrap();
    let authority = AuthorityFacts {
        alive: BTreeSet::from([PersonId(1), PersonId(2)]),
    };
    state
        .prepare_publication(InstitutionId(10), &authority, SimTime(10), &index)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(state.institution_views(PersonId(1))[0].offices[0].effective);
    state
        .institutions
        .get_mut(&InstitutionId(10))
        .unwrap()
        .charter
        .purpose = "Updated terms".into();
    let mut away = roster;
    away[0].area = None;
    let index = ObservationIndex::new(&away).unwrap();
    state
        .prepare_publication(InstitutionId(10), &authority, SimTime(20), &index)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        state.institution_views(PersonId(1))[0].observed_at,
        SimTime(10)
    );
    assert_eq!(
        state.institution_views(PersonId(2))[0].charter.purpose,
        "Updated terms"
    );
    let pending = state
        .prepare_publication(InstitutionId(10), &authority, SimTime(30), &index)
        .unwrap();
    state
        .institutions
        .get_mut(&InstitutionId(10))
        .unwrap()
        .revision += 1;
    let old = state.institution_views(PersonId(2));
    assert!(pending.commit(&mut state).is_err());
    assert_eq!(state.institution_views(PersonId(2)), old);
}
#[test]
fn closed_publication_and_command_opportunities_use_only_local_known_institutions() {
    let mut state = world();
    let mut i = institution(10, vec![], InstitutionStatus::Active);
    i.charter.open_membership = false;
    i.members = BTreeSet::from([PersonId(1), PersonId(2)]);
    state.institutions.insert(i.id, i);
    let roster = [
        observer(1, Some(AreaId(3))),
        observer(2, Some(AreaId(3))),
        observer(3, Some(AreaId(3))),
    ];
    let index = ObservationIndex::new(&roster).unwrap();
    let authority = AuthorityFacts {
        alive: BTreeSet::from([PersonId(1), PersonId(2)]),
    };
    state
        .prepare_publication(InstitutionId(10), &authority, SimTime(0), &index)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(state.institution_views(PersonId(3)).is_empty());
    assert!(
        state
            .institution_opportunities(PersonId(3), AreaId(3), &authority)
            .is_empty()
    );
    assert_eq!(
        state.institution_opportunities(PersonId(1), AreaId(3), &authority),
        vec![
            InstitutionAction::Leave {
                institution: InstitutionId(10)
            },
            InstitutionAction::SupportOffice {
                institution: InstitutionId(10),
                office: 0,
                support: true
            }
        ]
    );
    assert!(
        state
            .institution_opportunities(PersonId(1), AreaId(4), &authority)
            .is_empty()
    );
}
