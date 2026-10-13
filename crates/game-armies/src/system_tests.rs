use crate::*;
use game_core::*;
use game_ecs::{Schema, Store};
use game_missions::{MissionObjective, MissionView};
use std::collections::BTreeMap;

struct Fixture {
    store: Store,
    runtime: World,
    now: SimTime,
    next: u64,
    locations: BTreeMap<PersonId, Location>,
    activities: BTreeMap<PersonId, ActorActivity>,
    judgment: i64,
}
impl Fixture {
    fn new() -> Self {
        let mut schema = Schema::new();
        register_components(&mut schema).unwrap();
        game_missions::register_components(&mut schema).unwrap();
        Self {
            store: Store::with_schema(schema),
            runtime: World {
                rules: Rules::default(),
                last_review: SimTime(0),
            },
            judgment: 0,
            now: SimTime(0),
            next: 10,
            locations: [PersonId(1), PersonId(2)]
                .into_iter()
                .map(|id| (id, Location::AtArea(AreaId(1))))
                .collect(),
            activities: [PersonId(1), PersonId(2)]
                .into_iter()
                .map(|id| (id, ActorActivity::Idle))
                .collect(),
        }
    }
}
impl Ports for Fixture {
    fn store(&self) -> &Store {
        &self.store
    }
    fn store_mut(&mut self) -> &mut Store {
        &mut self.store
    }
    fn army_runtime(&self) -> Option<&World> {
        Some(&self.runtime)
    }
    fn army_runtime_mut(&mut self) -> Option<&mut World> {
        Some(&mut self.runtime)
    }
    fn now(&self) -> SimTime {
        self.now
    }
    fn next_id(&self) -> u64 {
        self.next
    }
    fn allocate_id(&mut self) -> Result<u64, Error> {
        let id = self.next;
        self.next += 1;
        Ok(id)
    }
    fn ruler(&self) -> PersonId {
        PersonId(1)
    }
    fn people(&self) -> Vec<PersonId> {
        self.locations.keys().copied().collect()
    }
    fn areas(&self) -> Vec<AreaId> {
        vec![AreaId(1), AreaId(2)]
    }
    fn person_exists(&self, actor: PersonId) -> bool {
        self.locations.contains_key(&actor)
    }
    fn independent(&self, _: PersonId) -> bool {
        true
    }
    fn combat_restricts(&self, _: PersonId) -> bool {
        false
    }
    fn actor_body(&self, actor: PersonId) -> Option<game_conflict::TargetBody> {
        self.person_exists(actor)
            .then_some(game_conflict::TargetBody {
                alive: true,
                health: 100,
            })
    }
    fn resolved_location(&self, actor: PersonId) -> Location {
        self.locations[&actor].clone()
    }
    fn location(&self, actor: PersonId) -> Location {
        self.locations[&actor].clone()
    }
    fn fatigue(&self, _: PersonId) -> Option<(u64, u64)> {
        Some((0, 100))
    }
    fn encounter(&self, _: EncounterId) -> Option<&game_conflict::Encounter> {
        None
    }
    fn encounters(&self) -> Vec<&game_conflict::Encounter> {
        vec![]
    }
    fn combat_map(&self, _: CombatMapId) -> Option<&game_conflict::Map> {
        None
    }
    fn combat_round(&self) -> SimDuration {
        SimDuration(6_000)
    }
    fn known_routes(
        &self,
        _: PersonId,
    ) -> Option<&BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>> {
        None
    }
    fn missions_enabled(&self) -> Option<()> {
        Some(())
    }
    fn agreements_enabled(&self) -> Option<()> {
        Some(())
    }
    fn activity(&self, actor: PersonId) -> Option<ActorActivity> {
        self.activities.get(&actor).cloned()
    }
    fn activities(&self) -> Vec<(PersonId, ActorActivity)> {
        self.activities
            .iter()
            .map(|(id, a)| (*id, a.clone()))
            .collect()
    }
    fn set_activity(&mut self, actor: PersonId, activity: ActorActivity) {
        self.activities.insert(actor, activity);
    }
    fn conversation_actor_available(&self, actor: PersonId) -> bool {
        self.activity(actor) == Some(ActorActivity::Idle) && !self.army_actor_talking(actor)
    }
    fn interrupt_actor_activity(&mut self, actor: PersonId) {
        self.interrupt_army_talk(actor);
        self.activities.insert(actor, ActorActivity::Idle);
    }
    fn ensure_event_capacity(&self, _: usize) -> Result<(), Error> {
        Ok(())
    }
    fn mission_view(&self, _: PersonId) -> Vec<MissionView> {
        vec![]
    }
    fn mission_objective_valid(&self, _: &MissionObjective) -> bool {
        false
    }
    fn commit_army_change(&mut self, plan: PreparedChange) -> Result<(), Error> {
        if !plan.effects.is_empty() {
            return Err(Error::Unavailable);
        }
        plan.commit(&mut self.store).map(|_| ())
    }
    fn mission_leader_judgment(
        &self,
        _: PersonId,
        _: MissionId,
        _: &MissionObjective,
    ) -> Result<i64, Error> {
        Ok(self.judgment)
    }
    fn can_fight(&self, _: PersonId) -> bool {
        false
    }
    fn mission_force_willing(&self, _: PersonId, _: MissionId, _: PersonId) -> bool {
        false
    }
    fn admit_local_army_approach(&mut self, _: &Approach) -> Result<(), Error> {
        Err(Error::Unavailable)
    }
}
fn queued(listener: PersonId) -> QueuedMessage {
    QueuedMessage {
        listener,
        queued_at: SimTime(0),
        message: Message::Invitation {
            army: ArmyId(9),
            mission: MissionId(8),
        },
    }
}
#[test]
fn communication_uses_local_contact_and_interruption_drops_only_the_started_message() {
    let mut fixture = Fixture::new();
    insert_queue(
        &mut fixture.store,
        PersonId(1),
        Queue {
            messages: [queued(PersonId(2)), queued(PersonId(2))].into(),
        },
    )
    .unwrap();
    fixture
        .locations
        .insert(PersonId(2), Location::AtArea(AreaId(2)));
    fixture.start_queued_army_talk(PersonId(1)).unwrap();
    assert!(talk(&fixture.store, PersonId(1)).is_none());
    assert_eq!(
        queue(&fixture.store, PersonId(1)).unwrap().messages.len(),
        2
    );
    fixture
        .locations
        .insert(PersonId(2), Location::AtArea(AreaId(1)));
    fixture.start_queued_army_talk(PersonId(1)).unwrap();
    assert!(fixture.army_actor_talking(PersonId(2)));
    assert_eq!(fixture.army_boundary(SimTime(100_000)), SimTime(60_000));
    assert_eq!(
        queue(&fixture.store, PersonId(1)).unwrap().messages.len(),
        1
    );
    fixture.interrupt_army_talk(PersonId(2));
    assert!(talk(&fixture.store, PersonId(1)).is_none());
    assert_eq!(fixture.activity(PersonId(1)), Some(ActorActivity::Idle));
    assert_eq!(
        queue(&fixture.store, PersonId(1)).unwrap().messages.len(),
        1
    );
}
#[test]
fn staged_cancellation_rejects_changed_rows_and_foreign_replacement() {
    let mut fixture = Fixture::new();
    fixture.queue_army_message(PersonId(1), PersonId(2), queued(PersonId(2)).message);
    fixture.start_queued_army_talk(PersonId(1)).unwrap();
    let opening = talk(&fixture.store, PersonId(1)).unwrap().clone();
    let mut batch = game_ecs::Batch::new();
    let guard = stage_cancel_talk(&fixture.store, &opening, &mut batch).unwrap();
    guard.validate(&fixture.store, &batch).unwrap();
    batch.insert(game_ecs::StableId(1), opening.clone());
    assert_eq!(
        guard.validate(&fixture.store, &batch),
        Err(Error::Unavailable)
    );
    fixture
        .store
        .get_mut::<Talk>(game_ecs::StableId(1))
        .unwrap()
        .area = AreaId(2);
    assert!(stage_cancel_talk(&fixture.store, &opening, &mut game_ecs::Batch::new()).is_err());
}
#[test]
fn restored_queues_continue_with_the_same_owner_boundary() {
    let mut uninterrupted = Fixture::new();
    uninterrupted.queue_army_message(PersonId(1), PersonId(2), queued(PersonId(2)).message);
    let mut resumed = Fixture::new();
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    game_missions::register_components(&mut schema).unwrap();
    resumed.store = Store::restore(schema, uninterrupted.store.snapshot().unwrap()).unwrap();
    uninterrupted.start_queued_army_talk(PersonId(1)).unwrap();
    resumed.start_queued_army_talk(PersonId(1)).unwrap();
    assert_eq!(uninterrupted.store, resumed.store);
    assert_eq!(uninterrupted.activities, resumed.activities);
    assert_eq!(
        uninterrupted.army_boundary(SimTime(100_000)),
        resumed.army_boundary(SimTime(100_000))
    );
}

