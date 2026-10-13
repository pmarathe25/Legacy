//! Production rule/case entry points compose with the host without the assembly.
use crate::{institutions::*, policy_fixtures, rule_publication::*};
use game_core::*;
use game_ecs::{Batch, Schema, StableId};
use game_sim::ClockWorld;
use game_world::{GenericHost, Transaction};
use std::collections::{BTreeMap, BTreeSet};
type Host = GenericHost<Runtime, &'static str>;
#[derive(Clone, Copy)]
enum Reject {
    None,
    Duty,
    Agreement,
    Knowledge,
    Publication,
    Case,
    Exclusion,
}
fn schema(reject: Reject) -> Schema {
    let mut schema = Schema::new();
    let institution: fn(&Institution) -> Result<(), Error> = if matches!(reject, Reject::Duty) {
        |row| {
            if !row.applied.is_empty() {
                Err(Error::Unavailable)
            } else {
                row.validate_shape()
            }
        }
    } else {
        Institution::validate_shape
    };
    let knowledge: fn(&crate::Knowledge) -> Result<(), Error> =
        if matches!(reject, Reject::Knowledge) {
            |row| {
                if !row.cases.is_empty() || !row.institutions.is_empty() {
                    Err(Error::Unavailable)
                } else {
                    row.validate_shape()
                }
            }
        } else if matches!(reject, Reject::Publication) {
            |row| {
                if !row.institutions.is_empty() {
                    Err(Error::Unavailable)
                } else {
                    row.validate_shape()
                }
            }
        } else {
            crate::Knowledge::validate_shape
        };
    let case: fn(&SocialCaseView) -> Result<(), Error> = if matches!(reject, Reject::Case) {
        |_| Err(Error::Unavailable)
    } else {
        SocialCaseView::validate_shape
    };
    let exclusions: fn(&crate::Exclusions) -> Result<(), Error> =
        if matches!(reject, Reject::Exclusion) {
            |row| {
                if !row.stores.is_empty() {
                    Err(Error::Unavailable)
                } else {
                    Ok(())
                }
            }
        } else {
            |_| Ok(())
        };
    schema
        .register::<Institution>("society/institution", institution)
        .unwrap();
    schema
        .register::<RuleProposal>("society/proposal", |_| Ok(()))
        .unwrap();
    schema
        .register::<crate::Concept>("society/concept", |row| {
            crate::validate_rule_shape(&row.rule)
        })
        .unwrap();
    schema
        .register::<SocialCaseView>("society/case", case)
        .unwrap();
    schema
        .register::<crate::Knowledge>("society/knowledge", knowledge)
        .unwrap();
    schema
        .register::<crate::Requests>("society/requests", |_| Ok(()))
        .unwrap();
    schema
        .register::<crate::Exclusions>("society/exclusions", exclusions)
        .unwrap();
    schema
        .register::<crate::Hearing>("society/hearing", |_| Ok(()))
        .unwrap();
    let agreement: fn(&game_agreements::Agreement) -> Result<(), Error> =
        if matches!(reject, Reject::Agreement) {
            |_| Err(Error::Unavailable)
        } else {
            game_agreements::Agreement::validate
        };
    schema
        .register::<game_agreements::Agreement>("agreements/agreement", agreement)
        .unwrap();
    schema
        .register::<game_agreements::Obligation>(
            "agreements/obligation",
            game_agreements::Obligation::validate,
        )
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
    game_property::register_components(&mut schema).unwrap();
    schema
}
fn fixture(reject: Reject) -> Host {
    let mut host = Host::new(schema(reject), 1, Runtime::default());
    host.allocate_ids(1_999).unwrap();
    let mut institution = policy_fixtures::institution(
        10,
        vec![policy_fixtures::labor()],
        InstitutionStatus::Active,
    );
    institution.members = BTreeSet::from([PersonId(1), PersonId(2)]);
    institution.office_support = BTreeMap::from([(0, institution.members.clone())]);
    policy_fixtures::seed_institution(&mut host.storage, institution);
    for actor in [PersonId(1), PersonId(2)] {
        crate::insert_knowledge(&mut host.storage, actor, crate::Knowledge::default()).unwrap();
    }
    crate::insert_proposal(
        &mut host.storage,
        RuleProposalId(30),
        RuleProposal {
            id: RuleProposalId(30),
            institution: InstitutionId(10),
            proposer: PersonId(2),
            change: InstitutionChange::Procedure(policy_fixtures::charter(vec![]).procedure),
            votes: BTreeMap::new(),
            base_revision: 0,
            status: RuleProposalStatus::Proposed,
        },
    )
    .unwrap();
    host.storage.ensure(StableId(4)).unwrap();
    game_property::declare_custody(&mut host.storage, InventoryId(4), PersonId(2)).unwrap();
    host
}
fn observers() -> [crate::perception::ObserverFacts; 2] {
    [PersonId(1), PersonId(2)].map(|actor| crate::perception::ObserverFacts {
        actor,
        area: Some(AreaId(3)),
        alive: true,
        independent: true,
        conversation_allowed: true,
        empathy: 500,
    })
}
fn stage_rules(
    host: &Host,
) -> (
    Batch,
    RuleApplicationPublication,
    u64,
    Vec<RuleApplicationEffect>,
) {
    let authority = AuthorityFacts {
        alive: BTreeSet::from([PersonId(1), PersonId(2)]),
    };
    let known = BTreeMap::new();
    let speakers = [crate::deliberation::RuleSpeaker {
        actor: PersonId(2),
        area: Some(AreaId(3)),
        hunger: 0,
        known_stocks: &known,
    }];
    let applications = host
        .state
        .rule_applications(
            &host.storage,
            &authority,
            &speakers,
            &BTreeMap::from([(PersonId(1), AreaId(3)), (PersonId(2), AreaId(3))]),
            host.clock().time,
        )
        .unwrap();
    assert_eq!(applications.len(), 1);
    let prepared = host
        .state
        .prepare_rule_application(
            &host.storage,
            &applications[0],
            host.next_id(),
            host.clock().time,
        )
        .unwrap();
    let ids = prepared.next_id - host.next_id();
    let effects = prepared.effects.clone();
    let roster = observers();
    let index = crate::perception::ObservationIndex::new(&roster).unwrap();
    let publications = prepared
        .prepare_publications(
            &host.state,
            &host.storage,
            &authority,
            host.clock().time,
            &index,
        )
        .unwrap();
    let mut batch = Batch::new();
    let publication = prepared
        .stage_with_publications(
            &host.state,
            &host.storage,
            host.clock().time,
            publications,
            &mut batch,
        )
        .unwrap();
    (batch, publication, ids, effects)
}
fn stage_case(host: &Host) -> (Batch, crate::reactions::CaseReactionPublication) {
    let known = BTreeMap::from([(InventoryId(4), policy_fixtures::stock(2_000))]);
    let reactions = host.state.exclusion_reactions(
        &host.storage,
        crate::reactions::SubjectFacts {
            actor: PersonId(1),
            area: Some(AreaId(3)),
            hunger: 120_000,
        },
        &known,
        host.clock().time,
    );
    assert_eq!(reactions.len(), 1);
    let reaction = reactions.into_iter().next().unwrap();
    let roster = observers();
    let index = crate::perception::ObservationIndex::new(&roster).unwrap();
    let observers = index
        .case_observers(&reaction.case, reaction.public)
        .unwrap();
    let prepared = host
        .state
        .prepare_case_reaction(&host.storage, reaction, CaseId(host.next_id()), &observers)
        .unwrap();
    let mut batch = Batch::new();
    let publication = prepared
        .stage_into(&host.state, &host.storage, host.clock().time, &mut batch)
        .unwrap();
    (batch, publication)
}
#[test]
fn rule_agreement_registration_and_receiving_publication_rejections_leave_all_state_unchanged() {
    for reject in [Reject::Duty, Reject::Agreement, Reject::Knowledge] {
        let mut host = fixture(reject);
        let before = host.snapshot().unwrap();
        let clock = host.clock();
        let (batch, publication, ids, effects) = stage_rules(&host);
        assert_eq!(ids, 2);
        assert_eq!(effects.len(), 1);
        publication
            .validate(&host.state, &host.storage, host.clock().time, &batch)
            .unwrap();
        let mut transaction = Transaction::new(host.transaction_opening(), batch);
        transaction.ids = ids;
        transaction.events = vec!["agreement activated", "rule applied"];
        transaction.guards = publication.row_guards();
        assert_eq!(
            host.commit_transaction(transaction),
            Err(Error::Unavailable)
        );
        assert_eq!(host.snapshot().unwrap(), before);
        assert_eq!(host.clock(), clock);
        assert!(game_agreements::agreements(&host.storage).next().is_none());
        assert!(
            crate::institution(&host.storage, InstitutionId(10))
                .unwrap()
                .applied
                .is_empty()
        );
    }
}
#[test]
fn recognized_duty_and_actual_agreement_publish_once_with_their_observed_view() {
    let mut host = fixture(Reject::None);
    let (batch, publication, ids, effects) = stage_rules(&host);
    publication
        .validate(&host.state, &host.storage, host.clock().time, &batch)
        .unwrap();
    assert!(matches!(
        effects.as_slice(),
        [RuleApplicationEffect::Duty {
            agreement: AgreementId(2_000),
            actor: PersonId(1),
            ..
        }]
    ));
    let mut transaction = Transaction::new(host.transaction_opening(), batch);
    transaction.ids = ids;
    transaction.events = vec!["agreement activated", "rule applied"];
    transaction.guards = publication.row_guards();
    host.commit_transaction(transaction).unwrap();
    let institution = crate::institution(&host.storage, InstitutionId(10)).unwrap();
    let concept = institution.concepts[0];
    assert_eq!(
        institution.applied[&concept][&PersonId(1)],
        AgreementId(2_000)
    );
    assert_eq!(
        game_agreements::obligation(&host.storage, ObligationId(2_001))
            .unwrap()
            .agreement,
        AgreementId(2_000)
    );
    assert_eq!(host.next_id(), 2_002);
    assert_eq!(host.events().len(), 2);
    assert_eq!(
        crate::knowledge(&host.storage, PersonId(1))
            .unwrap()
            .institutions[&InstitutionId(10)]
            .duties,
        vec![AgreementId(2_000)]
    );
    let (batch, publication, ids, effects) = stage_rules(&host);
    assert_eq!(ids, 0);
    assert!(effects.is_empty());
    publication
        .validate(&host.state, &host.storage, host.clock().time, &batch)
        .unwrap();
}
#[test]
fn case_learning_and_exclusion_schema_failures_do_not_consume_ids_or_publish_events() {
    for reject in [Reject::Case, Reject::Knowledge, Reject::Exclusion] {
        let mut host = fixture(reject);
        let before = host.snapshot().unwrap();
        let clock = host.clock();
        let (batch, publication) = stage_case(&host);
        publication
            .validate(&host.state, &host.storage, host.clock().time, &batch)
            .unwrap();
        let mut transaction = Transaction::new(host.transaction_opening(), batch);
        transaction.ids = 1;
        transaction.events = vec!["case recorded"];
        transaction.guards.push(publication.row_guard());
        assert_eq!(
            host.commit_transaction(transaction),
            Err(Error::Unavailable)
        );
        assert_eq!(host.snapshot().unwrap(), before);
        assert_eq!(host.clock(), clock);
        assert!(crate::case(&host.storage, CaseId(2_000)).is_none());
        assert!(crate::exclusions(&host.storage, PersonId(1)).is_none());
    }
}
#[test]
fn case_learning_and_exclusion_publish_together_with_the_allocation() {
    let mut host = fixture(Reject::None);
    let (batch, publication) = stage_case(&host);
    publication
        .validate(&host.state, &host.storage, host.clock().time, &batch)
        .unwrap();
    let mut transaction = Transaction::new(host.transaction_opening(), batch);
    transaction.ids = 1;
    transaction.events = vec!["case recorded"];
    transaction.guards.push(publication.row_guard());
    host.commit_transaction(transaction).unwrap();
    assert!(
        crate::knowledge(&host.storage, PersonId(1))
            .unwrap()
            .cases
            .contains(&CaseId(2_000))
    );
    assert_eq!(
        crate::exclusions(&host.storage, PersonId(1))
            .unwrap()
            .stores[&InventoryId(4)],
        CaseId(2_000)
    );
    assert_eq!(host.next_id(), 2_001);
    assert_eq!(host.next_event(), 2);
}

