use crate::transition_tests::*;
use crate::*;
use game_core::*;
use game_ecs::{Batch, StableId};
use std::collections::{BTreeMap, BTreeSet};

fn basic() -> crate::fixture::Fixture {
    let mut w = state();
    let mut p = proposal();
    p.supplies.clear();
    p.deadline = SimTime(200_000);
    start(&mut w, p, &facts());
    w
}
struct Scene {
    areas: BTreeMap<PersonId, AreaId>,
    combat: bool,
}
impl PresenceFacts for Scene {
    fn location(&self, p: PersonId) -> Location {
        Location::AtArea(self.areas[&p])
    }
    fn raw_location(&self, p: PersonId) -> Location {
        self.location(p)
    }
    fn area(&self, p: PersonId) -> Option<AreaId> {
        self.areas.get(&p).copied()
    }
    fn same_place(&self, a: PersonId, b: PersonId) -> bool {
        self.areas.get(&a) == self.areas.get(&b)
    }
    fn in_combat(&self, _: PersonId) -> bool {
        self.combat
    }
    fn combat_scene(
        &self,
        _: PersonId,
    ) -> Option<(&game_conflict::Encounter, &game_conflict::Map)> {
        None
    }
}
impl CommandAvailabilityFacts for Scene {
    fn area(&self, p: PersonId) -> Option<AreaId> {
        self.areas.get(&p).copied()
    }
    fn in_combat(&self, _: PersonId) -> bool {
        self.combat
    }
    fn urgent_care(&self, _: PersonId) -> bool {
        false
    }
    fn guardian_duty(&self, _: PersonId, _: MissionId) -> bool {
        false
    }
    fn local_people(&self, area: AreaId) -> impl Iterator<Item = PersonId> {
        self.areas
            .iter()
            .filter_map(move |(p, a)| (*a == area).then_some(*p))
    }
}
struct Context;
impl ValidationFacts for Context {
    fn inventory(&self, _: InventoryId) -> Option<&game_cargo::Inventory> {
        None
    }
    fn agreement_parties(&self, _: AgreementId) -> Option<&BTreeSet<PersonId>> {
        None
    }
    fn talk_deadline(&self, _: PersonId) -> Option<SimTime> {
        None
    }
    fn at_area(&self, _: PersonId) -> Option<AreaId> {
        Some(HOME)
    }
    fn talking_people(&self) -> impl Iterator<Item = PersonId> {
        std::iter::empty()
    }
}
fn contextual(
    w: &crate::fixture::Fixture,
    people: &BTreeSet<PersonId>,
    next_id: u64,
) -> Result<(), Error> {
    validate_native_context(
        Some(&w.runtime),
        &w.store,
        ValidationCatalog {
            now: SimTime(0),
            next_id,
            next_event: 1,
            people,
            areas: &BTreeSet::from([HOME, AWAY]),
            carrying_capacity: Quantity(5000),
        },
        RuntimeAvailability {
            physical: true,
            decisions: true,
            conflict: true,
            agreements: true,
        },
        &Context,
    )
}
#[test]
fn native_context_rejects_orphan_personal_knowledge_allocator_aliases_and_future_provenance() {
    let mut w = basic();
    let people = BTreeSet::from([SP, A, B]);
    contextual(&w, &people, 1000).unwrap();
    assert!(contextual(&w, &people, 13).is_err());
    let scene = Scene {
        areas: BTreeMap::from([(SP, HOME), (A, HOME), (B, HOME)]),
        combat: false,
    };
    let f = w
        .observation_facts(&w.store, A, ID, SimTime(0), &scene)
        .unwrap();
    w.prepare_observation(&w.store, A, ID, &f)
        .unwrap()
        .commit(&mut w.store)
        .unwrap();
    let copy = knowledge(&w.store, A).unwrap().clone();
    insert_knowledge(&mut w.store, PersonId(999), copy).unwrap();
    assert!(contextual(&w, &people, 1000).is_err());
    remove_knowledge(&mut w.store, PersonId(999)).unwrap();
    knowledge_mut(&mut w.store, A)
        .unwrap()
        .missions
        .get_mut(&ID)
        .unwrap()
        .observed_at = SimTime(1);
    assert!(contextual(&w, &people, 1000).is_err());
}
#[test]
fn native_owner_perception_and_available_commands_keep_remote_actor_dated_and_empty() {
    let mut w = basic();
    let mut scene = Scene {
        areas: BTreeMap::from([(SP, HOME), (A, HOME), (B, HOME)]),
        combat: false,
    };
    let f = w
        .observation_facts(&w.store, B, ID, SimTime(0), &scene)
        .unwrap();
    w.prepare_observation(&w.store, B, ID, &f)
        .unwrap()
        .commit(&mut w.store)
        .unwrap();
    assert!(w.available_commands(&w.store, B, &scene).iter().any(|action| matches!(action, MissionAction::Accept { recruitment } if *recruitment == RecruitmentId(13))));
    scene.areas.insert(B, AWAY);
    assert!(
        !w.available_commands(&w.store, B, &scene)
            .iter()
            .any(|action| matches!(action, MissionAction::Accept { .. }))
    );
    let f = w
        .observation_facts(&w.store, B, ID, SimTime(10), &scene)
        .unwrap();
    w.prepare_observation(&w.store, B, ID, &f)
        .unwrap()
        .commit(&mut w.store)
        .unwrap();
    assert_eq!(
        knowledge(&w.store, B).unwrap().missions[&ID].observed_at,
        SimTime(0)
    );
    assert!(!w.receivers(&w.store, ID, &scene).contains(&B));
}
#[test]
fn timed_native_speech_prepares_only_local_order_after_deadline_and_rejects_stale_or_foreign_rows()
{
    let mut w = basic();
    accept(&mut w, A, OFFER, &facts());
    let scene = TalkScene {
        speaker_area: Some(HOME),
        listener_area: Some(HOME),
        speaker_combat: false,
        listener_combat: false,
        active_deadline: None,
    };
    let conversation = Conversation::Objective {
        mission: ID,
        leader: A,
        objective: MissionObjective::Visit { destination: HOME },
    };
    let start = w
        .prepare_talk_start(&w.store, SP, A, conversation, SimTime(0), scene)
        .unwrap();
    assert_eq!(start.talk.completes, SimTime(60_000));
    let mut batch = Batch::new();
    start.stage_into(&w.store, &mut batch).unwrap();
    w.store.apply_batch(batch).unwrap();
    assert_eq!(
        mission(&w.store, ID).unwrap().proposal.objective,
        MissionObjective::Visit { destination: AWAY }
    );
    assert!(w.due_talks(&w.store, SimTime(59_999)).is_empty());
    assert_eq!(
        w.boundary(&w.store, SimTime(0), SimTime(200_000)),
        SimTime(60_000)
    );
    let scene = TalkScene {
        active_deadline: Some(SimTime(60_000)),
        ..scene
    };
    let mut authority = facts();
    authority.now = SimTime(60_000);
    let finish = w
        .prepare_talk_completion(&w.store, SP, SimTime(60_000), scene, Some(&authority))
        .unwrap();
    assert_eq!(
        finish.order.as_ref().unwrap().objective,
        MissionObjective::Visit { destination: HOME }
    );
    let before = w.clone();
    let mut batch = Batch::new();
    batch.remove::<PendingTalk>(StableId(SP.0));
    assert!(finish.stage_into(&w.store, &mut batch).is_err());
    assert_eq!(w, before);
    let finish = w
        .prepare_talk_completion(&w.store, SP, SimTime(60_000), scene, Some(&authority))
        .unwrap();
    talk_mut(&mut w.store, SP).unwrap().completes = SimTime(60_001);
    assert!(finish.stage_into(&w.store, &mut Batch::new()).is_err());
}
#[test]
fn native_receipt_and_allocation_failures_keep_records_and_event_cursor_unchanged() {
    let mut w = basic();
    accept(&mut w, A, OFFER, &facts());
    let before = w.clone();
    assert!(
        w.command_ids(
            &w.store,
            &MissionAction::Propose {
                proposal: proposal()
            },
            u64::MAX
        )
        .is_err()
    );
    let receipt = MissionReceipt::Departed {
        actor: A,
        from: HOME,
    };
    let change = w.prepare_receipt(&w.store, EventId(1), &receipt).unwrap();
    let mut batch = Batch::new();
    batch.remove::<Mission>(StableId(ID.0));
    assert!(change.stage_into(&w.runtime, &w.store, &mut batch).is_err());
    assert_eq!(w, before);
    let change = w.prepare_receipt(&w.store, EventId(1), &receipt).unwrap();
    let mut batch = Batch::new();
    let _finish = change.stage_into(&w.runtime, &w.store, &mut batch).unwrap();
    batch.remove::<Mission>(StableId(0));
    assert!(w.store.apply_batch(batch).is_err());
    assert_eq!(w, before);
}
#[test]
fn typed_talk_replacement_composes_cancellation_and_rejects_an_unproven_foreign_removal() {
    let mut w = basic();
    let old = PendingTalk {
        speaker: SP,
        listener: A,
        area: HOME,
        completes: SimTime(60_000),
        conversation: Conversation::Retreat {
            mission: ID,
            leader: A,
        },
    };
    insert_talk(&mut w.store, SP, old).unwrap();
    let scene = TalkScene {
        speaker_area: Some(HOME),
        listener_area: Some(HOME),
        speaker_combat: false,
        listener_combat: false,
        active_deadline: Some(SimTime(60_000)),
    };
    let cancellation = w.prepare_cancel_talks(&w.store, &[SP]);
    let replacement = w
        .prepare_talk_start(
            &w.store,
            SP,
            A,
            Conversation::Objective {
                mission: ID,
                leader: A,
                objective: MissionObjective::Visit { destination: HOME },
            },
            SimTime(1),
            scene,
        )
        .unwrap();
    let mut batch = Batch::new();
    let token = replacement
        .stage_into_with_replacement(&w.store, &mut batch)
        .unwrap();
    cancellation
        .stage_into_composed(&w.store, &mut batch, Some(&token))
        .unwrap();
    w.store.apply_batch(batch).unwrap();
    assert_eq!(talk(&w.store, SP).unwrap().completes, SimTime(60_001));
    let cancellation = w.prepare_cancel_talks(&w.store, &[SP]);
    let mut batch = Batch::new();
    batch.remove::<PendingTalk>(StableId(SP.0));
    assert!(cancellation.stage_into(&w.store, &mut batch).is_err());
}