#[test]
fn autonomy_uses_the_named_leaders_judgment_and_retains_distant_knowledge() {
    for (judgment, expected) in [(-1, Participation::Refused), (0, Participation::Accepted)] {
        let mut fixture = Fixture::new();
        fixture.judgment = judgment;
        let mission = MissionId(8);
        let leader = PersonId(2);
        let commander = PersonId(1);
        let army = ArmyId(9);
        let objective = MissionObjective::Visit {
            destination: AreaId(1),
        };
        let member = game_missions::Companion {
            confinement: None,
            person: leader,
            recruitment: RecruitmentId(10),
            status: game_missions::CompanionStatus::Accepted,
            directive: objective.clone(),
            received_at: SimTime(0),
            visited: Default::default(),
            delivered: 0,
            cargo: Default::default(),
            supply_cargo: Default::default(),
            contacts: Default::default(),
            last_leg_from: None,
            explanation: "Agreed".into(),
            declared_explanation: "Agreed".into(),
        };
        game_missions::insert_mission(
            &mut fixture.store,
            mission,
            game_missions::Mission {
                id: mission,
                sponsor: commander,
                proposal: game_missions::MissionProposal {
                    leader,
                    recruits: Default::default(),
                    objective,
                    home: AreaId(1),
                    supplies: vec![],
                    compensation: Default::default(),
                    deadline: SimTime(300_000),
                },
                status: game_missions::MissionStatus::Preparing,
                members: BTreeMap::from([(leader, member)]),
                created_at: SimTime(0),
                departed_at: None,
                explanation: "Preparing".into(),
                succeeded: false,
            },
        )
        .unwrap();
        let facts = DetachmentFacts {
            mission,
            leader,
            members: [leader].into(),
            status: game_missions::MissionStatus::Preparing,
            leader_capable: true,
        };
        Kernel::new(&fixture.store)
            .prepare_form(
                army,
                commander,
                &[mission].into(),
                &BTreeMap::from([(mission, facts)]),
                SimTime(0),
            )
            .unwrap()
            .commit(&mut fixture.store)
            .unwrap();
        Kernel::new(&fixture.store)
            .prepare_hear_invitation(
                army,
                mission,
                Communication {
                    speaker: commander,
                    listener: leader,
                    started_at: SimTime(0),
                    completed_at: SimTime(1),
                },
                SimTime(1),
            )
            .unwrap()
            .commit(&mut fixture.store)
            .unwrap();
        fixture.now = SimTime(60_000);
        fixture.review_armies().unwrap();
        assert_eq!(
            crate::army(&fixture.store, army).unwrap().detachments[&mission].participation,
            expected
        );
        assert_eq!(
            Kernel::new(&fixture.store)
                .view(commander, army)
                .unwrap()
                .detachments[&mission]
                .value
                .participation,
            Participation::Offered
        );
        assert!(talk(&fixture.store, leader).is_some());
    }
}