#[test]
fn rule_publication_rejects_stale_sources_runtime_and_late_foreign_creation_shapes() {
    let host = fixture(Reject::None);
    let (mut batch, publication, _, _) = stage_rules(&host);
    let mut runtime = host.state.clone();
    runtime.last_event += 1;
    assert!(
        publication
            .validate(&runtime, &host.storage, host.clock().time, &batch)
            .is_err()
    );
    batch.insert(StableId(2_000), crate::Requests::default());
    assert!(
        publication
            .validate(&host.state, &host.storage, host.clock().time, &batch)
            .is_err()
    );
    let mut host = fixture(Reject::None);
    let (batch, publication, _, _) = stage_rules(&host);
    crate::concept_row_mut(&mut host.storage, ConceptId(1_000))
        .unwrap()
        .rule
        .when = RuleExpression::Fact(SocialCondition::Local);
    assert!(
        publication
            .validate(&host.state, &host.storage, host.clock().time, &batch)
            .is_err()
    );
}
#[test]
fn case_publication_rejects_runtime_time_custody_and_late_fresh_entity_edits() {
    let host = fixture(Reject::None);
    let (mut batch, publication) = stage_case(&host);
    let mut runtime = host.state.clone();
    runtime.last_event += 1;
    assert!(
        publication
            .validate(&runtime, &host.storage, host.clock().time, &batch)
            .is_err()
    );
    assert!(
        publication
            .validate(&host.state, &host.storage, SimTime(1), &batch)
            .is_err()
    );
    batch.insert(StableId(2_000), crate::Requests::default());
    assert!(
        publication
            .validate(&host.state, &host.storage, host.clock().time, &batch)
            .is_err()
    );
    let mut host = fixture(Reject::None);
    let (batch, publication) = stage_case(&host);
    game_property::declare_custody(&mut host.storage, InventoryId(4), PersonId(1)).unwrap();
    assert!(
        publication
            .validate(&host.state, &host.storage, host.clock().time, &batch)
            .is_err()
    );
}