impl CommandFactSource for Scene {
    fn assets(
        &self,
        _: Option<&MissionObjective>,
        _: &[game_agreements::AgreementTerm],
        _: &[MissionSupply],
    ) -> MissionAssets {
        facts().assets
    }
}
#[test]
fn ordinary_action_admission_does_not_remove_actual_tactical_authority() {
    let mut w = basic();
    accept(&mut w, A, OFFER, &facts());
    let scene = Scene {
        areas: BTreeMap::from([(SP, HOME), (A, HOME), (B, HOME)]),
        combat: true,
    };
    assert!(w.validate_action_presence(SP, &scene).is_err());
    let objective = MissionObjective::Visit { destination: HOME };
    let f = w
        .command_facts(
            &w.store,
            SP,
            HOME,
            &MissionAction::Objective {
                mission: ID,
                objective: objective.clone(),
            },
            SimTime(0),
            &scene,
        )
        .unwrap();
    w.validate_order(&w.store, SP, ID, &objective, &f).unwrap();
}
#[test]
fn borrowed_actual_fighter_rows_feed_owned_defection_without_a_foreign_world() {
    let mut w = basic();
    accept(&mut w, B, RecruitmentId(13), &facts());
    let opponent = game_conflict::Fighter {
        actor: ENEMY,
        joins_round: 0,
        team: 1,
        position: Point { x: 0, y: 0 },
        surrendered: false,
        withdrawn: false,
        defending: false,
        retreat: None,
        known: BTreeMap::new(),
        search: game_conflict::SearchState::default(),
    };
    let relationships = BTreeMap::from([(
        ENEMY,
        game_characters::decisions::Relationship {
            trust: 300,
            affection: 0,
        },
    )]);
    let plan = w
        .prepare_combat_defection(
            &w.store,
            B,
            ID,
            CombatDefectionFacts {
                player: SP,
                own_team: 0,
                motives: ServiceMotives {
                    honesty: 100,
                    leader_trust: -900,
                    guardian_duty: false,
                    payment: None,
                },
                relationships: &relationships,
            },
            [&opponent],
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        plan.allegiance,
        Some(AllegianceEffect {
            actor: B,
            opponent: ENEMY,
            team: 1
        })
    );
    plan.commit(&mut w.runtime, &mut w.store).unwrap();
    assert_eq!(
        mission(&w.store, ID).unwrap().members[&B].status,
        CompanionStatus::Defected
    );
}
#[test]
fn supplied_target_reports_validate_context_and_change_only_native_roster_contacts() {
    let mut w = state();
    let mut p = proposal();
    p.supplies.clear();
    p.objective = MissionObjective::Defeat { person: ENEMY };
    start(&mut w, p, &facts());
    let plan = w
        .prepare_target_report(
            &w.store,
            ID,
            TargetReport {
                speaker: SP,
                target: ENEMY,
                area: AWAY,
                observed_at: SimTime(5),
            },
            &TargetReportFacts {
                speaker_exists: true,
                target_exists: true,
                area_exists: true,
                now: SimTime(10),
            },
        )
        .unwrap();
    plan.commit(&mut w.runtime, &mut w.store).unwrap();
    assert!(
        mission(&w.store, ID)
            .unwrap()
            .members
            .values()
            .all(|m| m.contacts[&ENEMY] == AWAY)
    );
    let before = w.clone();
    assert!(
        w.prepare_target_report(
            &w.store,
            ID,
            TargetReport {
                speaker: A,
                target: ENEMY,
                area: HOME,
                observed_at: SimTime(5)
            },
            &TargetReportFacts {
                speaker_exists: true,
                target_exists: true,
                area_exists: true,
                now: SimTime(10)
            }
        )
        .is_err()
    );
    assert_eq!(w, before);
}
#[test]
fn rejected_native_compensation_cannot_accept_recruitment_or_publish_service_metadata() {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    schema
        .register::<game_agreements::Agreement>(
            "agreements/agreement",
            game_agreements::Agreement::validate,
        )
        .unwrap();
    schema
        .register::<game_agreements::Obligation>("agreements/obligation", |_| {
            Err(Error::Unavailable)
        })
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
    let mut w = state();
    w.store = game_ecs::Store::with_schema(schema);
    let f = facts();
    let mut p = proposal();
    p.compensation.insert(A, vec![payment()]);
    start(&mut w, p, &f);
    let mission = w
        .prepare_command(
            &w.store,
            A,
            &MissionAction::Accept { recruitment: OFFER },
            &f,
            &MissionIds {
                backing: Some(AgreementId(30)),
                ..Default::default()
            },
        )
        .unwrap();
    let consent = mission.consent.as_ref().unwrap();
    let agreement = game_agreements::Ledger::new(&w.store)
        .prepare_service_commitment(
            &game_agreements::HeardServiceConsent {
                recruitment: consent.recruitment,
                sponsor: consent.sponsor,
                recruit: consent.recruit,
                offered_terms: recruitment(&w.store, OFFER).unwrap().compensation.clone(),
                accepted_terms: consent.terms.clone(),
                unanswered_counteroffer: false,
            },
            &game_agreements::TransitionFacts {
                actor_available: true,
                present: BTreeSet::from([SP, A]),
                assets: f.assets.terms.clone(),
            },
            &game_agreements::TransitionIds {
                agreement: Some(AgreementId(30)),
                obligations: vec![ObligationId(31)],
                ..Default::default()
            },
        )
        .unwrap();
    let before = w.store.clone();
    let metadata = w.runtime.clone();
    let mut batch = Batch::new();
    agreement
        .check(&game_agreements::Ledger::new(&w.store))
        .unwrap();
    agreement.stage(&mut batch);
    let _finish = mission
        .stage_into(&w.runtime, &w.store, &mut batch)
        .unwrap();
    assert_eq!(w.store.apply_batch(batch), Err(Error::Unavailable));
    assert_eq!(w.store, before);
    assert_eq!(w.runtime, metadata);
    assert_eq!(recruitment(&w.store, OFFER).unwrap().agreement, None);
    assert_eq!(
        crate::mission(&w.store, ID).unwrap().members[&A].status,
        CompanionStatus::Offered
    );
    assert!(game_agreements::agreement(&w.store, AgreementId(30)).is_none());
    assert!(!w.store.contains(StableId(31)));
}