#[test]
fn interruption_selects_listener_link_and_only_matching_talk_deadline() {
    let mut fixture = Fixture::new();
    fixture.queue_army_message(PersonId(1), PersonId(2), queued(PersonId(2)).message);
    fixture.start_queued_army_talk(PersonId(1)).unwrap();
    let linked = prepare_interruption_rows(&fixture.store, PersonId(2));
    assert_eq!(linked.talks.len(), 1);
    let row = &linked.talks[0];
    assert_eq!(
        ArmyInterruption::talk_reset(
            row,
            InterruptionActivity::Talk {
                completes: row.completes
            }
        ),
        Some(PersonId(1))
    );
    assert_eq!(
        ArmyInterruption::talk_reset(
            row,
            InterruptionActivity::Talk {
                completes: SimTime(row.completes.0 + 1)
            }
        ),
        None
    );
    assert_eq!(
        ArmyInterruption::talk_reset(row, InterruptionActivity::Approach),
        None
    );
    assert!(
        prepare_interruption_rows(&fixture.store, PersonId(99))
            .talks
            .is_empty()
    );
}

#[test]
fn interruption_approach_projection_uses_variant_without_deadline_matching() {
    let mut fixture = Fixture::new();
    let row = Approach {
        actor: PersonId(1),
        mission: MissionId(1),
        army: ArmyId(1),
        order: ArmyOrderId(1),
        encounter: EncounterId(1),
        area: AreaId(1),
        target: PersonId(2),
        started: SimTime(0),
        completes: SimTime(10),
    };
    insert_approach(&mut fixture.store, row.actor, row.clone()).unwrap();
    let selected = prepare_interruption_rows(&fixture.store, PersonId(1))
        .approach
        .unwrap();
    assert_eq!(selected, row);
    assert_eq!(
        ArmyInterruption::approach_reset(&selected, InterruptionActivity::Approach),
        Some(PersonId(1))
    );
    assert_eq!(
        ArmyInterruption::approach_reset(
            &selected,
            InterruptionActivity::Talk {
                completes: SimTime(10)
            }
        ),
        None
    );
    assert!(
        prepare_interruption_rows(&fixture.store, PersonId(2))
            .approach
            .is_none()
    );
}