#[test]
fn exhausted_case_allocation_and_duty_id_preparation_leave_every_row_and_cursor_intact() {
    let mut host = fixture(Reject::None);
    host.allocate_ids(u64::MAX - host.next_id()).unwrap();
    let before = host.snapshot().unwrap();
    let clock = host.clock();
    let (batch, publication) = stage_case(&host);
    publication
        .validate(&host.state, &host.storage, host.clock().time, &batch)
        .unwrap();
    let mut transaction = Transaction::new(host.transaction_opening(), batch);
    transaction.ids = 1;
    transaction.events = vec!["case recorded"];
    transaction.guards.push(publication.row_guard());
    assert_eq!(host.commit_transaction(transaction), Err(Error::Overflow));
    assert_eq!(host.snapshot().unwrap(), before);
    assert_eq!(host.clock(), clock);
    let authority = AuthorityFacts {
        alive: BTreeSet::from([PersonId(1), PersonId(2)]),
    };
    let known = BTreeMap::new();
    let speakers = [crate::deliberation::RuleSpeaker {
        actor: PersonId(2),
        area: Some(AreaId(3)),
        hunger: 0,
        known_stocks: &known,
    }];
    let applications = host
        .state
        .rule_applications(
            &host.storage,
            &authority,
            &speakers,
            &BTreeMap::from([(PersonId(1), AreaId(3)), (PersonId(2), AreaId(3))]),
            host.clock().time,
        )
        .unwrap();
    assert!(matches!(
        host.state.prepare_rule_application(
            &host.storage,
            &applications[0],
            host.next_id(),
            host.clock().time
        ),
        Err(Error::Overflow)
    ));
    assert_eq!(host.snapshot().unwrap(), before);
}