fn projected_creation(w: &crate::fixture::Fixture) -> PreparedMission {
    w.prepare_command(
        &w.store,
        SP,
        &MissionAction::Propose {
            proposal: proposal(),
        },
        &facts(),
        &MissionIds {
            mission: Some(ID),
            recruitments: BTreeMap::from([(A, OFFER), (B, RecruitmentId(13))]),
            backing: None,
        },
    )
    .unwrap()
}
#[test]
fn created_mission_publication_matches_local_observation_and_keeps_remote_member_ignorant() {
    let mut w = state();
    let scene = Scene {
        areas: BTreeMap::from([(SP, HOME), (A, HOME), (B, AWAY)]),
        combat: false,
    };
    let mut legacy = state();
    projected_creation(&legacy)
        .commit(&mut legacy.runtime, &mut legacy.store)
        .unwrap();
    for actor in legacy.runtime.receivers(&legacy.store, ID, &scene) {
        let facts = legacy
            .runtime
            .observation_facts(&legacy.store, actor, ID, SimTime(0), &scene)
            .unwrap();
        legacy
            .runtime
            .prepare_observation(&legacy.store, actor, ID, &facts)
            .unwrap()
            .commit(&mut legacy.store)
            .unwrap();
    }
    let plan = projected_creation(&w);
    let before = w.store.snapshot().unwrap();
    let mut batch = Batch::new();
    let (finish, guard) = plan
        .stage_created_with_publications(&w.runtime, &w.store, SimTime(0), &scene, &mut batch)
        .unwrap();
    assert_eq!(w.store.snapshot().unwrap(), before);
    assert!(guard.validate_batch(&Batch::new()).is_err());
    guard.validate_batch(&batch).unwrap();
    w.store.apply_batch(batch).unwrap();
    finish.finish(&mut w.runtime);
    assert_eq!(
        w.store.snapshot().unwrap(),
        legacy.store.snapshot().unwrap()
    );
    assert!(known_mission(&w.store, SP, ID).is_some());
    assert!(known_mission(&w.store, A, ID).is_some());
    assert!(known_mission(&w.store, B, ID).is_none());
}
#[test]
fn created_mission_publication_rejects_foreign_knowledge_and_source_replacements() {
    let w = state();
    let scene = Scene {
        areas: BTreeMap::from([(SP, HOME), (A, HOME), (B, AWAY)]),
        combat: false,
    };
    for source in [false, true] {
        let mut batch = Batch::new();
        let (_, guard) = projected_creation(&w)
            .stage_created_with_publications(&w.runtime, &w.store, SimTime(0), &scene, &mut batch)
            .unwrap();
        if source {
            batch.remove::<Mission>(StableId(ID.0));
        } else {
            batch.insert(StableId(SP.0), Knowledge::default());
        }
        assert!(guard.validate_batch(&batch).is_err());
        assert!(mission(&w.store, ID).is_none());
    }
}

