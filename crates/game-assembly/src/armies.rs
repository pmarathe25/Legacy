//! Assembly supplies factual ports and binds public Army commands.
use crate::*;
pub use game_armies::{Approach, Rules, Talk, World};
impl Simulation {
    /// Explicit army fixture: 65 citizens, one opponent, two guardians and the
    /// commander are distinct ordinary people before recruitment begins.
    pub fn army_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::kernel_fixture(seed, 69, execution)?;
        sim.host.state.scenario = ScenarioInfo {
            id: "army-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        sim.initialize_physical_people(physical::Rules {
            hunger_harm: 6_000_000,
            fatigue_limit: 18_000_000,
            max_health: 180_000,
            meal_relief: 3_000_000,
            meal_size: Quantity(1_000),
            eating_time: MINUTE,
            loading_time: MINUTE,
            carrying_capacity: Quantity(10_000),
        })?;
        let food = game_cargo::inventories(&sim.host.storage)
            .map(|(id, _)| id)
            .last()
            .unwrap();
        game_cargo::inventory_mut(&mut sim.host.storage, food)
            .unwrap()
            .quantity = Quantity(200_000);
        let routes = decisions::fixture_routes(&sim.host.state.areas);
        let minds = sim
            .people()
            .map(|p| {
                let mut personality = decisions::Personality::industrious();
                personality.ambition = 1_000;
                personality.sociability = 1_000;
                personality.risk_tolerance = 1_000;
                personality.empathy = 0;
                personality.aggression = 0;
                (
                    p.id,
                    decisions::Mind {
                        interests: BTreeMap::new(),
                        routes: routes.clone(),
                        last_failure: None,
                        child_signals: BTreeMap::new(),
                        personality,
                        dream: decisions::Aspiration::Independence,
                        sites: BTreeMap::new(),
                        relationships: BTreeMap::new(),
                        values: BTreeMap::new(),
                        goal: decisions::Goal::Rest,
                        plan: None,
                        explanation: decisions::Explanation {
                            goal: decisions::Goal::Rest,
                            need: 0,
                            benefit: 0,
                            value: 0,
                            total: 0,
                            blocker: None,
                        },
                        retry_after: SimTime(0),
                    },
                )
            })
            .collect();
        sim.initialize_decision_people(minds)?;
        for person_id in sim.people().map(|person| person.id).collect::<Vec<_>>() {
            game_characters::decisions::stocks_mut(&mut sim.host.storage, person_id)
                .expect("stock knowledge exists")
                .known
                .insert(
                    food,
                    StockKnowledge {
                        inventory: food,
                        area: sim.host.state.areas[0].id,
                        good: Good::Food,
                        quantity: Quantity(200_000),
                        accessible: true,
                        observed_at: SimTime(0),
                        learned_at: SimTime(0),
                        source: KnowledgeSource::Observed(person_id),
                    },
                );
        }

        sim.host.state.combat = Some(combat::World {
            rules: combat::Rules {
                encounter_limit: 128,
                round: SimDuration(6_000),
                movement: 5_000,
                reach: 2_000,
                damage: 45_000,
                defence: 45_000,
                incapacitation: 45_000,
            },
        });
        for person_id in sim.people().map(|person| person.id).collect::<Vec<_>>() {
            let person_role = sim.person(person_id).role;
            game_conflict::insert_profile(
                &mut sim.host.storage,
                person_id,
                combat::Profile {
                    initiative: if person_role == Role::Ruler { 200 } else { 100 },
                },
            )?;
        }
        let __custody: BTreeMap<InventoryId, PersonId> = game_cargo::inventories(&sim.host.storage)
            .map(|(_, row)| row)
            .map(|i| {
                (
                    i.id,
                    match i.location {
                        InventoryLocation::Carried(p) => p,
                        InventoryLocation::Area(_) => sim.host.state.ruler,
                    },
                )
            })
            .collect();
        for (id, owner) in __custody {
            game_property::declare_custody(&mut sim.host.storage, id, owner)?;
        }
        sim.host.state.agreements = Some(game_agreements::Runtime {
            last_event: sim.host.next_event() - 1,
            last_observation_event: sim.host.next_event() - 1,
        });
        sim.host.state.missions = Some(missions::Runtime {
            last_event: sim.host.next_event() - 1,
            last_review: sim.host.clock().time,
        });
        sim.initialize_armies(Rules::default())?;
        let village = sim.host.state.areas[0].id;
        sim.debug_add_inventory(
            Good::Binding,
            InventoryLocation::Area(village),
            Quantity(3_000),
            Quantity(3_000),
            sim.people().map(|p| p.id).collect(),
        )?;
        Ok(sim)
    }
    pub fn initialize_armies(&mut self, rules: Rules) -> Result<(), Error> {
        rules.validate()?;
        if self.host.state.armies.is_some()
            || self.host.state.missions.is_none()
            || self.host.state.physical.is_none()
            || self.host.state.decisions.is_none()
            || self.host.state.combat.is_none()
        {
            return Err(Error::Unavailable);
        }
        self.host.state.armies = Some(World {
            rules,
            last_review: self.host.clock().time,
        });
        Ok(())
    }
}

