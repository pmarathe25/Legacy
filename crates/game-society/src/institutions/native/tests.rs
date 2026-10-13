use super::*;
use game_characters::services::{ReadMind, ReadPerson, ReadPhysical, ReadPlacement, ReadStage};
use game_world::{GenericHost, OwnerSystem, RuntimeSlot};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum TestEvent {
    Institution(Event),
    Ate {
        actor: PersonId,
        source: InventoryId,
    },
}
impl From<Event> for TestEvent {
    fn from(event: Event) -> Self {
        Self::Institution(event)
    }
}
fn history_codec(event: &TestEvent) -> Option<HistoryEvent> {
    match event {
        TestEvent::Ate { actor, source } => Some(HistoryEvent::Ate {
            actor: *actor,
            source: *source,
        }),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct State {
    institutions: Option<Runtime>,
    reject: bool,
}
struct Peers;
impl OwnerSystem<TestEvent> for Peers {
    type Runtime = bool;
    type Input = ReceiveSocialReceipts;
    type Output = ();
    fn execute(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, TestEvent>,
        input: Self::Input,
    ) -> Result<(), Error> {
        tx.publish(input)
    }
}
fn body() -> game_physiology::services::BodyFacts {
    game_physiology::services::BodyFacts {
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
    }
}
fn read_body(
    _: &Peers,
    _: &mut OwnerReadTxn<'_, '_, Peers, TestEvent>,
    _: game_physiology::services::ReadBody,
) -> Result<Option<game_physiology::services::BodyFacts>, Error> {
    Ok(Some(body()))
}
fn physical(
    _: &Peers,
    _: &mut OwnerReadTxn<'_, '_, Peers, TestEvent>,
    _: ReadPhysical,
) -> Result<Option<game_characters::services::PhysicalFacts>, Error> {
    Ok(Some(game_characters::services::PhysicalFacts {
        vitals: body().vitals,
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
fn place(
    _: &Peers,
    _: &mut OwnerReadTxn<'_, '_, Peers, TestEvent>,
    _: ReadPlacement,
) -> Result<Option<game_characters::services::PlacementFacts>, Error> {
    Ok(Some(game_characters::services::PlacementFacts {
        location: Location::AtArea(AreaId(3)),
        point: None,
        can_resume: true,
    }))
}
fn stage(
    _: &Peers,
    _: &mut OwnerReadTxn<'_, '_, Peers, TestEvent>,
    _: ReadStage,
) -> Result<game_characters::services::StageFacts, Error> {
    Ok(game_characters::services::StageFacts {
        stage: Some(LifeStage::Adult),
        independent: true,
    })
}
fn combat(
    _: &Peers,
    _: &mut OwnerReadTxn<'_, '_, Peers, TestEvent>,
    _: game_conflict::services::ReadCombatStatus,
) -> Result<game_conflict::services::CombatStatus, Error> {
    Ok(game_conflict::services::CombatStatus {
        in_combat: false,
        restricted: false,
        can_fight: Some(true),
    })
}
fn mind(
    _: &Peers,
    _: &mut OwnerReadTxn<'_, '_, Peers, TestEvent>,
    _: ReadMind<()>,
) -> Result<Option<game_characters::decisions::Mind<()>>, Error> {
    Ok(Some(game_characters::decisions::Mind::new(
        game_characters::decisions::Personality::industrious(),
        game_characters::decisions::Aspiration::SupportOthers,
    )?))
}
fn person(
    _: &Peers,
    tx: &mut OwnerReadTxn<'_, '_, Peers, TestEvent>,
    q: ReadPerson,
) -> Result<Option<game_characters::people::Person>, Error> {
    Ok(tx.read(StableId(q.0.0)).cloned())
}
fn stock(
    _: &Peers,
    tx: &mut OwnerReadTxn<'_, '_, Peers, TestEvent>,
    q: game_cargo::facts::ReadInventory,
) -> Result<Option<game_cargo::Inventory>, Error> {
    Ok(tx.read(StableId(q.0.0)).cloned())
}
fn area(
    _: &Peers,
    _: &mut OwnerReadTxn<'_, '_, Peers, TestEvent>,
    _: game_cargo::facts::ReadInventoryArea,
) -> Result<Option<AreaId>, Error> {
    Ok(Some(AreaId(3)))
}
fn custody(
    _: &Peers,
    _: &mut OwnerReadTxn<'_, '_, Peers, TestEvent>,
    _: game_property::services::ReadCustodian,
) -> Result<Option<PersonId>, Error> {
    Ok(Some(PersonId(2)))
}
fn site(
    _: &Peers,
    _: &mut OwnerReadTxn<'_, '_, Peers, TestEvent>,
    _: ReadInstitutionSite,
) -> Result<Option<AreaId>, Error> {
    Ok(Some(AreaId(3)))
}
fn people(
    _: &Peers,
    tx: &mut OwnerReadTxn<'_, '_, Peers, TestEvent>,
    _: game_characters::services::ReadPeople,
) -> Result<Vec<PersonId>, Error> {
    Ok(tx
        .iter::<game_characters::people::Person>()
        .into_iter()
        .map(|(id, _)| PersonId(id.0))
        .collect())
}
fn publications(
    _: &Peers,
    tx: &mut OwnerTxn<'_, '_, Peers, TestEvent>,
    _: ReceiveRegionalPublications,
) -> Result<(), Error> {
    if *tx.runtime() {
        Err(Error::Unavailable)
    } else {
        Ok(())
    }
}
fn permit_enforcement(
    _: &Peers,
    _: &mut OwnerReadTxn<'_, '_, Peers, TestEvent>,
    _: ValidateEnforcementChange,
) -> Result<bool, Error> {
    Ok(true)
}
fn unexpected_deferred_enforcement(
    _: &Peers,
    _: &mut OwnerTxn<'_, '_, Peers, TestEvent>,
    _: ReceiveEnforcementOutcome,
) -> Result<(), Error> {
    panic!("the active receiver must receive its effect inline, once")
}

fn fixture_with_codec(
    reject: bool,
    history: bool,
) -> (GenericHost<State, TestEvent>, Registry<State, TestEvent>) {
    let mut schema = schema().unwrap();
    game_characters::people::register_people(&mut schema).unwrap();
    game_cargo::register_components(&mut schema).unwrap();
    let mut host = GenericHost::new(
        schema,
        5,
        State {
            institutions: Some(Runtime::default()),
            reject,
        },
    );
    host.allocate_ids(20).unwrap();
    for actor in [1, 2] {
        let id = StableId(actor);
        host.storage
            .insert_or_spawn(
                id,
                game_characters::people::Person {
                    id: PersonId(actor),
                    role: Role::Citizen,
                    inventory: InventoryId(actor + 5),
                    work_units: 0,
                    work_remainder: 0,
                    work_rate: 0,
                    hunger_exempt: false,
                },
            )
            .unwrap();
        host.storage
            .insert(id, crate::Knowledge::default())
            .unwrap();
    }
    host.storage
        .insert_or_spawn(
            StableId(4),
            game_cargo::Inventory {
                id: InventoryId(4),
                good: Good::Food,
                quantity: Quantity(0),
                capacity: Quantity(100),
                location: game_cargo::InventoryLocation::Area(AreaId(3)),
                access: BTreeSet::from([PersonId(2)]),
                last_granted: None,
                spoils_at: None,
            },
        )
        .unwrap();
    let mut registry = Registry::new();
    let slot = RuntimeSlot::new(|s: &State| &s.institutions, |s| &mut s.institutions);
    if history {
        crate::services::register_services_with_history(&mut registry, slot, history_codec)
            .unwrap();
    } else {
        crate::services::register_services(&mut registry, slot).unwrap();
    }
    register_native::<_, (), TestEvent>(&mut registry).unwrap();
    registry
        .register(
            Peers,
            RuntimeSlot::new(|s: &State| &s.reject, |s| &mut s.reject),
        )
        .unwrap();
    registry
        .register_read_request::<Peers, game_physiology::services::ReadBody>(read_body)
        .unwrap();
    registry
        .register_read_request::<Peers, ReadPhysical>(physical)
        .unwrap();
    registry
        .register_read_request::<Peers, ReadPlacement>(place)
        .unwrap();
    registry
        .register_read_request::<Peers, ReadStage>(stage)
        .unwrap();
    registry
        .register_read_request::<Peers, game_conflict::services::ReadCombatStatus>(combat)
        .unwrap();
    registry
        .register_read_request::<Peers, ReadMind<()>>(mind)
        .unwrap();
    registry
        .register_read_request::<Peers, ReadPerson>(person)
        .unwrap();
    registry
        .register_read_request::<Peers, game_characters::services::ReadPeople>(people)
        .unwrap();
    registry
        .register_read_request::<Peers, game_cargo::facts::ReadInventory>(stock)
        .unwrap();
    registry
        .register_read_request::<Peers, game_cargo::facts::ReadInventoryArea>(area)
        .unwrap();
    registry
        .register_read_request::<Peers, game_property::services::ReadCustodian>(custody)
        .unwrap();
    registry
        .register_read_request::<Peers, ReadInstitutionSite>(site)
        .unwrap();
    registry
        .register_request::<Peers, ReceiveRegionalPublications>(publications)
        .unwrap();
    registry
        .register_read_request::<Peers, ValidateEnforcementChange>(permit_enforcement)
        .unwrap();
    registry
        .register_request::<Peers, ReceiveEnforcementOutcome>(unexpected_deferred_enforcement)
        .unwrap();
    (host, registry)
}
fn charter() -> InstitutionCharter {
    crate::policy_fixtures::charter(Vec::new())
}
fn fixture(reject: bool) -> (GenericHost<State, TestEvent>, Registry<State, TestEvent>) {
    fixture_with_codec(reject, false)
}

#[test]
fn native_foundation_and_join_share_provisional_rows_and_publish_once() {
    let (mut host, registry) = fixture(false);
    let before = host.snapshot().unwrap();
    let prepared = registry
        .prepare(&host, SimTime(0), None, |tx| {
            tx.request(ExecuteInstitution {
                actor: PersonId(1),
                action: InstitutionAction::Found {
                    charter: charter(),
                    origin: None,
                },
            })?;
            tx.request(ExecuteInstitution {
                actor: PersonId(2),
                action: InstitutionAction::Join {
                    institution: InstitutionId(21),
                },
            })
        })
        .unwrap();
    assert_eq!(host.snapshot().unwrap(), before);
    registry.commit(&mut host, prepared).unwrap();
    let institution = crate::institution(&host.storage, InstitutionId(21)).unwrap();
    assert_eq!(institution.status, InstitutionStatus::Active);
    assert_eq!(
        institution.members,
        BTreeSet::from([PersonId(1), PersonId(2)])
    );
    assert_eq!(host.next_id(), 22);
    assert_eq!(
        host.events()
            .iter()
            .map(|record| match &record.event {
                TestEvent::Institution(event) => event,
                _ => panic!("unexpected source event"),
            })
            .collect::<Vec<_>>(),
        [
            &Event::Proposed {
                institution: InstitutionId(21),
                proposer: PersonId(1)
            },
            &Event::Founded {
                institution: InstitutionId(21)
            }
        ]
    );
    for actor in [PersonId(1), PersonId(2)] {
        assert_eq!(
            crate::knowledge(&host.storage, actor).unwrap().institutions[&InstitutionId(21)].status,
            InstitutionStatus::Active
        );
    }
}
#[test]
fn late_regional_receiver_failure_discards_foundation_rows_ids_events_and_memory() {
    let (host, registry) = fixture(true);
    let before = host.snapshot().unwrap();
    let result = registry.prepare(&host, SimTime(0), None, |tx| {
        tx.request(ExecuteInstitution {
            actor: PersonId(1),
            action: InstitutionAction::Found {
                charter: charter(),
                origin: None,
            },
        })
    });
    assert!(matches!(result, Err(Error::Unavailable)));
    assert_eq!(host.snapshot().unwrap(), before);
}
#[test]
fn source_topic_batch_deduplicates_labor_and_attributed_production() {
    use crate::reactions::{CaseAsset, CooperationTarget, SocialReceipt};
    let (mut host, registry) = fixture(false);
    let asset = CaseAsset {
        area: Some(AreaId(3)),
        inventory: InventoryId(4),
        good: Good::Food,
    };
    let prepared = registry
        .prepare(&host, SimTime(0), None, |tx| {
            tx.call::<Peers>(ReceiveSocialReceipts(vec![
                SocialReceipt::Produced {
                    actor: PersonId(1),
                    site: SiteId(20),
                    asset,
                    custodian: Some(PersonId(2)),
                },
                SocialReceipt::Performed {
                    actor: PersonId(1),
                    counterpart: PersonId(2),
                    agreement: AgreementId(10),
                    target: CooperationTarget::Labor {
                        site: SiteId(20),
                        asset,
                    },
                },
            ]))
        })
        .unwrap();
    registry.commit(&mut host, prepared).unwrap();
    assert_eq!(crate::cases(&host.storage).count(), 1);
    assert_eq!(host.next_id(), 22);
    assert_eq!(host.events().len(), 1);
    assert_eq!(
        crate::cases(&host.storage).next().unwrap().1.kind,
        SocialCaseKind::Cooperation
    );
}

#[test]
fn persisted_pending_public_receipt_replays_after_source_command_and_advances_exact_cursor() {
    let (mut host, registry) = fixture_with_codec(false, true);
    let exclusion = SocialCaseView {
        id: CaseId(20),
        kind: SocialCaseKind::Exclusion,
        subject: PersonId(1),
        counterpart: PersonId(2),
        area: AreaId(3),
        inventory: Some(InventoryId(4)),
        good: Good::Food,
        need_band: 0,
        reciprocal: false,
        outcome: SocialOutcome::Unresolved,
        observed_at: SimTime(0),
    };
    crate::insert_case(&mut host.storage, exclusion.id, exclusion).unwrap();
    crate::insert_exclusions(
        &mut host.storage,
        PersonId(1),
        crate::Exclusions {
            stores: BTreeMap::from([(InventoryId(4), CaseId(20))]),
        },
    )
    .unwrap();
    host.append_many(vec![TestEvent::Ate {
        actor: PersonId(1),
        source: InventoryId(4),
    }])
    .unwrap();
    let before = host.snapshot().unwrap();
    let prepared = registry
        .prepare(&host, SimTime(0), None, |tx| {
            tx.request(ExecuteInstitution {
                actor: PersonId(1),
                action: InstitutionAction::Found {
                    charter: charter(),
                    origin: None,
                },
            })
        })
        .unwrap();
    assert_eq!(host.snapshot().unwrap(), before);
    registry.commit(&mut host, prepared).unwrap();
    assert!(matches!(
        host.events()[1].event,
        TestEvent::Institution(Event::Proposed { .. })
    ));
    assert!(matches!(
        host.events()[2].event,
        TestEvent::Institution(Event::SocialCaseRecorded { .. })
    ));
    assert!(
        crate::exclusions(&host.storage, PersonId(1))
            .unwrap()
            .stores
            .is_empty()
    );
    assert_eq!(host.state.institutions.as_ref().unwrap().last_event, 2);
    let prepared = registry
        .prepare(&host, SimTime(0), None, |tx| {
            tx.request(ExecuteInstitution {
                actor: PersonId(2),
                action: InstitutionAction::Join {
                    institution: InstitutionId(21),
                },
            })
        })
        .unwrap();
    registry.commit(&mut host, prepared).unwrap();
    assert_eq!(crate::cases(&host.storage).count(), 2);
}

#[test]
fn enforcement_source_returns_bound_effect_without_a_duplicate_deferred_callback() {
    let (mut host, registry) = fixture(false);
    let mut institution =
        crate::policy_fixtures::institution(10, Vec::new(), InstitutionStatus::Active);
    institution.members = BTreeSet::from([PersonId(1), PersonId(2)]);
    crate::insert_institution(&mut host.storage, institution.id, institution).unwrap();
    let before = host.snapshot().unwrap();
    let prepared = registry
        .prepare(&host, SimTime(0), None, |tx| {
            let proposed = tx.request(ExecuteInstitutionForEnforcement {
                actor: PersonId(1),
                action: InstitutionAction::Propose {
                    institution: InstitutionId(10),
                    change: InstitutionChange::EnforcementPolicy {
                        office: 0,
                        policy: None,
                    },
                },
            })?;
            assert!(proposed.is_empty());
            let reactions = tx.request(ExecuteInstitutionForEnforcement {
                actor: PersonId(2),
                action: InstitutionAction::Vote {
                    proposal: RuleProposalId(21),
                    support: true,
                },
            })?;
            assert_eq!(reactions.len(), 1);
            let institution = tx.request(ReadInstitution(InstitutionId(10)))?.unwrap();
            let proposal = tx.request(ReadRuleProposal(RuleProposalId(21)))?.unwrap();
            let outcome = reactions
                .into_iter()
                .next()
                .unwrap()
                .into_outcome(&institution, &proposal)?;
            assert_eq!(outcome.status, RuleProposalStatus::Adopted);
            assert_eq!(proposal.status, RuleProposalStatus::Adopted);
            assert_eq!(institution.revision, 1);
            Ok(())
        })
        .unwrap();
    assert_eq!(host.snapshot().unwrap(), before);
    registry.commit(&mut host, prepared).unwrap();
    assert_eq!(
        crate::proposal(&host.storage, RuleProposalId(21))
            .unwrap()
            .status,
        RuleProposalStatus::Adopted
    );
}

#[test]
fn persisted_institution_event_relations_are_checked_by_the_native_read_owner() {
    let (mut host, registry) = fixture(false);
    crate::insert_institution(
        &mut host.storage,
        InstitutionId(10),
        crate::policy_fixtures::institution(10, Vec::new(), InstitutionStatus::Active),
    )
    .unwrap();
    crate::insert_institution(
        &mut host.storage,
        InstitutionId(11),
        crate::policy_fixtures::institution(11, Vec::new(), InstitutionStatus::Active),
    )
    .unwrap();
    crate::insert_proposal(
        &mut host.storage,
        RuleProposalId(12),
        RuleProposal {
            id: RuleProposalId(12),
            institution: InstitutionId(10),
            proposer: PersonId(1),
            change: InstitutionChange::Dissolve,
            votes: BTreeMap::new(),
            base_revision: 0,
            status: RuleProposalStatus::Proposed,
        },
    )
    .unwrap();
    let before = host.snapshot().unwrap();
    registry
        .inspect(
            &host,
            None,
            ValidateInstitutionEvent(Event::Changed {
                institution: InstitutionId(10),
                proposal: RuleProposalId(12),
            }),
        )
        .unwrap();
    assert!(matches!(
        registry.inspect(
            &host,
            None,
            ValidateInstitutionEvent(Event::Changed {
                institution: InstitutionId(11),
                proposal: RuleProposalId(12)
            })
        ),
        Err(Error::InvalidSave(_))
    ));
    assert!(matches!(
        registry.inspect(
            &host,
            None,
            ValidateInstitutionEvent(Event::Proposed {
                institution: InstitutionId(10),
                proposer: PersonId(99)
            })
        ),
        Err(Error::InvalidSave(_))
    ));
    assert_eq!(host.snapshot().unwrap(), before);
}