struct Execution {
    world: crate::fixture::Fixture,
    scene: Scene,
    index: MembershipIndex,
    now: SimTime,
    commands: Vec<(PersonId, MissionAction)>,
    controlled: Vec<(MissionId, PersonId)>,
}
impl Execution {
    fn new() -> Self {
        let world = basic();
        let index = MembershipIndex::build(&world.store);
        Self {
            world,
            index,
            now: SimTime(60_000),
            scene: Scene {
                areas: BTreeMap::from([(SP, HOME), (A, HOME), (B, HOME)]),
                combat: false,
            },
            commands: Vec::new(),
            controlled: Vec::new(),
        }
    }
}
impl MissionExecution for Execution {
    fn runtime(&self) -> &Runtime {
        &self.world.runtime
    }
    fn store(&self) -> &game_ecs::Store {
        &self.world.store
    }
    fn parts_mut(&mut self) -> (&mut Runtime, &mut game_ecs::Store) {
        (&mut self.world.runtime, &mut self.world.store)
    }
    fn now(&self) -> SimTime {
        self.now
    }
    fn presence(&self) -> impl PresenceFacts + CommandAvailabilityFacts {
        Scene {
            areas: self.scene.areas.clone(),
            combat: self.scene.combat,
        }
    }
    fn membership_index(&self) -> &MembershipIndex {
        &self.index
    }
    fn player(&self) -> PersonId {
        SP
    }
    fn preflight_review(&self) -> Result<(), Error> {
        Ok(())
    }
    fn recruitment_input(&self, actor: PersonId, _: &Mission) -> RecruitmentInput {
        input(actor)
    }
    fn personal_inventory(&self, _: PersonId) -> InventoryId {
        InventoryId(21)
    }
    fn known_stocks(&self, _: PersonId) -> Vec<StockKnowledge> {
        Vec::new()
    }
    fn command(&mut self, actor: PersonId, action: &MissionAction) -> Result<(), Error> {
        self.commands.push((actor, action.clone()));
        let mut facts = facts();
        facts.now = self.now;
        let plan = self.world.runtime.prepare_command(
            &self.world.store,
            actor,
            action,
            &facts,
            &MissionIds::default(),
        )?;
        plan.commit(&mut self.world.runtime, &mut self.world.store)
    }
    fn control_member(&mut self, id: MissionId, actor: PersonId) {
        self.controlled.push((id, actor));
    }
    fn commit_plan(&mut self, plan: PreparedMission) -> Result<(), Error> {
        plan.commit(&mut self.world.runtime, &mut self.world.store)
    }
    fn observation_facts(&self, actor: PersonId, id: MissionId) -> ObservationFacts {
        self.world
            .runtime
            .observation_facts(&self.world.store, actor, id, self.now, &self.scene)
            .unwrap()
    }
    fn member_body(&self, actor: PersonId) -> MemberBody {
        MemberBody {
            alive: true,
            captive: false,
            area: self.scene.areas.get(&actor).copied(),
        }
    }
    fn objective_knowledge(&self, _: PersonId, _: &MissionObjective) -> ObjectiveKnowledge {
        ObjectiveKnowledge::default()
    }
    fn talk_scene(&self, speaker: PersonId, listener: PersonId) -> TalkScene {
        TalkScene {
            speaker_area: self.scene.areas.get(&speaker).copied(),
            listener_area: self.scene.areas.get(&listener).copied(),
            speaker_combat: false,
            listener_combat: false,
            active_deadline: talk(&self.world.store, speaker).map(|t| t.completes),
        }
    }
    fn order_authority(&self, _: PersonId, _: &CommunicatedOrder) -> Option<CommandFacts> {
        let mut f = facts();
        f.now = self.now;
        Some(f)
    }
    fn commit_talk(&mut self, plan: PreparedTalkCompletion) -> Result<Vec<MissionEvent>, Error> {
        let events = plan.events.clone();
        let mut batch = Batch::new();
        plan.stage_into(&self.world.store, &mut batch)?;
        self.world.store.apply_batch(batch)?;
        Ok(events)
    }
    fn publish_talk_events(&mut self, _: Vec<MissionEvent>) {}
    fn prepare_order(&self, order: &CommunicatedOrder) -> Result<PreparedMission, Error> {
        self.world.runtime.prepare_order(
            &self.world.store,
            order.mission,
            order.leader,
            &order.objective,
            &OrderFacts {
                now: self.now,
                heard: BTreeMap::from([(A, ServiceChoice::DeclaredAcceptance)]),
                assets: facts().assets,
            },
            order.retreat,
        )
    }
}
#[test]
fn production_review_loop_accepts_locally_then_departs_and_uses_live_membership() {
    let mut port = Execution::new();
    review_missions(&mut port).unwrap();
    assert_eq!(port.commands.len(), 3);
    assert_eq!(
        mission(&port.world.store, ID).unwrap().status,
        MissionStatus::Traveling
    );
    assert_eq!(port.controlled, vec![(ID, A), (ID, B)]);
    let snapshot = port.world.store.snapshot().unwrap();
    review_missions(&mut port).unwrap();
    assert_eq!(port.commands.len(), 3);
    assert_eq!(port.world.store.snapshot().unwrap(), snapshot);
}
#[test]
fn production_receipt_outcome_and_observation_loop_uses_actual_arrival_and_local_reception() {
    let mut port = Execution::new();
    review_missions(&mut port).unwrap();
    port.scene.areas.insert(A, AWAY);
    port.scene.areas.insert(B, AWAY);
    reconcile_receipts(
        &mut port,
        [(
            EventId(1),
            ReceiptInput::Arrived {
                actor: A,
                area: AWAY,
                contacts: BTreeSet::from([A, B]),
            },
        )],
    )
    .unwrap();
    reconcile_progress(&mut port).unwrap();
    assert_eq!(
        mission(&port.world.store, ID).unwrap().status,
        MissionStatus::Returning
    );
    assert!(
        mission(&port.world.store, ID).unwrap().members[&A]
            .visited
            .contains(&AWAY)
    );
    assert_eq!(
        known_mission(&port.world.store, A, ID).unwrap().status,
        MissionStatus::Returning
    );
    assert!(known_mission(&port.world.store, SP, ID).is_none());
}
#[test]
fn army_retreat_owner_reaction_preserves_unheard_companions_and_is_stale_guarded() {
    let mut world = basic();
    accept(&mut world, A, OFFER, &facts());
    let companion = mission(&world.store, ID).unwrap().members[&B].clone();
    let objective = world
        .runtime
        .army_retreat_objective(&world.store, ID)
        .unwrap();
    let order_facts = OrderFacts {
        now: SimTime(0),
        heard: BTreeMap::from([(A, ServiceChoice::DeclaredAcceptance)]),
        assets: facts().assets,
    };
    let plan = world
        .runtime
        .prepare_army_directive(&world.store, ID, A, &objective, &order_facts, true)
        .unwrap();
    mission_mut(&mut world.store, ID).unwrap().explanation = "Changed since preparation".into();
    assert!(plan.commit(&mut world.runtime, &mut world.store).is_err());
    world
        .runtime
        .prepare_army_directive(&world.store, ID, A, &objective, &order_facts, true)
        .unwrap()
        .commit(&mut world.runtime, &mut world.store)
        .unwrap();
    assert_eq!(mission(&world.store, ID).unwrap().members[&B], companion);
    assert_eq!(
        mission(&world.store, ID).unwrap().status,
        MissionStatus::Returning
    );
    assert!(!mission(&world.store, ID).unwrap().succeeded);
}

