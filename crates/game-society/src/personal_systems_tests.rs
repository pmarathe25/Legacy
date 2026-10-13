use super::*;
use game_ecs::Schema;
use game_world::{GenericHost, actions::Actions};
#[derive(Clone, Debug, PartialEq, Eq)]
struct State {
    social: Option<World>,
    reject_conversation: bool,
}
struct Peers;
struct BoundaryProbe(game_world::scheduling::ClockSpan);
impl Request for BoundaryProbe {
    type Response = Option<SimTime>;
}
fn probe_boundary(
    owner: &Systems<SocialAction>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<SocialAction>, Event>,
    query: BoundaryProbe,
) -> Result<Option<SimTime>, Error> {
    boundary(owner, tx, query.0)
}
impl OwnerSystem<Event> for Peers {
    type Runtime = bool;
    type Input = ();
    type Output = ();
    fn execute(&self, _: &mut OwnerTxn<'_, '_, Self, Event>, _: ()) -> Result<(), Error> {
        Ok(())
    }
}
fn body(
    _: &Peers,
    _: &mut OwnerReadTxn<'_, '_, Peers, Event>,
    _: ReadBody,
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
fn placement(
    _: &Peers,
    _: &mut OwnerReadTxn<'_, '_, Peers, Event>,
    _: ReadPlacement,
) -> Result<Option<game_characters::services::PlacementFacts>, Error> {
    Ok(Some(game_characters::services::PlacementFacts {
        location: Location::AtArea(AreaId(1)),
        point: None,
        can_resume: true,
    }))
}
fn combat(
    _: &Peers,
    _: &mut OwnerReadTxn<'_, '_, Peers, Event>,
    _: ReadCombatStatus,
) -> Result<game_conflict::services::CombatStatus, Error> {
    Ok(game_conflict::services::CombatStatus {
        in_combat: false,
        restricted: false,
        can_fight: Some(true),
    })
}
fn stage(
    _: &Peers,
    _: &mut OwnerReadTxn<'_, '_, Peers, Event>,
    _: ReadStage,
) -> Result<game_characters::services::StageFacts, Error> {
    Ok(game_characters::services::StageFacts {
        stage: Some(LifeStage::Adult),
        independent: true,
    })
}
fn activity(
    _: &Peers,
    tx: &mut OwnerReadTxn<'_, '_, Peers, Event>,
    q: ReadActivity,
) -> Result<Activity, Error> {
    Ok(
        if tx
            .request(game_world::actions::ReadAction {
                actor: StableId(q.0.0),
            })?
            .is_some()
        {
            Activity::Rest
        } else {
            Activity::Idle
        },
    )
}
fn conversation(
    _: &Peers,
    tx: &mut OwnerTxn<'_, '_, Peers, Event>,
    _: ReceiveConversation,
) -> Result<(), Error> {
    if *tx.runtime() {
        Err(Error::Unavailable)
    } else {
        Ok(())
    }
}
fn care(
    _: &Peers,
    _: &mut OwnerReadTxn<'_, '_, Peers, Event>,
    _: ReadCareReceived,
) -> Result<bool, Error> {
    Ok(false)
}
fn available(
    _: &Peers,
    _: &mut OwnerReadTxn<'_, '_, Peers, Event>,
    _: ReadConversationAvailability,
) -> Result<bool, Error> {
    Ok(true)
}
fn farming(
    _: &Peers,
    _: &mut OwnerReadTxn<'_, '_, Peers, Event>,
    _: ReadFarmingAccess,
) -> Result<bool, Error> {
    Ok(false)
}
fn disruption(
    _: &Peers,
    _: &mut OwnerReadTxn<'_, '_, Peers, Event>,
    _: ReadVisibleDisruption,
) -> Result<i64, Error> {
    Ok(0)
}
fn skills(
    _: &Peers,
    _: &mut OwnerReadTxn<'_, '_, Peers, Event>,
    _: ReadSkillView,
) -> Result<Vec<game_core::SkillView>, Error> {
    Ok(Vec::new())
}
fn fixture(enabled: bool, reject: bool) -> (GenericHost<State, Event>, Registry<State, Event>) {
    let mut schema = Schema::new();
    social::register_components(&mut schema).unwrap();
    game_world::actions::register_components(&mut schema).unwrap();
    schema
        .register::<Mind<SocialAction>>("test/mind", |_| Ok(()))
        .unwrap();
    game_characters::register_people(&mut schema).unwrap();
    let mut host = GenericHost::new(
        schema,
        1,
        State {
            social: enabled.then_some(World {
                rules: Rules::fixture(),
                last_review: SimTime(0),
            }),
            reject_conversation: reject,
        },
    );
    for id in [1, 2] {
        game_characters::insert_person(
            &mut host.storage,
            game_characters::Person {
                id: PersonId(id),
                role: if id == 1 { Role::Ruler } else { Role::Citizen },
                inventory: InventoryId(id + 100),
                work_units: 0,
                work_remainder: 0,
                work_rate: 0,
                hunger_exempt: false,
            },
        )
        .unwrap();
        let mut mind = Mind::<SocialAction>::new(
            game_characters::decisions::Personality::industrious(),
            Aspiration::Parenthood,
        )
        .unwrap();
        mind.relationships.insert(
            PersonId(3 - id),
            game_characters::decisions::Relationship {
                affection: 200,
                trust: 100,
            },
        );
        host.storage.insert_or_spawn(StableId(id), mind).unwrap();
        if enabled {
            let mut row = Personal {
                development: Development::baseline(
                    Temperament::from_personality(
                        &game_characters::decisions::Personality::industrious(),
                    ),
                    Aspiration::Parenthood,
                )
                .unwrap(),
                ..Default::default()
            };
            row.companionship.insert(PersonId(3 - id));
            host.storage.insert(StableId(id), row).unwrap();
        }
    }
    let mut registry = Registry::new();
    register_systems(
        &mut registry,
        RuntimeSlot::new(|s: &State| &s.social, |s| &mut s.social),
        |action| action,
    )
    .unwrap();
    registry
        .register_read_request::<Systems<SocialAction>, BoundaryProbe>(probe_boundary)
        .unwrap();
    registry
        .register(
            Peers,
            RuntimeSlot::new(
                |s: &State| &s.reject_conversation,
                |s| &mut s.reject_conversation,
            ),
        )
        .unwrap();
    registry
        .register_read_request::<Peers, ReadBody>(body)
        .unwrap();
    registry
        .register_read_request::<Peers, ReadPlacement>(placement)
        .unwrap();
    registry
        .register_read_request::<Peers, ReadCombatStatus>(combat)
        .unwrap();
    registry
        .register_read_request::<Peers, ReadStage>(stage)
        .unwrap();
    registry
        .register_read_request::<Peers, ReadActivity>(activity)
        .unwrap();
    registry
        .register_read_request::<Peers, ReadCareReceived>(care)
        .unwrap();
    registry
        .register_read_request::<Peers, ReadConversationAvailability>(available)
        .unwrap();
    registry
        .register_read_request::<Peers, ReadFarmingAccess>(farming)
        .unwrap();
    registry
        .register_read_request::<Peers, ReadVisibleDisruption>(disruption)
        .unwrap();
    registry
        .register_read_request::<Peers, ReadSkillView>(skills)
        .unwrap();
    registry
        .register_request::<Peers, ReceiveConversation>(conversation)
        .unwrap();
    Actions::new()
        .with_route("social", cancel_action::<SocialAction, Event>)
        .unwrap()
        .register(&mut registry, RuntimeSlot::stateless())
        .unwrap();
    (host, registry)
}
fn start_pair(host: &mut GenericHost<State, Event>, registry: &Registry<State, Event>) {
    let plan = registry
        .prepare(host, SimTime(0), None, |tx| {
            tx.request(StartSocial {
                actor: PersonId(1),
                action: SocialAction::Talk {
                    partner: PersonId(2),
                    topic: SocialTopic::FamilyPlans,
                },
            })
        })
        .unwrap();
    registry.commit(host, plan).unwrap();
}
#[test]
fn family_intent_is_permissive_only_when_domain_is_inactive_and_queries_are_inert() {
    for enabled in [false, true] {
        let (host, registry) = fixture(enabled, false);
        let before = host.snapshot().unwrap();
        assert_eq!(
            registry
                .inspect(
                    &host,
                    None,
                    ReadFamilyIntent {
                        actor: PersonId(1),
                        partner: PersonId(2)
                    }
                )
                .unwrap(),
            !enabled
        );
        assert_eq!(host.snapshot().unwrap(), before);
    }
}
#[test]
fn native_family_conversation_starts_completes_and_releases_both_leases() {
    let (mut host, registry) = fixture(true, false);
    start_pair(&mut host, &registry);
    assert_eq!(host.storage.iter::<Participant>().count(), 2);
    let plan = registry
        .prepare(&host, SimTime(60_000), None, |tx| {
            tx.request(CompleteSocial(PersonId(1)))
        })
        .unwrap();
    registry.commit(&mut host, plan).unwrap();
    for (actor, partner) in [(1, 2), (2, 1)] {
        assert!(
            registry
                .inspect(
                    &host,
                    None,
                    ReadFamilyIntent {
                        actor: PersonId(actor),
                        partner: PersonId(partner)
                    }
                )
                .unwrap()
        );
        assert_eq!(
            host.storage
                .get::<Personal>(StableId(actor))
                .unwrap()
                .development
                .experiences,
            1
        );
        assert!(
            host.storage
                .get::<game_world::actions::ActionSlot>(StableId(actor))
                .is_none()
        );
    }
    assert_eq!(host.storage.iter::<InteractionRow>().count(), 0);
    assert_eq!(host.events().len(), 1);
}
#[test]
fn peer_rejection_after_personal_changes_rolls_back_completion_and_leases() {
    let (mut host, registry) = fixture(true, true);
    start_pair(&mut host, &registry);
    let before = host.snapshot().unwrap();
    assert!(matches!(
        registry.prepare(&host, SimTime(60_000), None, |tx| tx
            .request(CompleteSocial(PersonId(1)))),
        Err(Error::Unavailable)
    ));
    assert_eq!(host.snapshot().unwrap(), before);
}
#[test]
fn cancelling_one_participant_removes_both_native_bindings_and_leases() {
    let (mut host, registry) = fixture(true, false);
    start_pair(&mut host, &registry);
    let plan = registry
        .prepare(&host, SimTime(1), None, |tx| {
            tx.request(game_world::actions::CancelAction { actor: StableId(2) })
        })
        .unwrap();
    registry.commit(&mut host, plan).unwrap();
    assert_eq!(host.storage.iter::<Participant>().count(), 0);
    assert_eq!(host.storage.iter::<InteractionRow>().count(), 0);
    assert_eq!(
        host.storage
            .iter::<game_world::actions::ActionSlot>()
            .count(),
        0
    );
    assert!(host.events().is_empty());
}
#[test]
fn readonly_skills_facts_and_boundary_keep_live_interaction_and_clock_unchanged() {
    let (mut host, registry) = fixture(true, false);
    start_pair(&mut host, &registry);
    let before = host.snapshot().unwrap();
    let facts = registry
        .inspect(
            &host,
            None,
            ReadSocialLearningFacts {
                from: SimTime(0),
                next: SimTime(60_000),
            },
        )
        .unwrap();
    assert_eq!(
        facts,
        vec![SocialFacts {
            interaction: InteractionFacts::Talk {
                first: PersonId(1),
                second: PersonId(2)
            },
            can_progress: true,
            completes: SimTime(60_000)
        }]
    );
    assert_eq!(
        registry
            .inspect(
                &host,
                None,
                BoundaryProbe(game_world::scheduling::ClockSpan {
                    from: SimTime(0),
                    to: SimTime(300_000),
                })
            )
            .unwrap(),
        Some(SimTime(60_000))
    );
    assert_eq!(host.snapshot().unwrap(), before);
}
#[test]
fn unsupported_practice_has_no_rows_events_or_clock_effects() {
    let (host, registry) = fixture(true, false);
    let before = host.snapshot().unwrap();
    assert!(matches!(
        registry.prepare(&host, SimTime(0), None, |tx| tx.request(StartSocial {
            actor: PersonId(1),
            action: SocialAction::Practice {
                skill: Skill::Combat
            }
        })),
        Err(Error::Unavailable)
    ));
    assert_eq!(host.snapshot().unwrap(), before);
}
#[test]
fn manual_social_availability_uses_actual_local_adults_without_mutating_choice_state() {
    let (host, registry) = fixture(true, false);
    let before = host.snapshot().unwrap();
    assert_eq!(
        registry
            .inspect(&host, None, AvailableSocial(PersonId(1)))
            .unwrap(),
        vec![
            SocialAction::Talk {
                partner: PersonId(2),
                topic: SocialTopic::Companionship
            },
            SocialAction::Talk {
                partner: PersonId(2),
                topic: SocialTopic::FamilyPlans
            },
        ]
    );
    assert_eq!(host.snapshot().unwrap(), before);
}