impl game_armies::Ports for Simulation {
    fn store(&self) -> &game_ecs::Store {
        &self.host.storage
    }
    fn store_mut(&mut self) -> &mut game_ecs::Store {
        &mut self.host.storage
    }
    fn army_runtime(&self) -> Option<&World> {
        self.host.state.armies.as_ref()
    }
    fn army_runtime_mut(&mut self) -> Option<&mut World> {
        self.host.state.armies.as_mut()
    }
    fn now(&self) -> SimTime {
        self.host.clock().time
    }
    fn next_id(&self) -> u64 {
        self.host.next_id()
    }
    fn allocate_id(&mut self) -> Result<u64, Error> {
        self.host.allocate_id().map(|id| id.0)
    }
    fn ruler(&self) -> PersonId {
        self.host.state.ruler
    }
    fn people(&self) -> Vec<PersonId> {
        self.people().map(|p| p.id).collect()
    }
    fn areas(&self) -> Vec<AreaId> {
        self.host.state.areas.iter().map(|p| p.id).collect()
    }
    fn person_exists(&self, actor: PersonId) -> bool {
        self.person_index.contains_key(&actor)
    }
    fn independent(&self, actor: PersonId) -> bool {
        Simulation::independent(self, actor)
    }
    fn combat_restricts(&self, actor: PersonId) -> bool {
        Simulation::combat_restricts(self, actor)
    }
    fn actor_body(&self, actor: PersonId) -> Option<game_conflict::TargetBody> {
        game_physiology::body(&self.host.storage, actor).map(|b| game_conflict::TargetBody {
            alive: b.vitals.alive,
            health: b.vitals.health,
        })
    }
    fn resolved_location(&self, actor: PersonId) -> Location {
        Simulation::resolved_location(self, actor)
    }
    fn location(&self, actor: PersonId) -> Location {
        game_spatial::location(&self.host.storage, game_ecs::StableId(actor.0))
            .expect("person location exists")
            .clone()
    }
    fn fatigue(&self, actor: PersonId) -> Option<(u64, u64)> {
        game_physiology::body(&self.host.storage, actor)
            .map(|b| (b.vitals.fatigue, self.body_rules(actor).fatigue_limit))
    }
    fn encounter(&self, id: EncounterId) -> Option<&game_conflict::Encounter> {
        self.host.state.combat.as_ref()?;
        game_conflict::encounter(&self.host.storage, id)
    }
    fn encounters(&self) -> Vec<&game_conflict::Encounter> {
        if self.host.state.combat.is_some() {
            game_conflict::encounters(&self.host.storage)
                .map(|(_, row)| row)
                .collect()
        } else {
            Vec::new()
        }
    }
    fn combat_map(&self, id: CombatMapId) -> Option<&game_conflict::Map> {
        self.host.state.combat.as_ref()?;
        game_conflict::map(&self.host.storage, id)
    }
    fn combat_round(&self) -> SimDuration {
        self.host
            .state
            .combat
            .as_ref()
            .expect("combat enabled")
            .rules
            .round
    }
    fn known_routes(
        &self,
        actor: PersonId,
    ) -> Option<&BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>> {
        Some(&game_characters::decisions::mind::<Command>(&self.host.storage, actor)?.routes)
    }
    fn missions_enabled(&self) -> Option<()> {
        self.host.state.missions.as_ref().map(|_| ())
    }
    fn agreements_enabled(&self) -> Option<()> {
        self.host.state.agreements.as_ref().map(|_| ())
    }
    fn activity(&self, actor: PersonId) -> Option<game_armies::ActorActivity> {
        self.host.storage.activity(actor).map(army_activity)
    }
    fn activities(&self) -> Vec<(PersonId, game_armies::ActorActivity)> {
        self.host
            .storage
            .activities()
            .map(|(id, a)| (id, army_activity(a)))
            .collect()
    }
    fn set_activity(&mut self, actor: PersonId, activity: game_armies::ActorActivity) {
        *self
            .host
            .storage
            .activity_mut(actor)
            .expect("person activity exists") = match activity {
            game_armies::ActorActivity::Idle => Activity::Idle,
            game_armies::ActorActivity::ArmyTalk {
                army,
                listener,
                completes,
            } => Activity::ArmyTalk {
                army,
                listener,
                completes,
            },
            game_armies::ActorActivity::ArmyApproach {
                mission,
                encounter,
                completes,
            } => Activity::ArmyApproach {
                mission,
                encounter,
                completes,
            },
            game_armies::ActorActivity::Other => {
                unreachable!("army effects never assign foreign activity")
            }
        };
    }
    fn conversation_actor_available(&self, actor: PersonId) -> bool {
        Simulation::conversation_actor_available(self, actor)
    }
    fn interrupt_actor_activity(&mut self, actor: PersonId) {
        Simulation::interrupt_actor_activity(self, actor)
    }
    fn ensure_event_capacity(&self, count: usize) -> Result<(), Error> {
        Simulation::ensure_event_capacity(self, count)
    }
    fn mission_view(&self, actor: PersonId) -> Vec<MissionView> {
        Simulation::mission_view(self, actor)
    }
    fn mission_objective_valid(&self, objective: &MissionObjective) -> bool {
        Simulation::mission_objective_valid(self, objective)
    }
    fn commit_army_change(&mut self, change: game_armies::PreparedChange) -> Result<(), Error> {
        let mut native = Vec::new();
        for request in change.mission_requests() {
            match request {
                game_armies::MissionRequest::Directive {
                    mission,
                    leader,
                    objective,
                } => native
                    .push(self.prepare_army_mission_directive(leader, mission, objective, false)?),
                game_armies::MissionRequest::Retreat { mission, leader } => {
                    native.push(self.prepare_army_mission_retreat(leader, mission)?)
                }
                game_armies::MissionRequest::Leave { mission, leader } => {
                    native.push(self.prepare_army_mission_leave(leader, mission)?)
                }
            }
        }
        let event_count = native.iter().try_fold(0usize, |count, plan| {
            count.checked_add(plan.events.len()).ok_or(Error::Overflow)
        })?;
        self.ensure_event_capacity(event_count)?;
        let mut batch = game_ecs::Batch::new();
        let (guard, _) = change.stage(&self.host.storage, &mut batch)?;
        let mut finishes = Vec::new();
        let mut guards = vec![guard];
        let mut publications = Vec::new();
        for plan in native {
            publications.push((plan.events.clone(), plan.notices.clone()));
            let (finish, guard) = plan.stage_guarded(
                self.host
                    .state
                    .missions
                    .as_ref()
                    .ok_or(Error::Unavailable)?,
                &self.host.storage,
                &mut batch,
            )?;
            finishes.push(finish);
            guards.push(guard);
        }
        for guard in guards {
            guard.validate(&self.host.storage, &batch)?;
        }
        self.host.storage.validate_batch(&batch)?;
        self.host.storage.apply_batch(batch)?;
        for finish in finishes {
            finish.finish(self.host.state.missions.as_mut().unwrap());
        }
        for (events, notices) in publications {
            self.publish_mission_effects(events, notices);
        }
        Ok(())
    }
    fn mission_leader_judgment(
        &self,
        actor: PersonId,
        mission: MissionId,
        objective: &MissionObjective,
    ) -> Result<i64, Error> {
        Simulation::mission_leader_judgment(self, actor, mission, objective)
    }
    fn can_fight(&self, actor: PersonId) -> bool {
        Simulation::can_fight(self, actor)
    }
    fn mission_force_willing(&self, actor: PersonId, mission: MissionId, target: PersonId) -> bool {
        Simulation::mission_force_willing(self, actor, mission, target)
    }
    fn admit_local_army_approach(&mut self, approach: &Approach) -> Result<(), Error> {
        Simulation::admit_local_army_approach(self, approach)
    }
}
fn army_activity(activity: &Activity) -> game_armies::ActorActivity {
    match *activity {
        Activity::Idle => game_armies::ActorActivity::Idle,
        Activity::ArmyTalk {
            army,
            listener,
            completes,
        } => game_armies::ActorActivity::ArmyTalk {
            army,
            listener,
            completes,
        },
        Activity::ArmyApproach {
            mission,
            encounter,
            completes,
        } => game_armies::ActorActivity::ArmyApproach {
            mission,
            encounter,
            completes,
        },
        _ => game_armies::ActorActivity::Other,
    }
}
impl Simulation {
    pub(crate) fn army_command(
        &mut self,
        actor: PersonId,
        action: &game_armies::ArmyAction,
    ) -> Result<(), Error> {
        game_armies::Systems::army_command(self, actor, action)
    }
    pub(crate) fn army_boundary(&self, limit: SimTime) -> SimTime {
        game_armies::Systems::army_boundary(self, limit)
    }
    pub(crate) fn army_actor_talking(&self, actor: PersonId) -> bool {
        game_armies::Systems::army_actor_talking(self, actor)
    }
    pub(crate) fn army_allied_missions(&self, mission: MissionId) -> BTreeSet<MissionId> {
        game_armies::Systems::army_allied_missions(self, mission)
    }
    pub(crate) fn start_army_approach(
        &mut self,
        actor: PersonId,
        mission: MissionId,
        encounter: EncounterId,
    ) -> Result<(), Error> {
        game_armies::Systems::start_army_approach(self, actor, mission, encounter)
    }
    pub(crate) fn validate_army_approach(
        &self,
        approach: &Approach,
        completed: bool,
    ) -> Result<(u32, Point), Error> {
        game_armies::Systems::validate_army_approach(self, approach, completed)
    }
    pub(crate) fn reconcile_armies(&mut self) -> Result<(), Error> {
        game_armies::Systems::reconcile_armies(self)
    }
    pub(crate) fn review_armies(&mut self) -> Result<(), Error> {
        game_armies::Systems::review_armies(self)
    }
    pub(crate) fn army_view(&self, actor: PersonId) -> Vec<game_armies::ArmyView> {
        game_armies::Systems::army_view(self, actor)
    }
    pub(crate) fn available_army_commands(&self, actor: PersonId) -> Vec<Command> {
        game_armies::Systems::available_army_commands(self, actor)
            .into_iter()
            .map(|action| Command::Army { action })
            .collect()
    }
    pub(crate) fn army_allocation_bound(&self) -> u64 {
        game_armies::Systems::army_allocation_bound(self)
    }
    pub(crate) fn army_event_bound(&self) -> usize {
        game_armies::Systems::army_event_bound(self)
    }
    pub(crate) fn preflight_armies_at(&self, at: SimTime) -> Result<(), Error> {
        game_armies::Systems::preflight_armies_at(self, at)
    }
    pub(crate) fn validate_armies(&self) -> Result<(), Error> {
        game_armies::Systems::validate_armies(self)
    }
}