impl ObjectiveFacts for Scene {
    fn stock(&self, _: PersonId, _: InventoryId) -> Option<StockKnowledge> {
        None
    }
    fn target_observation(&self, target: PersonId) -> Option<TargetObservation> {
        Some(TargetObservation {
            person: target,
            area: *self.areas.get(&target)?,
            alive: true,
            health: 100,
            incapacitation: 10,
            captor: None,
            restricted: false,
            observed_at: SimTime(10),
        })
    }
}
#[test]
fn production_objective_fact_selection_does_not_observe_remote_target_truth() {
    let world = basic();
    let objective = MissionObjective::Defeat { person: ENEMY };
    let mut scene = Scene {
        areas: BTreeMap::from([(A, HOME), (ENEMY, AWAY)]),
        combat: false,
    };
    assert!(
        world
            .runtime
            .objective_knowledge(A, &objective, &scene)
            .target
            .is_none()
    );
    scene.areas.insert(ENEMY, HOME);
    assert_eq!(
        world
            .runtime
            .objective_knowledge(A, &objective, &scene)
            .target
            .unwrap()
            .observed_at,
        SimTime(10)
    );
    scene.combat = true;
    assert!(
        world
            .runtime
            .objective_knowledge(A, &objective, &scene)
            .target
            .is_none()
    );
}

