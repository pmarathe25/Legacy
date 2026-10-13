//! Real Society command production and receiving Enforcement effects share one host publication.
use crate::{enforcement as enforcement_owner, institutions::*, policy_fixtures};
use game_core::*;
use game_ecs::{Batch, Schema, StableId};
use game_sim::ClockWorld;
use game_world::{GenericHost, Transaction};
use std::collections::{BTreeMap, BTreeSet};
type Host = GenericHost<Runtime, &'static str>;
fn policy() -> enforcement_owner::EnforcementPolicy {
    enforcement_owner::EnforcementPolicy {
        topics: BTreeSet::from([enforcement_owner::EnforcementTopic::Assault]),
        areas: BTreeSet::from([AreaId(3)]),
        subjects: Subjects::members(),
        enforcer_office: 0,
        remedies: BTreeSet::from([enforcement_owner::RemedyKind::Dismiss]),
        spending: BTreeMap::new(),
        supplies: BTreeMap::new(),
        maximum_confinement: SimDuration(100),
    }
}
fn fixture(
    change: InstitutionChange,
    reject_case: bool,
    reject_policy: bool,
    reject_memory: bool,
) -> Host {
    let mut schema = Schema::new();
    crate::register_components(&mut schema).unwrap();
    let case_validator: fn(&enforcement_owner::Case) -> Result<(), Error> = if reject_case {
        |row| {
            if row.judgment.is_some() {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        }
    } else {
        |_| Ok(())
    };
    let policy_validator: fn(&enforcement_owner::Policies) -> Result<(), Error> = if reject_policy {
        |row| {
            if !row.offices.is_empty() {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        }
    } else {
        |_| Ok(())
    };
    schema
        .register::<enforcement_owner::Case>("society/enforcement-case", case_validator)
        .unwrap();
    schema
        .register::<enforcement_owner::Policies>("society/enforcement-policies", policy_validator)
        .unwrap();
    let memory_validator: fn(&enforcement_owner::Memory) -> Result<(), Error> = if reject_memory {
        |row| {
            if !row.contacts.is_empty() || !row.known.is_empty() || !row.known_policies.is_empty() {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        }
    } else {
        |_| Ok(())
    };
    schema
        .register::<enforcement_owner::Memory>("society/enforcement-memory", memory_validator)
        .unwrap();
    let mut host = Host::new(schema, 1, Runtime::default());
    host.allocate_ids(39).unwrap();
    let mut institution = policy_fixtures::institution(10, vec![], InstitutionStatus::Active);
    institution.members = BTreeSet::from([PersonId(1), PersonId(2)]);
    institution.charter.procedure.minimum_support = 2;
    institution.charter.offices[0].powers =
        BTreeSet::from([OfficePower::Adjudicate, OfficePower::Enforce]);
    institution.office_support = BTreeMap::from([(0, BTreeSet::from([PersonId(1), PersonId(2)]))]);
    crate::insert_institution(&mut host.storage, InstitutionId(10), institution).unwrap();
    for actor in [PersonId(1), PersonId(2)] {
        crate::insert_knowledge(&mut host.storage, actor, crate::Knowledge::default()).unwrap();
    }
    host.storage
        .insert_or_spawn(StableId(10), enforcement_owner::Policies::default())
        .unwrap();
    host.storage
        .insert_or_spawn(
            StableId(20),
            enforcement_owner::Case {
                id: EnforcementCaseId(20),
                institution: InstitutionId(10),
                office: 0,
                filer: PersonId(1),
                target: PersonId(2),
                topic: enforcement_owner::EnforcementTopic::Assault,
                evidence: enforcement_owner::EnforcementEvidence::Assault(
                    enforcement_owner::WitnessedAssault {
                        event: EventId(1),
                        attacker: PersonId(2),
                        target: PersonId(1),
                        area: AreaId(3),
                        damage: 1,
                        observed_at: SimTime(0),
                    },
                ),
                contact: None,
                policy: policy(),
                judgment: None,
                mission: None,
                settlement: None,
                status: enforcement_owner::EnforcementStatus::Pending,
                explanation: "pending".into(),
                changed_at: SimTime(0),
                notice_area: Some(AreaId(3)),
                witnesses: BTreeSet::from([PersonId(1)]),
            },
        )
        .unwrap();
    crate::insert_proposal(
        &mut host.storage,
        RuleProposalId(30),
        RuleProposal {
            id: RuleProposalId(30),
            institution: InstitutionId(10),
            proposer: PersonId(2),
            change,
            votes: BTreeMap::from([(PersonId(1), true)]),
            base_revision: 0,
            status: RuleProposalStatus::Proposed,
        },
    )
    .unwrap();
    host
}
fn stage(
    host: &Host,
) -> (
    Batch,
    crate::projected_publication::CommandPublicationGuard,
    enforcement_owner::AdoptionPublication,
    enforcement_owner::ObservationPublication,
    u64,
) {
    let action = InstitutionAction::Vote {
        proposal: RuleProposalId(30),
        support: true,
    };
    let original = crate::proposal(&host.storage, RuleProposalId(30)).unwrap();
    let references = policy_fixtures::refs();
    let authority = AuthorityFacts {
        alive: references.people.clone(),
    };
    let permit = ExternalChangePermit {
        actor: PersonId(2),
        institution: InstitutionId(10),
        institution_revision: 0,
        change: original.change.clone(),
    };
    let producer = host
        .state
        .prepare_command(
            &host.storage,
            &action,
            &CommandFacts {
                actor: PersonId(2),
                area: AreaId(3),
                now: host.clock().time,
                references: &references,
                authority: &authority,
                external_permit: Some(&permit),
            },
            CommandIds {
                first: host.next_id(),
            },
        )
        .unwrap();
    assert!(producer.effects.iter().any(|effect| matches!(
        effect,
        CommandEffect::ExternalOutcome {
            outcome: RuleProposalStatus::Adopted,
            ..
        }
    )));
    let observers = [
        crate::perception::ObserverFacts {
            actor: PersonId(1),
            area: Some(AreaId(3)),
            alive: true,
            independent: true,
            conversation_allowed: true,
            empathy: 500,
        },
        crate::perception::ObserverFacts {
            actor: PersonId(2),
            area: Some(AreaId(3)),
            alive: true,
            independent: true,
            conversation_allowed: true,
            empathy: 500,
        },
    ];
    let index = crate::perception::ObservationIndex::new(&observers).unwrap();
    let publications = producer
        .prepare_publications(
            &host.state,
            &host.storage,
            &authority,
            host.clock().time,
            &index,
        )
        .unwrap();
    let mut batch = Batch::new();
    let producer_guard = producer
        .stage_with_publications(&host.state, &host.storage, publications, &mut batch)
        .unwrap();
    let projected = batch
        .projected::<RuleProposal>(&host.storage, StableId(30))
        .unwrap()
        .unwrap();
    let judgment = matches!(projected.change, InstitutionChange::Adjudicate { .. })
        .then_some(JudgmentId(host.next_id()));
    let receiver =
        enforcement_owner::prepare_adoption(&host.storage, host.clock().time, projected, judgment)
            .unwrap()
            .stage(&host.storage, &mut batch)
            .unwrap();
    let people = [PersonId(1), PersonId(2)].map(|id| enforcement_owner::PersonFacts {
        id,
        inventory: InventoryId(id.0 + 100),
        location: Location::AtArea(AreaId(3)),
        vitals: Vitals {
            alive: true,
            health: 100,
            hunger: 0,
            fatigue: 0,
        },
        independent: true,
        combat_restricted: false,
    });
    let observation = enforcement_owner::stage_adoption_observation(
        &host.storage,
        &mut batch,
        &[&receiver],
        enforcement_owner::AdoptionObservationFacts {
            now: host.clock().time,
            people: &people,
            assaults: &[],
            last_event: None,
        },
        game_ecs::ReadSet::new(),
    )
    .unwrap();
    (
        batch,
        producer_guard,
        receiver,
        observation,
        u64::from(judgment.is_some()),
    )
}
#[test]
fn producer_and_receiving_schema_rejection_preserve_the_whole_host_publication() {
    for (change, reject_case, reject_policy) in [
        (
            InstitutionChange::Adjudicate {
                case: EnforcementCaseId(20),
                remedy: enforcement_owner::EnforcementRemedy::Dismiss,
            },
            true,
            false,
        ),
        (
            InstitutionChange::EnforcementPolicy {
                office: 0,
                policy: Some(policy()),
            },
            false,
            true,
        ),
    ] {
        let mut host = fixture(change, reject_case, reject_policy, false);
        let before = host.snapshot().unwrap();
        let clock = host.clock();
        let (batch, producer, receiver, observation, ids) = stage(&host);
        producer.validate(&batch).unwrap();
        receiver.validate(&host.storage, &batch).unwrap();
        observation.validate(None, &host.storage, &batch).unwrap();
        let mut transaction = Transaction::new(host.transaction_opening(), batch);
        transaction.ids = ids;
        transaction.events = vec!["institution changed", "enforcement changed"];
        transaction.revisions = 1;
        assert_eq!(
            host.commit_transaction(transaction),
            Err(Error::Unavailable)
        );
        assert_eq!(host.snapshot().unwrap(), before);
        assert_eq!(host.clock(), clock);
        assert_eq!(
            crate::proposal(&host.storage, RuleProposalId(30))
                .unwrap()
                .status,
            RuleProposalStatus::Proposed
        );
        assert!(
            enforcement_owner::case(&host.storage, EnforcementCaseId(20))
                .unwrap()
                .judgment
                .is_none()
        );
        assert!(
            host.storage
                .get::<enforcement_owner::Policies>(StableId(10))
                .unwrap()
                .offices
                .is_empty()
        );
    }
}
#[test]
fn real_vote_and_receiving_judgment_publish_together_and_guard_the_producer_projection() {
    let change = InstitutionChange::Adjudicate {
        case: EnforcementCaseId(20),
        remedy: enforcement_owner::EnforcementRemedy::Dismiss,
    };
    let mut host = fixture(change, false, false, false);
    let (mut batch, _producer, receiver, _observation, _ids) = stage(&host);
    let mut mismatched = batch
        .projected::<RuleProposal>(&host.storage, StableId(30))
        .unwrap()
        .unwrap()
        .clone();
    mismatched.status = RuleProposalStatus::Proposed;
    batch.insert(StableId(30), mismatched);
    assert!(receiver.validate(&host.storage, &batch).is_err());
    let (batch, producer, receiver, observation, ids) = stage(&host);
    producer.validate(&batch).unwrap();
    receiver.validate(&host.storage, &batch).unwrap();
    observation.validate(None, &host.storage, &batch).unwrap();
    assert!(matches!(
        receiver.finish(),
        Some(enforcement_owner::Event::EnforcementDecided {
            case: EnforcementCaseId(20),
            judgment: JudgmentId(40),
            proposer: PersonId(2)
        })
    ));
    let mut transaction = Transaction::new(host.transaction_opening(), batch);
    transaction.ids = ids;
    transaction.events = vec!["institution changed", "enforcement decided"];
    transaction.revisions = 1;
    host.commit_transaction(transaction).unwrap();
    assert_eq!(
        crate::proposal(&host.storage, RuleProposalId(30))
            .unwrap()
            .status,
        RuleProposalStatus::Adopted
    );
    assert_eq!(
        enforcement_owner::case(&host.storage, EnforcementCaseId(20))
            .unwrap()
            .status,
        enforcement_owner::EnforcementStatus::Dismissed
    );
    assert_eq!(
        enforcement_owner::memory(&host.storage, PersonId(1))
            .unwrap()
            .known[&EnforcementCaseId(20)]
            .status,
        enforcement_owner::EnforcementStatus::Dismissed
    );
    assert_eq!(observation.finish(host.next_event() - 1).last_event, 2);
    assert_eq!(host.next_id(), 41);
    assert_eq!(host.next_event(), 3);
    assert_eq!(host.clock().revision, 1);
    assert_eq!(host.events().len(), 2);
}

#[test]
fn producer_schema_rejection_keeps_the_receiving_judgment_unpublished() {
    let change = InstitutionChange::Adjudicate {
        case: EnforcementCaseId(20),
        remedy: enforcement_owner::EnforcementRemedy::Dismiss,
    };
    let mut host = fixture(change, false, false, false);
    let mut schema = Schema::new();
    schema
        .register::<Institution>("society/institution", Institution::validate_shape)
        .unwrap();
    schema
        .register::<RuleProposal>("society/proposal", |row| {
            if row.status == RuleProposalStatus::Adopted {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    schema
        .register::<crate::Knowledge>("society/knowledge", crate::Knowledge::validate_shape)
        .unwrap();
    schema
        .register::<enforcement_owner::Case>("society/enforcement-case", |_| Ok(()))
        .unwrap();
    schema
        .register::<enforcement_owner::Policies>("society/enforcement-policies", |_| Ok(()))
        .unwrap();
    schema
        .register::<enforcement_owner::Memory>("society/enforcement-memory", |_| Ok(()))
        .unwrap();
    schema
        .register::<crate::Concept>("society/concept", |row| {
            crate::validate_rule_shape(&row.rule)
        })
        .unwrap();
    schema
        .register::<SocialCaseView>("society/case", SocialCaseView::validate_shape)
        .unwrap();
    schema
        .register::<crate::Requests>("society/requests", |_| Ok(()))
        .unwrap();
    schema
        .register::<crate::Exclusions>("society/exclusions", |_| Ok(()))
        .unwrap();
    schema
        .register::<crate::Hearing>("society/hearing", |_| Ok(()))
        .unwrap();
    host.storage = game_ecs::Store::restore(schema, host.storage.snapshot().unwrap()).unwrap();
    let before = host.snapshot().unwrap();
    let clock = host.clock();
    let (batch, producer, receiver, observation, ids) = stage(&host);
    producer.validate(&batch).unwrap();
    receiver.validate(&host.storage, &batch).unwrap();
    observation.validate(None, &host.storage, &batch).unwrap();
    let mut transaction = Transaction::new(host.transaction_opening(), batch);
    transaction.ids = ids;
    transaction.events = vec!["institution changed", "enforcement decided"];
    transaction.revisions = 1;
    assert_eq!(
        host.commit_transaction(transaction),
        Err(Error::Unavailable)
    );
    assert_eq!(host.snapshot().unwrap(), before);
    assert_eq!(host.clock(), clock);
}

#[test]
fn exhausted_host_publication_budget_cannot_publish_only_one_owner() {
    for exhaust_ids in [true, false] {
        let change = InstitutionChange::Adjudicate {
            case: EnforcementCaseId(20),
            remedy: enforcement_owner::EnforcementRemedy::Dismiss,
        };
        let mut host = fixture(change, false, false, false);
        if exhaust_ids {
            host.allocate_ids(u64::MAX - host.next_id()).unwrap();
        } else {
            let mut clock = host.clock();
            clock.revision = u64::MAX;
            host.publish_clock(clock);
        }
        let before = host.snapshot().unwrap();
        let clock = host.clock();
        let (batch, producer, receiver, observation, ids) = stage(&host);
        producer.validate(&batch).unwrap();
        receiver.validate(&host.storage, &batch).unwrap();
        observation.validate(None, &host.storage, &batch).unwrap();
        let mut transaction = Transaction::new(host.transaction_opening(), batch);
        transaction.ids = ids;
        transaction.events = vec!["institution changed", "enforcement decided"];
        transaction.revisions = 1;
        assert_eq!(host.commit_transaction(transaction), Err(Error::Overflow));
        assert_eq!(host.snapshot().unwrap(), before);
        assert_eq!(host.clock(), clock);
    }
}

#[test]
fn receiving_memory_schema_rejection_keeps_adoption_judgment_ids_and_events_unpublished() {
    let change = InstitutionChange::Adjudicate {
        case: EnforcementCaseId(20),
        remedy: enforcement_owner::EnforcementRemedy::Dismiss,
    };
    let mut host = fixture(change, false, false, true);
    let before = host.snapshot().unwrap();
    let clock = host.clock();
    let (batch, producer, receiver, observation, ids) = stage(&host);
    producer.validate(&batch).unwrap();
    receiver.validate(&host.storage, &batch).unwrap();
    observation.validate(None, &host.storage, &batch).unwrap();
    let mut transaction = Transaction::new(host.transaction_opening(), batch);
    transaction.ids = ids;
    transaction.events = vec!["institution changed", "enforcement decided"];
    transaction.revisions = 1;
    assert_eq!(
        host.commit_transaction(transaction),
        Err(Error::Unavailable)
    );
    assert_eq!(host.snapshot().unwrap(), before);
    assert_eq!(host.clock(), clock);
    assert!(enforcement_owner::memory(&host.storage, PersonId(1)).is_none());
}
