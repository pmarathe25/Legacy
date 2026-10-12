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
    validate_context(
        &w.runtime,
        &w.store,
        ValidationCatalog {
            now: SimTime(0),
            next_id,
            next_event: 1,
            people,
            areas: &BTreeSet::from([HOME, AWAY]),
            carrying_capacity: Quantity(5000),
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