#[test]
fn accepted_army_directive_stages_companion_consent_and_retreat_together() {
    let mut world = basic();
    accept(&mut world, A, OFFER, &facts());
    accept(&mut world, B, RecruitmentId(13), &facts());
    command(
        &mut world,
        A,
        MissionAction::Depart { mission: ID },
        &facts(),
        MissionIds::default(),
    );
    let before = world.store.snapshot().unwrap();
    let objective = MissionObjective::Visit { destination: HOME };
    let plan = world
        .runtime
        .prepare_army_directive(
            &world.store,
            ID,
            A,
            &objective,
            &OrderFacts {
                now: SimTime(60_000),
                heard: BTreeMap::from([
                    (A, ServiceChoice::DeclaredAcceptance),
                    (B, ServiceChoice::DeclaredAcceptance),
                ]),
                assets: facts().assets,
            },
            true,
        )
        .unwrap();
    let mut batch = Batch::new();
    let (finish, guard) = plan
        .stage_guarded(&world.runtime, &world.store, &mut batch)
        .unwrap();
    assert_eq!(world.store.snapshot().unwrap(), before);
    guard.validate(&world.store, &batch).unwrap();
    world.store.apply_batch(batch).unwrap();
    finish.finish(&mut world.runtime);
    let mission = mission(&world.store, ID).unwrap();
    assert_eq!(mission.status, MissionStatus::Returning);
    assert_eq!(mission.proposal.objective, objective);
    assert_eq!(mission.members[&B].directive, objective);
    assert_eq!(mission.members[&B].status, CompanionStatus::Accepted);
    assert!(mission.explanation.contains("army order"));
}

#[test]
fn production_native_validation_rejects_missing_runtime_or_required_collaborators() {
    let world = basic();
    let people = BTreeSet::from([SP, A, B]);
    let areas = BTreeSet::from([HOME, AWAY]);
    let catalog = || ValidationCatalog {
        now: SimTime(0),
        next_id: 1000,
        next_event: 1,
        people: &people,
        areas: &areas,
        carrying_capacity: Quantity(5000),
    };
    let available = RuntimeAvailability {
        physical: true,
        decisions: true,
        conflict: true,
        agreements: true,
    };
    assert!(validate_native_context(None, &world.store, catalog(), available, &Context).is_err());
    assert!(
        validate_native_context(
            Some(&world.runtime),
            &world.store,
            catalog(),
            RuntimeAvailability {
                conflict: false,
                ..available
            },
            &Context
        )
        .is_err()
    );
    assert!(
        validate_native_context(
            Some(&world.runtime),
            &world.store,
            catalog(),
            available,
            &Context
        )
        .is_ok()
    );
}

