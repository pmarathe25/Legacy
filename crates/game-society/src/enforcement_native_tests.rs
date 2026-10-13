use super::*;
use game_characters::social::SocialAction;
use game_world::{GenericHost, actions::Actions};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PublishedEvent(String);
impl From<Event> for PublishedEvent {
    fn from(event: Event) -> Self {
        Self(format!("{event:?}"))
    }
}
impl From<crate::institutions::native::Event> for PublishedEvent {
    fn from(event: crate::institutions::native::Event) -> Self {
        Self(format!("Institution::{event:?}"))
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct State {
    enforcement: Option<World>,
    institutions: Option<institutions::Runtime>,
}
struct HumanFacts;
impl OwnerSystem<PublishedEvent> for HumanFacts {
    type Runtime = ();
    type Input = ();
    type Output = ();
    fn execute(&self, _: &mut OwnerTxn<'_, '_, Self, PublishedEvent>, _: ()) -> Result<(), Error> {
        Ok(())
    }
}
fn read_body(
    _: &HumanFacts,
    _: &mut OwnerReadTxn<'_, '_, HumanFacts, PublishedEvent>,
    _: game_physiology::services::ReadBody,
) -> Result<Option<game_physiology::services::BodyFacts>, Error> {
    Ok(Some(game_physiology::services::BodyFacts {
        vitals: Vitals {
            hunger: 0,
            fatigue: 0,
            health: 100,
            alive: true,
        },
        rules: game_physiology::Rules {
            hunger_harm: 100,
            fatigue_limit: 100,
            max_health: 100,
            meal_relief: 100,
            meal_size: Quantity(1),
            eating_time: SimDuration(1),
            loading_time: SimDuration(1),
            carrying_capacity: Quantity(100),
        },
    }))
}
fn read_placement(
    _: &HumanFacts,
    _: &mut OwnerReadTxn<'_, '_, HumanFacts, PublishedEvent>,
    _: ReadPlacement,
) -> Result<Option<game_characters::services::PlacementFacts>, Error> {
    Ok(Some(game_characters::services::PlacementFacts {
        location: Location::AtArea(AreaId(2)),
        point: None,
        can_resume: true,
    }))
}
fn read_stage(
    _: &HumanFacts,
    _: &mut OwnerReadTxn<'_, '_, HumanFacts, PublishedEvent>,
    _: ReadStage,
) -> Result<game_characters::services::StageFacts, Error> {
    Ok(game_characters::services::StageFacts {
        stage: Some(LifeStage::Adult),
        independent: true,
    })
}
fn read_combat(
    _: &HumanFacts,
    _: &mut OwnerReadTxn<'_, '_, HumanFacts, PublishedEvent>,
    _: game_conflict::services::ReadCombatStatus,
) -> Result<game_conflict::services::CombatStatus, Error> {
    Ok(game_conflict::services::CombatStatus {
        in_combat: false,
        restricted: false,
        can_fight: Some(true),
    })
}
fn read_physical(
    _: &HumanFacts,
    _: &mut OwnerReadTxn<'_, '_, HumanFacts, PublishedEvent>,
    _: game_characters::services::ReadPhysical,
) -> Result<Option<game_characters::services::PhysicalFacts>, Error> {
    Ok(Some(game_characters::services::PhysicalFacts {
        vitals: Vitals {
            hunger: 0,
            fatigue: 0,
            health: 100,
            alive: true,
        },
        limits: game_characters::decisions::NeedLimits {
            hunger_harm: 100,
            fatigue_limit: 100,
            max_health: 100,
            meal_relief: 100,
            meal_size: Quantity(1),
            eating_time: SimDuration(1),
            drink_quantity: Quantity(0),
        },
    }))
}
fn read_people(
    _: &HumanFacts,
    tx: &mut OwnerReadTxn<'_, '_, HumanFacts, PublishedEvent>,
    _: game_characters::services::ReadPeople,
) -> Result<Vec<PersonId>, Error> {
    Ok(tx
        .iter::<game_characters::Person>()
        .into_iter()
        .map(|(_, person)| person.id)
        .collect())
}
fn read_person(
    _: &HumanFacts,
    tx: &mut OwnerReadTxn<'_, '_, HumanFacts, PublishedEvent>,
    request: game_characters::services::ReadPerson,
) -> Result<Option<game_characters::Person>, Error> {
    Ok(tx
        .read::<game_characters::Person>(StableId(request.0.0))
        .cloned())
}
fn read_mind(
    _: &HumanFacts,
    tx: &mut OwnerReadTxn<'_, '_, HumanFacts, PublishedEvent>,
    request: game_characters::services::ReadMind<SocialAction>,
) -> Result<Option<Mind<SocialAction>>, Error> {
    Ok(tx
        .read::<Mind<SocialAction>>(StableId(request.0.0))
        .cloned())
}
fn read_topology(
    _: &HumanFacts,
    _: &mut OwnerReadTxn<'_, '_, HumanFacts, PublishedEvent>,
    _: game_transport::ReadTopology,
) -> Result<Vec<game_transport::Area>, Error> {
    Ok(vec![game_transport::Area {
        id: AreaId(2),
        name: "Chosen court".into(),
        links: Vec::new(),
    }])
}
fn publications(
    _: &HumanFacts,
    _: &mut OwnerTxn<'_, '_, HumanFacts, PublishedEvent>,
    _: services::ReceiveRegionalPublications,
) -> Result<(), Error> {
    Ok(())
}
fn no_incidents(
    _: &HumanFacts,
    _: &mut OwnerReadTxn<'_, '_, HumanFacts, PublishedEvent>,
    _: game_property::services::ReadPropertyIncidents,
) -> Result<Vec<game_property::IncidentView>, Error> {
    Ok(Vec::new())
}
fn no_agreements(
    _: &HumanFacts,
    _: &mut OwnerReadTxn<'_, '_, HumanFacts, PublishedEvent>,
    _: game_agreements::native::ReadAgreementView,
) -> Result<Vec<AgreementView>, Error> {
    Ok(Vec::new())
}
fn case() -> Case {
    Case {
        id: EnforcementCaseId(20),
        institution: InstitutionId(10),
        office: 0,
        filer: PersonId(1),
        target: PersonId(3),
        topic: EnforcementTopic::Assault,
        evidence: EnforcementEvidence::Assault(WitnessedAssault {
            event: EventId(1),
            attacker: PersonId(3),
            target: PersonId(1),
            area: AreaId(2),
            damage: 1,
            observed_at: SimTime(0),
        }),
        contact: None,
        policy: EnforcementPolicy {
            topics: BTreeSet::from([EnforcementTopic::Assault]),
            areas: BTreeSet::from([AreaId(2)]),
            subjects: Subjects::members(),
            enforcer_office: 1,
            remedies: BTreeSet::from([RemedyKind::Dismiss]),
            spending: Default::default(),
            supplies: Default::default(),
            maximum_confinement: SimDuration(0),
        },
        judgment: None,
        mission: None,
        settlement: None,
        status: EnforcementStatus::Filing,
        explanation: "Admitted filing conversation".into(),
        changed_at: SimTime(0),
        notice_area: None,
        witnesses: BTreeSet::from([PersonId(1)]),
    }
}
fn fixture(
    reject_withdrawal: bool,
) -> (
    GenericHost<State, PublishedEvent>,
    Registry<State, PublishedEvent>,
) {
    let mut schema = Schema::new();
    crate::register_components(&mut schema).unwrap();
    let validator: fn(&Case) -> Result<(), Error> = if reject_withdrawal {
        |row| {
            if row.status == EnforcementStatus::Withdrawn {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        }
    } else {
        |_| Ok(())
    };
    schema
        .register::<Case>("society/enforcement-case", validator)
        .unwrap();
    schema
        .register::<Talk>("society/enforcement-talk", |_| Ok(()))
        .unwrap();
    schema
        .register::<Policies>("society/enforcement-policies", |_| Ok(()))
        .unwrap();
    schema
        .register::<Memory>("society/enforcement-memory", |_| Ok(()))
        .unwrap();
    game_world::actions::register_components(&mut schema).unwrap();
    game_characters::register_people(&mut schema).unwrap();
    schema
        .register::<Mind<SocialAction>>("test/mind", |_| Ok(()))
        .unwrap();
    let mut host = GenericHost::new(
        schema,
        1,
        State {
            enforcement: Some(World { last_event: 0 }),
            institutions: Some(institutions::Runtime::default()),
        },
    );
    host.allocate_ids(200).unwrap();
    host.storage.insert_or_spawn(StableId(20), case()).unwrap();
    host.storage
        .insert_or_spawn(
            StableId(1),
            Talk {
                case: EnforcementCaseId(20),
                recipient: PersonId(2),
                area: AreaId(2),
                completes: SimTime(60_000),
                conversation: Conversation::File,
            },
        )
        .unwrap();
    host.storage
        .insert(
            StableId(1),
            ActionSlot {
                actor: StableId(1),
                owner: "enforcement".into(),
                started: SimTime(0),
                completes: Some(SimTime(60_000)),
            },
        )
        .unwrap();
    let mut registry = Registry::new();
    register_systems::<_, SocialAction, _>(
        &mut registry,
        RuntimeSlot::new(
            |state: &State| &state.enforcement,
            |state| &mut state.enforcement,
        ),
        |_| None,
    )
    .unwrap();
    services::register_services(
        &mut registry,
        RuntimeSlot::new(
            |state: &State| &state.institutions,
            |state| &mut state.institutions,
        ),
    )
    .unwrap();
    crate::institutions::native::register_native::<_, SocialAction, PublishedEvent>(&mut registry)
        .unwrap();
    for id in [1, 2, 3] {
        game_characters::insert_person(
            &mut host.storage,
            game_characters::Person {
                id: PersonId(id),
                role: if id == 2 { Role::Ruler } else { Role::Citizen },
                inventory: InventoryId(id + 100),
                work_units: 0,
                work_remainder: 0,
                work_rate: 0,
                hunger_exempt: false,
            },
        )
        .unwrap();
        host.storage
            .insert(
                StableId(id),
                Mind::<SocialAction>::new(
                    Personality::restful(),
                    game_characters::decisions::Aspiration::Comfort,
                )
                .unwrap(),
            )
            .unwrap();
    }
    registry.register_stateless(HumanFacts).unwrap();
    registry
        .register_read_request::<HumanFacts, game_physiology::services::ReadBody>(read_body)
        .unwrap();
    registry
        .register_read_request::<HumanFacts, ReadPlacement>(read_placement)
        .unwrap();
    registry
        .register_read_request::<HumanFacts, ReadStage>(read_stage)
        .unwrap();
    registry
        .register_read_request::<HumanFacts, game_conflict::services::ReadCombatStatus>(read_combat)
        .unwrap();
    registry
        .register_read_request::<HumanFacts, game_characters::services::ReadPhysical>(read_physical)
        .unwrap();
    registry
        .register_read_request::<HumanFacts, game_characters::services::ReadPeople>(read_people)
        .unwrap();
    registry
        .register_read_request::<HumanFacts, game_characters::services::ReadPerson>(read_person)
        .unwrap();
    registry
        .register_read_request::<HumanFacts, game_characters::services::ReadMind<SocialAction>>(
            read_mind,
        )
        .unwrap();
    registry
        .register_read_request::<HumanFacts, game_transport::ReadTopology>(read_topology)
        .unwrap();
    registry
        .register_request::<HumanFacts, services::ReceiveRegionalPublications>(publications)
        .unwrap();
    registry
        .register_read_request::<HumanFacts, game_property::services::ReadPropertyIncidents>(
            no_incidents,
        )
        .unwrap();
    registry
        .register_read_request::<HumanFacts, game_agreements::native::ReadAgreementView>(
            no_agreements,
        )
        .unwrap();
    Actions::new()
        .with_route("enforcement", cancel_action::<SocialAction, PublishedEvent>)
        .unwrap()
        .register(&mut registry, RuntimeSlot::stateless())
        .unwrap();
    (host, registry)
}
#[test]
fn action_cancellation_withdraws_filing_and_removes_owned_talk_and_lease() {
    let (mut host, registry) = fixture(false);
    let plan = registry
        .prepare(&host, SimTime(10), None, |tx| {
            tx.request(game_world::actions::CancelAction { actor: StableId(1) })
        })
        .unwrap();
    registry.commit(&mut host, plan).unwrap();
    assert_eq!(
        host.storage.get::<Case>(StableId(20)).unwrap().status,
        EnforcementStatus::Withdrawn
    );
    assert!(host.storage.get::<Talk>(StableId(1)).is_none());
    assert!(host.storage.get::<ActionSlot>(StableId(1)).is_none());
    assert!(host.events().is_empty());
}
#[test]
fn canonical_schema_rejection_preserves_case_talk_lease_clock_and_runtime() {
    let (mut host, registry) = fixture(true);
    let before = host.snapshot().unwrap();
    let plan = registry
        .prepare(&host, SimTime(10), None, |tx| {
            tx.request(game_world::actions::CancelAction { actor: StableId(1) })
        })
        .unwrap();
    assert_eq!(registry.commit(&mut host, plan), Err(Error::Unavailable));
    assert_eq!(host.snapshot().unwrap(), before);
}
#[test]
fn readonly_view_does_not_create_missing_enforcement_memory() {
    let (host, registry) = fixture(false);
    let before = host.snapshot().unwrap();
    assert_eq!(
        registry
            .inspect(&host, None, ReadEnforcementView(PersonId(1)))
            .unwrap(),
        EnforcementView::default()
    );
    assert!(host.storage.get::<Memory>(StableId(1)).is_none());
    assert_eq!(host.snapshot().unwrap(), before);
}

#[test]
fn native_filing_completion_reaches_officer_and_local_witnesses_before_releasing_lease() {
    let (mut host, registry) = fixture(false);
    let plan = registry
        .prepare(&host, SimTime(60_000), None, |tx| {
            tx.call::<Systems<SocialAction, PublishedEvent>>(Input::Complete(PersonId(1)))
        })
        .unwrap();
    registry.commit(&mut host, plan).unwrap();
    assert_eq!(
        host.storage.get::<Case>(StableId(20)).unwrap().status,
        EnforcementStatus::Pending
    );
    for id in [1, 2, 3] {
        assert!(
            host.storage
                .get::<Memory>(StableId(id))
                .unwrap()
                .known
                .contains_key(&EnforcementCaseId(20))
        );
    }
    assert!(host.storage.get::<Talk>(StableId(1)).is_none());
    assert!(host.storage.get::<ActionSlot>(StableId(1)).is_none());
    assert_eq!(host.events().len(), 1);
}
#[test]
fn inactive_enforcement_is_inert_for_queries_but_rejects_commands_and_orphan_rows() {
    let (mut host, registry) = fixture(false);
    host.state.enforcement = None;
    let before = host.snapshot().unwrap();
    assert!(
        registry
            .inspect(&host, None, AvailableEnforcement(PersonId(1)))
            .unwrap()
            .is_empty()
    );
    assert_eq!(host.snapshot().unwrap(), before);
    assert!(matches!(
        registry.prepare(&host, SimTime(0), None, |tx| tx.request(
            ExecuteEnforcement {
                actor: PersonId(1),
                action: EnforcementAction::Respond {
                    case: EnforcementCaseId(20),
                    comply: false
                },
            }
        )),
        Err(Error::Unavailable)
    ));
    assert!(matches!(
        registry.prepare(&host, SimTime(0), None, |tx| tx.call::<Systems<
            SocialAction,
            PublishedEvent,
        >>(
            Input::ValidateSave
        )),
        Err(Error::InvalidSave(_))
    ));
    assert_eq!(host.snapshot().unwrap(), before);
}
struct VoteThenReviewLaterActor;
impl Request for VoteThenReviewLaterActor {
    type Response = ();
}
fn vote_then_review_later_actor(
    owner: &Systems<SocialAction, PublishedEvent>,
    tx: &mut OwnerTxn<'_, '_, Systems<SocialAction, PublishedEvent>, PublishedEvent>,
    _: VoteThenReviewLaterActor,
) -> Result<(), Error> {
    let mut port = NativePort::new(Access::Write(tx), owner.strike)?;
    port.institution_command(
        PersonId(1),
        &InstitutionAction::Vote {
            proposal: RuleProposalId(30),
            support: true,
        },
    )?;
    let mut later = System::for_actor(port, PersonId(3));
    let action = later
        .available_enforcement_commands(PersonId(3))
        .into_iter()
        .find(|action| {
            matches!(
                action,
                EnforcementAction::File {
                    evidence: EnforcementEvidenceRef::Assault(EventId(99)),
                    ..
                }
            )
        })
        .ok_or(Error::Unavailable)?;
    later.enforcement_command(PersonId(3), &action)?;
    later.publish()
}
#[test]
fn institutional_adoption_is_visible_to_later_actor_before_current_enforcement_review_returns() {
    let (mut host, mut registry) = fixture(false);
    let mut institution =
        crate::policy_fixtures::institution(10, Vec::new(), InstitutionStatus::Active);
    institution.charter.jurisdiction = AreaId(2);
    institution.members = BTreeSet::from([PersonId(1), PersonId(2), PersonId(3)]);
    institution.charter.founders = institution.members.clone();
    institution.charter.procedure = DecisionProcedure {
        proposers: Subjects::members(),
        voters: Subjects::members(),
        minimum_support: 1,
    };
    institution.charter.offices = vec![
        OfficeMandate {
            slot: 0,
            holder: PersonId(1),
            inventories: Default::default(),
            sites: Default::default(),
            powers: BTreeSet::from([OfficePower::Adjudicate]),
            minimum_support: 1,
        },
        OfficeMandate {
            slot: 1,
            holder: PersonId(1),
            inventories: Default::default(),
            sites: Default::default(),
            powers: BTreeSet::from([OfficePower::Enforce]),
            minimum_support: 1,
        },
    ];
    institution.office_support = std::collections::BTreeMap::from([
        (0, BTreeSet::from([PersonId(1)])),
        (1, BTreeSet::from([PersonId(1)])),
    ]);
    crate::insert_institution(&mut host.storage, InstitutionId(10), institution).unwrap();
    crate::insert_proposal(
        &mut host.storage,
        RuleProposalId(30),
        RuleProposal {
            id: RuleProposalId(30),
            institution: InstitutionId(10),
            proposer: PersonId(1),
            change: InstitutionChange::EnforcementPolicy {
                office: 0,
                policy: Some(case().policy),
            },
            votes: Default::default(),
            base_revision: 0,
            status: RuleProposalStatus::Proposed,
        },
    )
    .unwrap();
    let mut memory = Memory::default();
    memory.assaults.insert(
        EventId(99),
        WitnessedAssault {
            event: EventId(99),
            attacker: PersonId(2),
            target: PersonId(3),
            area: AreaId(2),
            damage: 1,
            observed_at: SimTime(0),
        },
    );
    host.storage.insert(StableId(3), memory).unwrap();
    registry
        .register_request::<Systems<SocialAction, PublishedEvent>, VoteThenReviewLaterActor>(
            vote_then_review_later_actor,
        )
        .unwrap();
    let before = host.snapshot().unwrap();
    let plan = registry
        .prepare(&host, SimTime(0), None, |tx| {
            tx.request(VoteThenReviewLaterActor)
        })
        .unwrap();
    assert_eq!(host.snapshot().unwrap(), before);
    registry.commit(&mut host, plan).unwrap();
    assert_eq!(
        host.storage
            .get::<RuleProposal>(StableId(30))
            .unwrap()
            .status,
        RuleProposalStatus::Adopted
    );
    assert!(host.storage.iter::<Case>().any(|(_,case)| case.filer == PersonId(3) && matches!(case.evidence, EnforcementEvidence::Assault(ref assault) if assault.event == EventId(99))));
    let events = host.events();
    assert_eq!(events.len(), 3);
    assert!(events[0].event.0.starts_with("Institution::Changed"));
    assert!(events[1].event.0.starts_with("EnforcementPolicyChanged"));
    assert!(events[2].event.0.starts_with("EnforcementFiling"));
}
#[test]
fn persisted_enforcement_event_validation_rejects_missing_judgment_and_is_inert() {
    let (host, registry) = fixture(false);
    let before = host.snapshot().unwrap();
    registry
        .inspect(
            &host,
            None,
            ValidateEnforcementEvent(Event::EnforcementFiling {
                case: EnforcementCaseId(20),
                actor: PersonId(1),
                recipient: PersonId(2),
            }),
        )
        .unwrap();
    assert!(matches!(
        registry.inspect(
            &host,
            None,
            ValidateEnforcementEvent(Event::EnforcementDecided {
                case: EnforcementCaseId(20),
                judgment: JudgmentId(90),
                proposer: PersonId(1)
            })
        ),
        Err(Error::InvalidSave(_))
    ));
    assert_eq!(host.snapshot().unwrap(), before);
}