#[test]
fn hearing_refusal_and_dated_publication_reject_together_before_any_origin_record() {
    let mut host = fixture(Reject::Publication);
    crate::institution_mut(&mut host.storage, InstitutionId(10))
        .unwrap()
        .charter
        .offices[0]
        .powers
        .insert(OfficePower::HearAccessCases);
    crate::insert_case(
        &mut host.storage,
        CaseId(35),
        policy_fixtures::case(35, 1, SocialCaseKind::Exclusion, SocialOutcome::Unresolved),
    )
    .unwrap();
    for actor in [PersonId(1), PersonId(2)] {
        crate::knowledge_mut(&mut host.storage, actor)
            .unwrap()
            .cases
            .insert(CaseId(35));
    }
    let authority = AuthorityFacts {
        alive: BTreeSet::from([PersonId(1), PersonId(2)]),
    };
    let roster = observers();
    let index = crate::perception::ObservationIndex::new(&roster).unwrap();
    let action = InstitutionAction::HearCase {
        institution: InstitutionId(10),
        office: 0,
        case: CaseId(35),
        share: false,
    };
    let refusal = host
        .state
        .case_command_record(&host.storage, &action, host.clock().time)
        .unwrap();
    let observed = index.case_observers(&refusal, false).unwrap();
    let plan = host
        .state
        .prepare_case_command(
            &host.storage,
            &action,
            crate::case_commands::CaseCommandFacts {
                actor: PersonId(2),
                area: AreaId(3),
                now: host.clock().time,
                recipient_area: None,
                authority: &authority,
                refusal: Some(CaseId(host.next_id())),
                refusal_observers: &observed,
            },
        )
        .unwrap();
    let publications = plan
        .prepare_publications(
            &host.state,
            &host.storage,
            &authority,
            host.clock().time,
            &index,
        )
        .unwrap();
    let before = host.snapshot().unwrap();
    let mut batch = Batch::new();
    let publication = plan
        .stage_with_publications(
            &host.state,
            &host.storage,
            host.clock().time,
            publications,
            &mut batch,
        )
        .unwrap();
    publication
        .validate(&host.state, &host.storage, host.clock().time, &batch)
        .unwrap();
    assert_eq!(
        publication.effects.recorded,
        Some((CaseId(2_000), PersonId(1)))
    );
    assert!(
        batch
            .projected::<crate::Knowledge>(&host.storage, StableId(1))
            .unwrap()
            .unwrap()
            .institutions
            .contains_key(&InstitutionId(10))
    );

    let mut transaction = Transaction::new(host.transaction_opening(), batch);
    transaction.ids = 1;
    transaction.events = vec!["case recorded"];
    transaction.guards.push(publication.row_guard());
    assert_eq!(
        host.commit_transaction(transaction),
        Err(Error::Unavailable)
    );
    assert_eq!(host.snapshot().unwrap(), before);
    assert!(crate::hearing(&host.storage, CaseId(35)).is_none());
    assert!(crate::case(&host.storage, CaseId(2_000)).is_none());
}