#[test]
fn generic_service_guard_rejects_later_owner_write_and_allows_unrelated_person() {
    let mut world = basic();
    accept(&mut world, A, OFFER, &facts());
    for conflicting in [false, true] {
        let plan = world
            .runtime
            .prepare_explanation(&world.store, ID, A, "Changed native explanation")
            .unwrap();
        let mut batch = Batch::new();
        let (_, guard) = plan
            .stage_guarded(&world.runtime, &world.store, &mut batch)
            .unwrap();
        if conflicting {
            batch.insert(StableId(ID.0), mission(&world.store, ID).unwrap().clone());
        } else {
            batch.insert(StableId(999), Knowledge::default());
        }
        assert_eq!(guard.validate(&world.store, &batch).is_err(), conflicting);
    }
    assert_eq!(
        mission(&world.store, ID).unwrap().status,
        MissionStatus::Preparing
    );
}

#[test]
fn army_retreat_keeps_existing_admission_for_preparing_and_returning_detachments() {
    for status in [MissionStatus::Preparing, MissionStatus::Returning] {
        let mut world = basic();
        accept(&mut world, A, OFFER, &facts());
        mission_mut(&mut world.store, ID).unwrap().status = status;
        let objective = world
            .runtime
            .army_retreat_objective(&world.store, ID)
            .unwrap();
        world
            .runtime
            .prepare_army_directive(
                &world.store,
                ID,
                A,
                &objective,
                &OrderFacts {
                    now: SimTime(60_000),
                    heard: BTreeMap::from([(A, ServiceChoice::DeclaredAcceptance)]),
                    assets: facts().assets,
                },
                true,
            )
            .unwrap()
            .commit(&mut world.runtime, &mut world.store)
            .unwrap();
        assert_eq!(
            mission(&world.store, ID).unwrap().status,
            MissionStatus::Returning
        );
    }
}
#[test]
fn saved_native_view_validation_checks_frozen_recruitment_references_and_dates() {
    let mut world = basic();
    let scene = Scene {
        areas: BTreeMap::from([(SP, HOME), (A, HOME), (B, HOME)]),
        combat: false,
    };
    let f = world
        .runtime
        .observation_facts(&world.store, SP, ID, SimTime(0), &scene)
        .unwrap();
    let mut view = world
        .runtime
        .prepare_observation(&world.store, SP, ID, &f)
        .unwrap()
        .snapshot;
    let people = BTreeSet::from([SP, A, B]);
    let areas = BTreeSet::from([HOME, AWAY]);
    validate_saved_view(&world.store, &view, SimTime(0), &people, &areas, true).unwrap();
    view.observed_at = SimTime(1);
    assert!(validate_saved_view(&world.store, &view, SimTime(0), &people, &areas, true).is_err());
    view.observed_at = SimTime(0);
    remove_recruitment(&mut world.store, OFFER).unwrap();
    assert!(validate_saved_view(&world.store, &view, SimTime(0), &people, &areas, true).is_err());
}

#[test]
fn native_defection_and_conflict_allegiance_publish_together_through_owner_guards() {
    let mut world = basic();
    accept(&mut world, B, RecruitmentId(13), &facts());
    let mut schema = world.store.schema().clone();
    game_conflict::register_components(&mut schema).unwrap();
    let old = world.store.snapshot().unwrap();
    let mut expanded = game_ecs::Store::with_schema(schema.clone())
        .snapshot()
        .unwrap();
    expanded.entities = old.entities;
    expanded.columns.extend(old.columns);
    world.store = game_ecs::Store::restore(schema, expanded).unwrap();
    let actor = game_conflict::Fighter {
        actor: B,
        joins_round: 0,
        team: 0,
        position: Point { x: 0, y: 0 },
        surrendered: false,
        withdrawn: false,
        defending: false,
        retreat: None,
        known: BTreeMap::new(),
        search: game_conflict::SearchState::default(),
    };
    let opponent = game_conflict::Fighter {
        actor: ENEMY,
        team: 1,
        ..actor.clone()
    };
    let encounter = EncounterId(500);
    game_conflict::insert_encounter(
        &mut world.store,
        game_conflict::Encounter {
            id: encounter,
            area: HOME,
            location: Location::AtArea(HOME),
            map: CombatMapId(501),
            round: 0,
            started: SimTime(0),
            ends: SimTime(6),
            fighters: BTreeMap::from([(B, actor), (ENEMY, opponent.clone())]),
            order: vec![B, ENEMY],
            cursor: 0,
            active: true,
        },
    )
    .unwrap();
    let conflict = game_conflict::World {
        rules: game_conflict::Rules {
            encounter_limit: 128,
            round: SimDuration(6),
            movement: 1000,
            reach: 1000,
            damage: 10,
            defence: 2,
            incapacitation: 10,
        },
    };
    let relationships = BTreeMap::from([(
        ENEMY,
        game_characters::decisions::Relationship {
            trust: 300,
            affection: 0,
        },
    )]);
    let service = world
        .runtime
        .prepare_combat_defection(
            &world.store,
            B,
            ID,
            CombatDefectionFacts {
                player: SP,
                own_team: 0,
                motives: ServiceMotives {
                    honesty: 100,
                    leader_trust: -900,
                    guardian_duty: false,
                    payment: None,
                },
                relationships: &relationships,
            },
            [&opponent],
        )
        .unwrap()
        .unwrap();
    let allegiance = conflict
        .prepare_allegiance(&world.store, encounter, B, ENEMY, 0, 1)
        .unwrap();
    let before = world.store.snapshot().unwrap();
    let mut batch = Batch::new();
    let (finish, service_guard) = service
        .stage_guarded(&world.runtime, &world.store, &mut batch)
        .unwrap();
    let conflict_guard = allegiance.stage_into(&world.store, &mut batch).unwrap();
    service_guard.validate(&world.store, &batch).unwrap();
    conflict_guard.validate(&world.store, &batch).unwrap();
    assert_eq!(world.store.snapshot().unwrap(), before);
    world.store.apply_batch(batch).unwrap();
    finish.finish(&mut world.runtime);
    assert_eq!(
        mission(&world.store, ID).unwrap().members[&B].status,
        CompanionStatus::Defected
    );
    assert_eq!(
        game_conflict::encounter(&world.store, encounter)
            .unwrap()
            .fighters[&B]
            .team,
        1
    );
}

