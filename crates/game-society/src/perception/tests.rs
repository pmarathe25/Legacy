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
    let (state, mut store) = world();
    let mut i = institution(10, vec![], InstitutionStatus::Active);
    i.members = BTreeSet::from([PersonId(1), PersonId(2)]);
    i.office_support.insert(0, BTreeSet::from([PersonId(2)]));
    seed_institution(&mut store, i);
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
        .prepare_publication(&store, InstitutionId(10), &authority, SimTime(10), &index)
        .unwrap()
        .commit(&state, &mut store)
        .unwrap();
    assert!(state.institution_views(&store, PersonId(1))[0].offices[0].effective);
    crate::storage::institution_mut(&mut store, InstitutionId(10))
        .unwrap()
        .charter
        .purpose = "Updated terms".into();
    let mut away = roster;
    away[0].area = None;
    let index = ObservationIndex::new(&away).unwrap();
    state
        .prepare_publication(&store, InstitutionId(10), &authority, SimTime(20), &index)
        .unwrap()
        .commit(&state, &mut store)
        .unwrap();
    assert_eq!(
        state.institution_views(&store, PersonId(1))[0].observed_at,
        SimTime(10)
    );
    assert_eq!(
        state.institution_views(&store, PersonId(2))[0]
            .charter
            .purpose,
        "Updated terms"
    );
    let pending = state
        .prepare_publication(&store, InstitutionId(10), &authority, SimTime(30), &index)
        .unwrap();
    crate::storage::institution_mut(&mut store, InstitutionId(10))
        .unwrap()
        .revision += 1;
    let old = state.institution_views(&store, PersonId(2));
    assert!(pending.commit(&state, &mut store).is_err());
    assert_eq!(state.institution_views(&store, PersonId(2)), old);
}
#[test]
fn closed_publication_and_command_opportunities_use_only_local_known_institutions() {
    let (state, mut store) = world();
    let mut i = institution(10, vec![], InstitutionStatus::Active);
    i.charter.open_membership = false;
    i.members = BTreeSet::from([PersonId(1), PersonId(2)]);
    seed_institution(&mut store, i);
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
        .prepare_publication(&store, InstitutionId(10), &authority, SimTime(0), &index)
        .unwrap()
        .commit(&state, &mut store)
        .unwrap();
    assert!(state.institution_views(&store, PersonId(3)).is_empty());
    assert!(
        state
            .institution_opportunities(&store, PersonId(3), AreaId(3), &authority)
            .is_empty()
    );
    assert_eq!(
        state.institution_opportunities(&store, PersonId(1), AreaId(3), &authority),
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
            .institution_opportunities(&store, PersonId(1), AreaId(4), &authority)
            .is_empty()
    );
}
fn dated_report(id: u64, at: u64, purpose: &str) -> InstitutionView {
    let row = institution(id, vec![], InstitutionStatus::Active);
    let mut charter = row.charter;
    charter.purpose = purpose.into();
    InstitutionView {
        id: row.id,
        origin: None,
        charter,
        members: BTreeSet::new(),
        declined: BTreeSet::new(),
        status: row.status,
        offices: vec![],
        proposals: vec![],
        funding: vec![],
        duties: vec![],
        observed_at: SimTime(at),
    }
}
#[test]
fn reports_preserve_unrelated_learning_and_choose_equal_date_policy_explicitly() {
    for (tie, expected) in [
        (ReportTie::KeepCurrent, "old"),
        (ReportTie::Receive, "incoming"),
    ] {
        let (_, mut store) = world();
        store
            .insert_or_spawn(StableId(1), crate::Knowledge::default())
            .unwrap();
        crate::knowledge_mut(&mut store, PersonId(1))
            .unwrap()
            .institutions
            .insert(InstitutionId(10), dated_report(10, 5, "old"));
        let plan = prepare_institution_reports_with_ties(
            &store,
            PersonId(1),
            &[
                dated_report(10, 5, "incoming"),
                dated_report(10, 4, "older"),
            ],
            tie,
        )
        .unwrap();
        let mut batch = Batch::new();
        let guard = plan.stage_into(&store, &mut batch).unwrap();
        let unrelated = dated_report(11, 8, "independent");
        crate::knowledge_mut(&mut store, PersonId(1))
            .unwrap()
            .institutions
            .insert(InstitutionId(11), unrelated.clone());
        guard.validate_source(&store, &batch).unwrap();
        store.apply_batch(batch).unwrap();
        guard.finish(&mut store);
        let known = crate::knowledge(&store, PersonId(1)).unwrap();
        assert_eq!(
            known.institutions[&InstitutionId(10)].charter.purpose,
            expected
        );
        assert_eq!(known.institutions[&InstitutionId(11)], unrelated);
    }
}
#[test]
fn report_final_guard_rejects_selected_source_replacement_and_conflicting_row_edit() {
    let (_, mut store) = world();
    store
        .insert_or_spawn(StableId(1), crate::Knowledge::default())
        .unwrap();
    let mut batch = Batch::new();
    let guard =
        prepare_institution_reports(&store, PersonId(1), &[dated_report(10, 5, "incoming")])
            .unwrap()
            .stage_into(&store, &mut batch)
            .unwrap();
    crate::knowledge_mut(&mut store, PersonId(1))
        .unwrap()
        .institutions
        .insert(InstitutionId(10), dated_report(10, 6, "newer"));
    assert!(guard.validate_source(&store, &batch).is_err());
    crate::knowledge_mut(&mut store, PersonId(1))
        .unwrap()
        .institutions
        .remove(&InstitutionId(10));
    guard.validate_source(&store, &batch).unwrap();
    batch.insert(StableId(1), crate::Knowledge::default());
    assert!(guard.validate_source(&store, &batch).is_err());
    assert!(
        !crate::knowledge(&store, PersonId(1))
            .unwrap()
            .institutions
            .contains_key(&InstitutionId(10))
    );
}
#[test]
fn absent_report_recipient_is_created_only_in_the_admitted_batch() {
    let (_, mut store) = world();
    let mut batch = Batch::new();
    let guard =
        prepare_institution_reports(&store, PersonId(99), &[dated_report(10, 5, "incoming")])
            .unwrap()
            .stage_into(&store, &mut batch)
            .unwrap();
    assert!(crate::knowledge(&store, PersonId(99)).is_none());
    assert!(!store.contains(StableId(99)));
    guard.validate_source(&store, &batch).unwrap();
    store.apply_batch(batch).unwrap();
    guard.finish(&mut store);
    assert_eq!(
        crate::knowledge(&store, PersonId(99)).unwrap().institutions[&InstitutionId(10)]
            .observed_at,
        SimTime(5)
    );
}

#[test]
fn malformed_report_identity_is_rejected_before_any_receiving_write() {
    let (_, store) = world();
    let original = store.snapshot().unwrap();
    assert!(
        prepare_institution_reports(&store, PersonId(1), &[dated_report(0, 5, "invalid")]).is_err()
    );
    assert!(prepare_institution_reports(&store, PersonId(0), &[]).is_err());
    assert_eq!(store.snapshot().unwrap(), original);
}