#[test]
fn production_encounter_reaction_reviews_accepted_confinement_actor_only_after_end() {
    let mut port = Execution::new();
    review_missions(&mut port).unwrap();
    port.controlled.clear();
    mission_mut(&mut port.world.store, ID)
        .unwrap()
        .members
        .get_mut(&A)
        .unwrap()
        .directive = MissionObjective::Confine {
        person: ENEMY,
        until: SimTime(100_000),
    };
    let mut schema = port.world.store.schema().clone();
    game_conflict::register_components(&mut schema).unwrap();
    let old = port.world.store.snapshot().unwrap();
    let mut expanded = game_ecs::Store::with_schema(schema.clone())
        .snapshot()
        .unwrap();
    expanded.entities = old.entities;
    expanded.columns.extend(old.columns);
    port.world.store = game_ecs::Store::restore(schema, expanded).unwrap();
    let encounter = EncounterId(500);
    game_conflict::insert_encounter(
        &mut port.world.store,
        game_conflict::Encounter {
            id: encounter,
            area: HOME,
            location: Location::AtArea(HOME),
            map: CombatMapId(501),
            round: 0,
            started: SimTime(0),
            ends: SimTime(6),
            fighters: BTreeMap::from([(
                A,
                game_conflict::Fighter {
                    actor: A,
                    joins_round: 0,
                    team: 0,
                    position: Point { x: 0, y: 0 },
                    surrendered: false,
                    withdrawn: false,
                    defending: false,
                    retreat: None,
                    known: BTreeMap::new(),
                    search: game_conflict::SearchState::default(),
                },
            )]),
            order: vec![A],
            cursor: 0,
            active: true,
        },
    )
    .unwrap();
    react_to_encounter_end(&mut port, encounter);
    assert!(port.controlled.is_empty());
    game_conflict::encounter_mut(&mut port.world.store, encounter)
        .unwrap()
        .active = false;
    react_to_encounter_end(&mut port, encounter);
    assert_eq!(port.controlled, vec![(ID, A)]);
}

struct HearingScene {
    scene: Scene,
    audible: BTreeSet<PersonId>,
}
impl PresenceFacts for HearingScene {
    fn location(&self, p: PersonId) -> Location {
        self.scene.location(p)
    }
    fn raw_location(&self, p: PersonId) -> Location {
        self.scene.raw_location(p)
    }
    fn area(&self, p: PersonId) -> Option<AreaId> {
        PresenceFacts::area(&self.scene, p)
    }
    fn same_place(&self, a: PersonId, b: PersonId) -> bool {
        a == b || self.audible.contains(&a) && self.audible.contains(&b)
    }
    fn in_combat(&self, p: PersonId) -> bool {
        PresenceFacts::in_combat(&self.scene, p)
    }
    fn combat_scene(
        &self,
        p: PersonId,
    ) -> Option<(&game_conflict::Encounter, &game_conflict::Map)> {
        self.scene.combat_scene(p)
    }
}
impl OrderHearingFacts for HearingScene {
    fn player(&self) -> PersonId {
        SP
    }
    fn recruitment(&self, person: PersonId, _: &Mission) -> RecruitmentInput {
        input(person)
    }
}
#[test]
fn production_combat_order_preparation_uses_the_supplied_projected_hearing_scene() {
    for hears in [false, true] {
        let mut world = basic();
        accept(&mut world, A, OFFER, &facts());
        accept(&mut world, B, RecruitmentId(13), &facts());
        let scene = HearingScene {
            scene: Scene {
                areas: BTreeMap::from([(SP, HOME), (A, HOME), (B, HOME)]),
                combat: true,
            },
            audible: if hears {
                BTreeSet::from([A, B])
            } else {
                BTreeSet::from([A])
            },
        };
        let mut command = facts();
        command.now = SimTime(10);
        let before = world.store.snapshot().unwrap();
        let objective = MissionObjective::Visit { destination: HOME };
        let plan = world
            .runtime
            .prepare_combat_order(&world.store, SP, ID, &objective, &command, &scene)
            .unwrap();
        assert_eq!(world.store.snapshot().unwrap(), before);
        plan.commit(&mut world.runtime, &mut world.store).unwrap();
        assert_eq!(
            mission(&world.store, ID).unwrap().members[&A].received_at,
            SimTime(10)
        );
        assert_eq!(
            mission(&world.store, ID).unwrap().members[&B].received_at,
            if hears { SimTime(10) } else { SimTime(0) }
        );
    }
}
