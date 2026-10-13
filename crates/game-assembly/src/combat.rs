//! Initiative turns refine existing people and areas. One shared round deadline
//! advances the world once, including when several encounters run concurrently.
use crate::*;
mod opening_preview;

pub use game_conflict::{Captivity, Encounter, Fighter, Map, Profile, Rules, World};

use game_conflict::EncounterOpening;

pub(crate) use game_conflict::PreparedMap as PreparedCombatMap;

impl Simulation {
    pub fn combat_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::life_fixture(seed, execution)?;
        sim.host.state.scenario = scenarios().remove(4);
        // These rates and starting bindings belong to this explicit fixture.
        let rules = Rules {
            encounter_limit: 128,
            round: SimDuration(6_000),
            movement: 5_000,
            reach: 2_000,
            damage: 45_000,
            defence: 30_000,
            incapacitation: 45_000,
        };
        let profiles: Vec<_> = sim
            .people()
            .map(|person| {
                (
                    person.id,
                    Profile {
                        initiative: if person.role == Role::Ruler { 200 } else { 100 },
                    },
                )
            })
            .collect();
        for (actor, profile) in profiles {
            game_conflict::insert_profile(&mut sim.host.storage, actor, profile)?;
        }
        sim.host.state.combat = Some(World { rules });
        for actor in [sim.host.state.ruler, sim.person_at(3).id] {
            sim.debug_add_inventory(
                Good::Binding,
                InventoryLocation::Carried(actor),
                Quantity(1_000),
                Quantity(1_000),
                BTreeSet::from([actor]),
            )?;
        }
        let target = sim.person_at(3).id;
        let mut personality = decisions::Personality::industrious();
        personality.aggression = 900;
        personality.risk_tolerance = 900;
        let dream =
            game_characters::decisions::mind::<game_api::Command>(&sim.host.storage, target)
                .unwrap()
                .dream;
        sim.debug_set_personality(target, personality, dream)?;
        Ok(sim)
    }
    pub(crate) fn in_combat(&self, actor: PersonId) -> bool {
        self.host.state.physical.is_some()
            && matches!(
                self.host.storage.activity(actor),
                Some(Activity::Combat { .. })
            )
    }
    pub(crate) fn combat_restricts(&self, actor: PersonId) -> bool {
        let active = self.in_combat(actor);
        self.host.state.combat.as_ref().map_or(active, |world| {
            world.rules.restricts(
                active,
                game_conflict::captivity(&self.host.storage, actor).is_some(),
                self.host
                    .state
                    .physical
                    .as_ref()
                    .and_then(|_| game_physiology::body(&self.host.storage, actor))
                    .map(|body| game_conflict::TargetBody {
                        alive: body.vitals.alive,
                        health: body.vitals.health,
                    }),
            )
        })
    }
    pub(crate) fn can_fight(&self, actor: PersonId) -> bool {
        let w = self.host.state.combat.as_ref().unwrap();
        let v = &game_physiology::body(&self.host.storage, actor)
            .expect("body exists")
            .vitals;
        w.rules.can_fight(
            game_conflict::TargetBody {
                alive: v.alive,
                health: v.health,
            },
            game_conflict::captivity(&self.host.storage, actor).is_some(),
        )
    }
    fn can_combat_act(&self, actor: PersonId) -> bool {
        let world = self.host.state.combat.as_ref().unwrap();
        let body = game_physiology::body(&self.host.storage, actor).expect("body exists");
        world.rules.can_act(
            game_conflict::TargetBody {
                alive: body.vitals.alive,
                health: body.vitals.health,
            },
            game_conflict::captivity(&self.host.storage, actor).is_some(),
            self.independent(actor),
            self.force_can_combat_act(actor),
        )
    }
    pub(crate) fn admit_mission_reinforcement(
        &mut self,
        id: EncounterId,
        actor: PersonId,
        team: u32,
        from: AreaId,
    ) -> Result<(), Error> {
        let world = self.host.state.combat.as_ref().ok_or(Error::Unavailable)?;
        let encounter =
            game_conflict::encounter(&self.host.storage, id).ok_or(Error::Unavailable)?;
        let body = game_physiology::body(&self.host.storage, actor).ok_or(Error::Unavailable)?;
        let area = encounter.area;
        let arrived=self.host.events().iter().rev().take_while(|event|event.at==self.host.clock().time).any(|event|matches!(event.event,EventKind::Arrived{actor:person,area:destination} if person==actor&&destination==area));
        let departed=self.host.events().iter().rev().any(|event|matches!(event.event,EventKind::Departed{actor:person,from:origin,to,arrives} if person==actor&&origin==from&&to==area&&arrives==self.host.clock().time));
        let facts = game_conflict::AdmissionFacts {
            actor,
            team,
            from,
            body: game_conflict::TargetBody {
                alive: body.vitals.alive,
                health: body.vitals.health,
            },
            in_combat: self.in_combat(actor),
            same_place: self.transport_position(actor).map(|p| p.key())
                == transport::location_position(&encounter.location, self.host.clock().time)
                    .ok()
                    .map(|p| p.key()),
            arrived,
            departed,
        };
        let (position, joins_round) = world.mission_admission(&self.host.storage, id, facts)?;
        self.admit_combat_fighter(id, actor, team, position, joins_round)
    }
    pub(crate) fn admit_local_army_approach(
        &mut self,
        approach: &armies::Approach,
    ) -> Result<(), Error> {
        let (team, position) = self.validate_army_approach(approach, true)?;
        let joins_round = self
            .host
            .state
            .combat
            .as_ref()
            .unwrap()
            .admission_round(&self.host.storage, approach.encounter)?;
        self.admit_combat_fighter(
            approach.encounter,
            approach.actor,
            team,
            position,
            joins_round,
        )
    }
    fn admit_combat_fighter(
        &mut self,
        id: EncounterId,
        actor: PersonId,
        team: u32,
        position: Point,
        joins_round: u64,
    ) -> Result<(), Error> {
        let opening = self.host.transaction_opening();
        let world = self.host.state.combat.as_ref().ok_or(Error::Unavailable)?;
        let prepared =
            world.prepare_admission(&self.host.storage, id, actor, team, position, joins_round)?;
        let interruptions =
            self.prepare_interruptions(&[actor], commands::ActivityOwner::Combat)?;
        self.ensure_event_capacity(
            usize::try_from(interruptions.event_count()?)
                .map_err(|_| Error::Overflow)?
                .checked_add(1)
                .ok_or(Error::Overflow)?,
        )?;
        let final_activities = BTreeMap::from([(actor, Activity::Combat { encounter: id })]);
        let mut batch = game_ecs::Batch::new();
        let staged = prepared.stage_into(world, &self.host.storage, &mut batch)?;
        let mut finish = interruptions.stage_into(self, &mut batch, &final_activities)?;
        finish.validate_batch(self, &batch)?;
        let guard = staged.into_guard(world, &self.host.storage, &batch)?;
        let mut transaction = game_world::Transaction::new(opening, batch);
        transaction.guards.push(guard);
        self.host.commit_transaction(transaction)?;
        finish.finish_actor(self, actor);
        self.emit(EventKind::CombatReinforcement {
            encounter: id,
            actor,
            joins_round,
        });
        Ok(())
    }

    pub(crate) fn admit_due_mission_reinforcements(&mut self) {
        for (encounter, actor, team, from) in self.mission_reinforcements() {
            let _ = self.admit_mission_reinforcement(encounter, actor, team, from);
        }
    }

    pub(crate) fn awaiting_combat_input(&self) -> Option<EncounterId> {
        self.host.state.combat.as_ref()?.awaiting_input(
            &self.host.storage,
            self.host.state.ruler,
            self.past_life_controls_actor(self.host.state.ruler),
        )
    }
    pub(crate) fn tactical_view(&self) -> Option<TacticalView> {
        let w = self.host.state.combat.as_ref()?;
        let candidate = game_conflict::actor_encounter(&self.host.storage, self.host.state.ruler)?;
        let alive = game_physiology::body(&self.host.storage, self.host.state.ruler)?
            .vitals
            .alive;
        let same_place = self
            .transport_position(self.host.state.ruler)
            .map(|p| p.key())
            == transport::location_position(&candidate.location, self.host.clock().time)
                .ok()
                .map(|p| p.key());
        let id =
            w.current_encounter(&self.host.storage, self.host.state.ruler, alive, same_place)?;
        let e = game_conflict::encounter(&self.host.storage, id)?;
        let map = game_conflict::map(&self.host.storage, e.map)?;
        let place = transport::location_position(&e.location, self.host.clock().time)
            .ok()?
            .key();
        let present = e
            .fighters
            .keys()
            .copied()
            .filter(|actor| {
                self.transport_position(*actor)
                    .map(|position| position.key())
                    == Some(place)
            })
            .collect();
        let visible = w
            .visible_fighters(&self.host.storage, id, self.host.state.ruler, &present)
            .ok()?;
        Some(TacticalView {
            encounter: e.id,
            active: e.active,
            map: e.map,
            area: e.area,
            location: e.location.clone(),
            round: e.round,
            round_started: e.started,
            round_ends: e.ends,
            turn: e.order.get(e.cursor).copied(),
            ready_to_advance: e.cursor == e.order.len(),
            width: map.width,
            height: map.height,
            obstacles: map.obstacles.clone(),
            exits: map.exits.clone(),
            fighters: visible
                .into_iter()
                .map(|f| FighterView {
                    actor: f.actor,
                    position: f.position,
                    team: f.team,
                    incapacitated: !self.can_combat_act(f.actor),
                    surrendered: f.surrendered,
                    captured: game_conflict::captivity(&self.host.storage, f.actor).is_some(),
                })
                .collect(),
        })
    }
    pub(crate) fn submit_combat(
        &mut self,
        submission: Submission,
    ) -> Result<CommandReceipt, Error> {
        self.host.preflight_records(0, 0, 1)?;
        let next = self
            .host
            .state
            .next_command
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        self.preflight_agreements()?;
        let actor = self.host.state.ruler;
        let (encounter, selected) = match &submission.command {
            Command::Engage { target } => (
                None,
                BTreeSet::from([game_ecs::StableId(actor.0), game_ecs::StableId(target.0)]),
            ),
            Command::CombatTurn { encounter, .. } => (Some(*encounter), BTreeSet::new()),
            _ => return Err(Error::Unavailable),
        };
        let mut context =
            self.prepare_physical_maintenance_context(&game_ecs::Batch::new(), &[])?;
        context.include_enforcement_observation();
        let prepared = self.prepare_source_followups_with_context(
            encounter,
            &game_ecs::Batch::new(),
            0,
            0,
            selected,
            &context,
            |preview| match &submission.command {
                Command::Engage { target } => preview
                    .debug_start_encounter(vec![(actor, 0), (*target, 1)])
                    .map(|_| ()),
                Command::CombatTurn {
                    encounter,
                    movement,
                    action,
                } => preview.execute_turn(*encounter, actor, *movement, action.clone()),
                _ => unreachable!("combat command was checked before preparation"),
            },
            |preview| {
                preview.update_property_events()?;
                preview.reconcile_agreements()?;
                context.observe_enforcement(preview)?;
                preview.host.bump_revision(1)
            },
        )?;
        let (transaction, finish) = prepared.into_transaction(self)?;
        self.host.commit_transaction(transaction)?;
        finish.finish(self);
        let receipt = CommandReceipt {
            caller_sequence: submission.caller_sequence,
            authoritative_sequence: self.host.state.next_command,
            accepted_at: self.host.clock().time,
        };
        self.host.state.next_command = next;
        self.host.state.accepted.insert(
            submission.caller_sequence,
            (submission.command, receipt.clone()),
        );
        Ok(receipt)
    }
    pub(crate) fn preflight_start_encounter(
        &self,
        participants: &[(PersonId, u32)],
    ) -> Result<(), Error> {
        self.prepare_encounter_opening(participants).map(|_| ())
    }

    pub(crate) fn prepare_encounter_opening(
        &self,
        participants: &[(PersonId, u32)],
    ) -> Result<EncounterOpening, Error> {
        self.prepare_encounter_opening_projection(participants, None)
    }
    pub(crate) fn prepare_encounter_opening_after_motion(
        &self,
        participants: &[(PersonId, u32)],
        motion: &game_transport::PreparedMotion,
    ) -> Result<EncounterOpening, Error> {
        self.prepare_encounter_opening_projection(participants, Some(motion))
    }
    fn prepare_encounter_opening_projection(
        &self,
        participants: &[(PersonId, u32)],
        motion: Option<&game_transport::PreparedMotion>,
    ) -> Result<EncounterOpening, Error> {
        let world = self.host.state.combat.as_ref().ok_or(Error::Unavailable)?;
        let facts = participants
            .iter()
            .map(|(actor, team)| {
                self.person_index.get(actor).ok_or(Error::Unavailable)?;
                let body =
                    game_physiology::body(&self.host.storage, *actor).ok_or(Error::Unavailable)?;
                Ok(game_conflict::ParticipantFacts {
                    actor: *actor,
                    team: *team,
                    body: game_conflict::TargetBody {
                        alive: body.vitals.alive,
                        health: body.vitals.health,
                    },
                    independent: self.independent(*actor),
                    force_can_act: self.force_can_combat_act(*actor),
                    force_passive: self.force_passive_participant(*actor),
                    in_combat: self.in_combat(*actor),
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let opening = if let Some(motion) = motion {
            world.prepare_opening_after_motion(
                &self.host.storage,
                &facts,
                self.host.clock().time,
                motion,
            )?
        } else {
            world.prepare_opening(&self.host.storage, &facts, self.host.clock().time)?
        };
        self.host.next_id().checked_add(2).ok_or(Error::Overflow)?;
        self.ensure_event_capacity(opening.event_bound())?;
        Ok(opening)
    }

    pub fn debug_start_encounter(
        &mut self,
        participants: Vec<(PersonId, u32)>,
    ) -> Result<EncounterId, Error> {
        let owner_now = self.host.clock().time;
        let opening = self.prepare_encounter_opening(&participants)?;
        let area = opening.area();
        let map_plan = self.prepare_combat_location_map(opening.location())?;
        let map_count = u64::from(map_plan.is_new());
        let id = EncounterId(
            self.host
                .next_id()
                .checked_add(map_count)
                .ok_or(Error::Overflow)?,
        );
        let fatigue: Vec<_> = participants
            .iter()
            .map(|(actor, _)| {
                (
                    *actor,
                    game_physiology::body(&self.host.storage, *actor)
                        .expect("body exists")
                        .vitals
                        .fatigue,
                )
            })
            .collect();
        let world = self.host.state.combat.as_ref().ok_or(Error::Unavailable)?;
        let start = world.prepare_start(
            &self.host.storage,
            id,
            opening,
            map_plan.map(),
            &participants,
            &fatigue,
            owner_now,
        )?;
        let actors: Vec<_> = participants.iter().map(|(actor, _)| *actor).collect();
        let interruptions = self.prepare_interruptions(&actors, commands::ActivityOwner::Combat)?;
        self.ensure_event_capacity(
            usize::try_from(interruptions.event_count()?)
                .map_err(|_| Error::Overflow)?
                .checked_add(1)
                .ok_or(Error::Overflow)?,
        )?;
        let final_activities = actors
            .iter()
            .map(|actor| (*actor, Activity::Combat { encounter: id }))
            .collect();
        let mut batch = game_ecs::Batch::new();
        let map_guard = map_plan.stage_into(&self.host.storage, &mut batch)?;
        let start_guard = start.stage_into(world, &self.host.storage, &mut batch)?;
        let mut finish = interruptions.stage_into(self, &mut batch, &final_activities)?;
        finish.validate_batch(self, &batch)?;
        map_guard.validate(&self.host.storage, &batch)?;
        start_guard.validate(&self.host.storage, &batch)?;
        let mut events = finish.events_for(&actors);
        events.push(EventKind::EncounterStarted {
            encounter: id,
            area,
        });
        let followups = self.prepare_combat_opening_followups(
            id,
            &batch,
            map_count.checked_add(1).ok_or(Error::Overflow)?,
            &events,
            1,
        )?;
        let (transaction, followups) = followups.into_transaction(self)?;
        self.host.commit_transaction(transaction)?;
        for actor in actors {
            finish.finish_actor_metadata(self, actor);
        }
        followups.finish(self);
        Ok(id)
    }
    pub(crate) fn prepare_combat_location_map(
        &self,
        location: &Location,
    ) -> Result<PreparedCombatMap, Error> {
        self.prepare_combat_location_map_at(location, self.host.next_id())
    }
    pub(crate) fn prepare_combat_location_map_at(
        &self,
        location: &Location,
        first_id: u64,
    ) -> Result<PreparedCombatMap, Error> {
        if first_id < self.host.next_id() {
            return Err(Error::Unavailable);
        }
        let world = self.host.state.combat.as_ref().ok_or(Error::Unavailable)?;
        let position = transport::location_position(location, self.host.clock().time)?.key();
        let (area, areas_exist) = match position {
            game_transport::PositionKey::Area(area) => (
                area,
                self.host.state.areas.iter().any(|node| node.id == area),
            ),
            game_transport::PositionKey::Road { from, to, .. } => (
                from,
                self.host.state.areas.iter().any(|node| node.id == from)
                    && self.host.state.areas.iter().any(|node| node.id == to),
            ),
        };
        let links: Vec<_> = self
            .host
            .state
            .areas
            .iter()
            .find(|node| node.id == area)
            .map(|node| node.links.iter().map(|link| link.destination).collect())
            .unwrap_or_default();
        let plan = world.prepare_map(
            &self.host.storage,
            location,
            self.host.clock().time,
            self.host.seed(),
            CombatMapId(first_id),
            &links,
            areas_exist,
        )?;
        if plan.is_new() {
            first_id.checked_add(1).ok_or(Error::Overflow)?;
        }
        Ok(plan)
    }
    #[cfg(test)]
    pub(crate) fn install_combat_location_map(&mut self, plan: PreparedCombatMap) -> CombatMapId {
        if plan.is_new() {
            let id = self.host.allocate_id().expect("reserved scene ID");
            debug_assert_eq!(id.0, plan.map().id.0);
        }
        plan.commit(&mut self.host.storage)
            .expect("prepared combat map")
    }
    #[cfg(test)]
    pub(crate) fn ensure_combat_location_map(
        &mut self,
        location: &Location,
    ) -> Result<CombatMapId, Error> {
        let plan = self.prepare_combat_location_map(location)?;
        Ok(self.install_combat_location_map(plan))
    }
    fn begin_combat_round(&mut self, id: EncounterId) -> Result<(), Error> {
        let world = self.host.state.combat.as_ref().unwrap();
        let encounter =
            game_conflict::encounter(&self.host.storage, id).ok_or(Error::Unavailable)?;
        let fatigue: Vec<_> = encounter
            .fighters
            .keys()
            .map(|actor| {
                (
                    *actor,
                    game_physiology::body(&self.host.storage, *actor)
                        .expect("body exists")
                        .vitals
                        .fatigue,
                )
            })
            .collect();
        world
            .prepare_begin_round(&self.host.storage, id, &fatigue)?
            .commit(world, &mut self.host.storage)?;
        self.drive_combat_npcs(id)
    }
    fn observe_fighter(&mut self, id: EncounterId, actor: PersonId) -> Result<(), Error> {
        let alive = game_conflict::encounter(&self.host.storage, id)
            .ok_or(Error::Unavailable)?
            .fighters
            .keys()
            .copied()
            .filter(|actor| {
                game_physiology::body(&self.host.storage, *actor)
                    .expect("body exists")
                    .vitals
                    .alive
            })
            .collect();
        let world = self.host.state.combat.as_ref().unwrap();
        world
            .prepare_observation(&self.host.storage, id, actor, &alive)?
            .commit(world, &mut self.host.storage)?;
        Ok(())
    }
    pub(crate) fn binding_stock(&self, actor: PersonId) -> Option<InventoryId> {
        game_cargo::binding_stock(
            actor,
            self.inventory_index
                .get(&InventoryLocation::Carried(actor))
                .into_iter()
                .flatten()
                .filter_map(|id| game_cargo::inventory(&self.host.storage, *id)),
        )
    }
    fn execute_turn(
        &mut self,
        id: EncounterId,
        actor: PersonId,
        movement: Option<Point>,
        action: CombatAction,
    ) -> Result<(), Error> {
        self.execute_turn_with_search(id, actor, movement, action, None)
    }
    fn execute_turn_with_search(
        &mut self,
        id: EncounterId,
        actor: PersonId,
        movement: Option<Point>,
        action: CombatAction,
        search: Option<game_conflict::SearchState>,
    ) -> Result<(), Error> {
        let transaction_opening = self.host.transaction_opening();
        self.host.preflight_records(0, 0, 1)?;
        let world = self.host.state.combat.as_ref().ok_or(Error::Unavailable)?;
        let encounter =
            game_conflict::encounter(&self.host.storage, id).ok_or(Error::Unavailable)?;
        let body = game_physiology::body(&self.host.storage, actor).ok_or(Error::Unavailable)?;
        let (owner_action, target) = match &action {
            CombatAction::Wait => (game_conflict::Action::Wait, None),
            CombatAction::Defend => (game_conflict::Action::Defend, None),
            CombatAction::Surrender => (game_conflict::Action::Surrender, None),
            CombatAction::Attack { target } => {
                (game_conflict::Action::Attack(*target), Some(*target))
            }
            CombatAction::Capture { target } => {
                (game_conflict::Action::Capture(*target), Some(*target))
            }
            CombatAction::Release { target } => {
                (game_conflict::Action::Release(*target), Some(*target))
            }
            CombatAction::Retreat { destination } => {
                (game_conflict::Action::Retreat(*destination), None)
            }
            CombatAction::Breach { obstacle } => (game_conflict::Action::Breach(*obstacle), None),
            CombatAction::MissionObjective { mission, objective } => {
                self.validate_mission_combat_order(actor, *mission, objective)?;
                let leader = game_missions::mission(&self.host.storage, *mission)
                    .ok_or(Error::Unavailable)?
                    .proposal
                    .leader;
                (game_conflict::Action::MissionOrder { leader }, None)
            }
        };
        let retreat_route_valid = if let CombatAction::Retreat { destination } = action {
            match encounter.location {
                Location::StoppedOnRoad {
                    from,
                    to,
                    progress,
                    duration,
                } => {
                    self.road_retreat_plan(
                        game_transport::RoadPosition {
                            from,
                            to,
                            progress,
                            duration,
                        },
                        destination,
                        self.host.clock().time,
                    )?;
                }
                _ => {
                    self.path(encounter.area, destination)?;
                    self.departure(encounter.area, destination)?;
                }
            }
            true
        } else {
            false
        };
        let mut factual_reads = game_ecs::ReadSet::new();
        let lives = factual_reads
            .read::<game_invocation::ControlSession>(
                &self.host.storage,
                game_ecs::StableId(actor.0),
            )
            .and_then(|session| session.active.as_ref())
            .map(|active| active.lives.clone())
            .unwrap_or_default();
        factual_reads
            .read::<game_invocation::Burden>(&self.host.storage, game_ecs::StableId(actor.0));
        factual_reads.read::<game_physiology::traits::Development>(
            &self.host.storage,
            game_ecs::StableId(actor.0),
        );
        factual_reads
            .read::<game_genetics::PersonGenome>(&self.host.storage, game_ecs::StableId(actor.0));
        for life in lives {
            factual_reads.read::<game_invocation::PastLife<game_api::Command>>(
                &self.host.storage,
                game_ecs::StableId(life.0),
            );
        }
        let damage = self.invocation_combat_damage(
            actor,
            self.effective_combat_damage(actor, world.rules.damage)?,
        )?;
        let facts = game_conflict::TurnFacts {
            actor,
            independent: self.independent(actor),
            body: game_conflict::TargetBody {
                alive: body.vitals.alive,
                health: body.vitals.health,
            },
            force_can_act: self.force_can_combat_act(actor),
            force_allowed: self.force_action_allowed(actor, movement, &action),
            target_body: target
                .and_then(|target| game_physiology::body(&self.host.storage, target))
                .map(|body| game_conflict::TargetBody {
                    alive: body.vitals.alive,
                    health: body.vitals.health,
                }),
            binding_available: self.binding_stock(actor).is_some(),
            release_available: target.is_some_and(|target| self.can_release(actor, target)),
            mission_order_valid: true,
            retreat_route_valid,
            damage,
            practice_ms: self
                .host
                .state
                .skills
                .as_ref()
                .map_or(0, |runtime| runtime.rules.action_practice_ms),
        };
        let mut prepared = world
            .prepare_turn(&self.host.storage, id, movement, owner_action, facts)?
            .with_factual_reads(factual_reads);
        if let Some(search) = search {
            prepared = prepared.with_search(&self.host.storage, actor, search)?;
        }
        self.ensure_event_capacity(8)?;
        let mission_order = if let CombatAction::MissionObjective {
            mission,
            ref objective,
        } = action
        {
            Some(self.prepare_mission_combat_order(
                actor,
                mission,
                objective,
                prepared.projected_encounter(),
            )?)
        } else {
            None
        };
        let mission_events = mission_order
            .as_ref()
            .map(|plan| plan.events.clone())
            .unwrap_or_default();
        let mission_notices = mission_order
            .as_ref()
            .map(|plan| plan.notices.clone())
            .unwrap_or_default();
        self.ensure_event_capacity(mission_events.len().checked_add(8).ok_or(Error::Overflow)?)?;
        let capture = prepared.effects().iter().find_map(|effect| {
            if let game_conflict::Effect::Capture { target, context } = effect {
                Some((*target, context.clone()))
            } else {
                None
            }
        });
        let mut confinement = if let Some((target, context)) = capture {
            self.host.next_id().checked_add(1).ok_or(Error::Overflow)?;
            let source = self.binding_stock(actor).ok_or(Error::Unavailable)?;
            let facts = self.confinement_facts(
                actor,
                target,
                encounter.fighters[&target].surrendered,
                true,
                None,
            )?;
            let plan = game_conflict::prepare_confinement(
                &world.rules,
                facts.clone(),
                game_cargo::inventory(&self.host.storage, source).ok_or(Error::Unavailable)?,
                InventoryId(self.host.next_id()),
                context,
                game_conflict::ConfinementMode::Tactical { encounter: id },
                self.host.clock().time,
            )?;
            let plan = plan.begin(&self.host.storage, &facts)?;
            self.preflight_confinement_receipt(plan.receipt(), None)?;
            Some(plan)
        } else {
            None
        };
        let effects = prepared.effects().to_vec();
        let surrender = effects
            .iter()
            .find_map(|effect| match effect {
                game_conflict::Effect::Surrendered { actor } => Some(*actor),
                _ => None,
            })
            .map(|actor| {
                let scene = prepared.projected_encounter();
                self.host
                    .state
                    .force
                    .clone()
                    .unwrap_or_else(|| {
                        game_conflict::force::Runtime::new(self.host.next_event() - 1)
                    })
                    .prepare_surrender(
                        &self.host.storage,
                        actor,
                        scene.area,
                        self.host.clock().time,
                        scene.fighters[&actor].team,
                        scene
                            .fighters
                            .values()
                            .map(|fighter| (fighter.actor, fighter.team, fighter.withdrawn)),
                    )
            });
        let release = effects
            .iter()
            .find_map(|effect| match effect {
                game_conflict::Effect::Release { target } => Some(*target),
                _ => None,
            })
            .map(|target| self.prepare_captive_release(actor, target))
            .transpose()?;
        if let Some(plan) = &release {
            self.ensure_event_capacity(
                usize::try_from(plan.event_count()?)
                    .map_err(|_| Error::Overflow)?
                    .checked_add(8)
                    .ok_or(Error::Overflow)?,
            )?;
        }
        let strike = effects.iter().find_map(|effect| {
            if let game_conflict::Effect::Struck {
                target,
                damage,
                practice,
                ..
            } = effect
            {
                Some((*target, *damage, practice.clone()))
            } else {
                None
            }
        });
        let injury = strike
            .as_ref()
            .map(|(target, damage, _)| {
                game_physiology::prepare_injury(
                    &self.host.storage,
                    *target,
                    *damage,
                    self.person(*target).hunger_exempt,
                )
            })
            .transpose()?;
        let death = if injury.as_ref().is_some_and(|plan| plan.lethal()) {
            Some(self.prepare_death_cleanup(strike.as_ref().unwrap().0)?)
        } else {
            None
        };
        let practice = match (self.host.state.skills.as_ref(), strike.as_ref()) {
            (Some(runtime), Some((_, _, practice))) => {
                Some(runtime.prepare_practice(&self.host.storage, [practice.clone()])?)
            }
            _ => None,
        };
        let mut batch = game_ecs::Batch::new();
        let surrender = surrender
            .map(|plan| plan.stage_into(&self.host.storage, &mut batch))
            .transpose()?;
        let injury_guard = injury
            .map(|prepared| prepared.stage_into(&self.host.storage, &mut batch))
            .transpose()?;
        let mut death = death
            .map(|plan| plan.stage_into(self, &mut batch))
            .transpose()?;
        if let Some(death) = &death {
            self.ensure_event_capacity(
                death
                    .events()
                    .len()
                    .checked_add(mission_events.len())
                    .and_then(|n| n.checked_add(8))
                    .ok_or(Error::Overflow)?,
            )?;
        }
        let practice_guard = practice
            .map(|prepared| {
                prepared.stage_into(
                    self.host.state.skills.as_ref().unwrap(),
                    &self.host.storage,
                    &mut batch,
                )
            })
            .transpose()?;
        let confinement_guard = confinement
            .take()
            .map(|prepared| prepared.stage_into(&self.host.storage, &mut batch))
            .transpose()?;
        let release_finish = release
            .map(|plan| plan.stage_into(self, &mut batch))
            .transpose()?;
        let mission_order = mission_order
            .map(|prepared| {
                prepared.stage_guarded(
                    self.host.state.missions.as_ref().unwrap(),
                    &self.host.storage,
                    &mut batch,
                )
            })
            .transpose()?;
        let mut direct_events = Vec::new();
        for effect in &effects {
            match effect {
                game_conflict::Effect::Moved { actor, position } => {
                    direct_events.push(EventKind::CombatMoved {
                        encounter: id,
                        actor: *actor,
                        position: *position,
                    })
                }
                game_conflict::Effect::Struck {
                    actor,
                    target,
                    damage,
                    ..
                } => {
                    if let Some(death) = &death {
                        direct_events.extend(death.events());
                    }
                    direct_events.push(EventKind::Struck {
                        encounter: id,
                        actor: *actor,
                        target: *target,
                        damage: *damage,
                    });
                }
                game_conflict::Effect::Surrendered { actor } => {
                    direct_events.push(EventKind::Surrendered {
                        encounter: id,
                        actor: *actor,
                    })
                }
                game_conflict::Effect::ObstacleDamaged {
                    map,
                    obstacle,
                    damage,
                } => direct_events.push(EventKind::ObstacleDamaged {
                    map: *map,
                    obstacle: *obstacle,
                    damage: *damage,
                }),
                game_conflict::Effect::Capture { .. } => {
                    let receipt = confinement_guard
                        .as_ref()
                        .ok_or(Error::Unavailable)?
                        .receipt();
                    direct_events.push(EventKind::Transferred {
                        actor: receipt.captor,
                        source: receipt.source,
                        destination: receipt.binding,
                        quantity: receipt.quantity,
                    });
                    direct_events.push(EventKind::Captured {
                        encounter: id,
                        actor: receipt.captor,
                        target: receipt.target,
                        binding: receipt.binding,
                    });
                }
                game_conflict::Effect::Release { .. } => {
                    direct_events
                        .extend(release_finish.as_ref().ok_or(Error::Unavailable)?.events());
                }
                _ => {}
            }
        }
        let traits = self.stage_trait_events(&direct_events, &mut batch)?;
        let death_invocation = if let Some((target, _, _)) = &strike {
            if death.is_some() {
                self.prepare_death_invocation(*target, None, &mut batch)?
            } else {
                None
            }
        } else {
            None
        };
        let combat_guard = prepared.stage_with_death(
            self.host.state.combat.as_ref().unwrap(),
            &self.host.storage,
            &mut batch,
            injury_guard.as_ref(),
            confinement_guard.as_ref(),
            practice_guard.as_ref(),
            release_finish.as_ref().map(|finish| finish.force_receipt()),
            release_finish
                .as_ref()
                .and_then(|finish| finish.motion_finalization()),
            death.as_ref().map(|death| death.motion_finalization()),
        )?;
        combat_guard.validate(
            self.host.state.combat.as_ref().unwrap(),
            &self.host.storage,
            &batch,
        )?;
        if let Some(guard) = &injury_guard {
            guard.validate(&self.host.storage, &batch)?;
        }
        if let Some(guard) = &practice_guard {
            guard.validate(
                self.host.state.skills.as_ref().unwrap(),
                &self.host.storage,
                &batch,
            )?;
        }
        if let Some(guard) = &confinement_guard {
            guard.validate(&self.host.storage, &batch)?;
        }
        if let Some((_, guard)) = &mission_order {
            guard.validate(&self.host.storage, &batch)?;
        }
        if let Some(finish) = &release_finish {
            finish.validate_batch(self, &batch)?;
        }
        if let Some(death) = &death {
            death.validate(self, &batch)?;
        }
        if let Some(traits) = &traits {
            self.validate_trait_events(traits, &batch)?;
        }
        if let Some(surrender) = &surrender {
            surrender.validate_batch(&self.host.storage, &batch)?;
        }
        if let Some(invocation) = &death_invocation {
            invocation.validate(self, &batch)?;
        }
        let combat_guard = combat_guard.into_guard(
            self.host.state.combat.as_ref().unwrap(),
            &self.host.storage,
            &batch,
        )?;
        let mut transaction = game_world::Transaction::new(transaction_opening, batch);
        transaction.ids = u64::from(confinement_guard.is_some());
        transaction.revisions = 1;
        transaction.guards.push(combat_guard);
        transaction.events = direct_events;
        if let Some(traits) = &traits {
            transaction
                .guards
                .extend(traits.row_guards().iter().cloned());
        }
        if let Some(surrender) = surrender {
            transaction.guards.push(surrender.into_guard());
        }
        if let Some(death) = &death {
            transaction.guards.extend(death.guards());
        }
        if let Some(invocation) = &death_invocation {
            transaction.guards.extend(invocation.guards());
        }
        self.host.commit_transaction(transaction)?;
        if let Some(death) = death.take() {
            death.finish(self);
        }
        if let Some(invocation) = death_invocation {
            invocation.finish(self);
        }
        for effect in effects {
            match effect {
                game_conflict::Effect::Moved { .. } => {}
                game_conflict::Effect::Struck { .. } => {}
                game_conflict::Effect::Surrendered { .. } => {
                    let cursor = self.host.next_event() - 1;
                    self.host
                        .state
                        .force
                        .get_or_insert_with(|| game_conflict::force::Runtime::new(cursor));
                }
                game_conflict::Effect::Capture { .. } => {
                    let receipt = confinement_guard
                        .as_ref()
                        .expect("prepared capture receipt")
                        .receipt();
                    self.inventory_index
                        .entry(InventoryLocation::Carried(receipt.target))
                        .or_default()
                        .push(receipt.binding);
                }
                game_conflict::Effect::Release { .. } => {}
                game_conflict::Effect::MissionOrder => {}
                game_conflict::Effect::ObstacleDamaged { .. } => {}
            }
        }
        if let Some(finish) = release_finish {
            finish.finish_metadata(self);
        }
        if let Some((finish, _)) = mission_order {
            self.finish_mission_combat_order(finish, mission_events, mission_notices);
        }
        Ok(())
    }
    pub(crate) fn drive_combat_npcs(&mut self, id: EncounterId) -> Result<(), Error> {
        let world = self
            .host
            .state
            .combat
            .as_ref()
            .ok_or(Error::Unavailable)?
            .clone();
        world.drive_npcs(id, &mut CombatDriver(self))
    }
    pub(crate) fn preflight_combat(&self, due: &[Due], at: SimTime) -> Result<(), Error> {
        let Some(world) = &self.host.state.combat else {
            return Ok(());
        };
        let ids: Vec<_> = due
            .iter()
            .filter_map(|deadline| {
                if let Due::CombatRound(id) = deadline {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();
        for request in world.preflight_rounds(&self.host.storage, &ids, at)? {
            match request {
                game_conflict::RetreatRequest::Road { point, to } => {
                    self.road_retreat_plan(point, to, at)?;
                }
                game_conflict::RetreatRequest::Area { from, to } => {
                    let link = self
                        .host
                        .state
                        .areas
                        .iter()
                        .find(|area| area.id == from)
                        .and_then(|area| area.links.iter().find(|link| link.destination == to))
                        .ok_or(Error::NoRoute)?;
                    at.0.checked_add(link.duration.0).ok_or(Error::Overflow)?;
                }
            }
        }
        Ok(())
    }

    fn road_retreat_plan(
        &self,
        point: game_transport::RoadPosition,
        destination: AreaId,
        at: SimTime,
    ) -> Result<game_transport::Leg, Error> {
        let duration = |from, to| {
            self.host
                .state
                .areas
                .iter()
                .find(|area| area.id == from)
                .and_then(|area| area.links.iter().find(|link| link.destination == to))
                .map(|link| link.duration)
        };
        game_transport::retreat_with_routes(
            point,
            destination,
            duration(point.from, point.to),
            duration(point.to, point.from),
            at,
        )
    }
    pub(crate) fn finish_combat_round(&mut self, id: EncounterId) -> Result<(), Error> {
        let encounter = game_conflict::encounter(&self.host.storage, id)
            .ok_or(Error::Unavailable)?
            .clone();
        let facts: Vec<_> = encounter
            .fighters
            .keys()
            .map(|actor| game_conflict::RoundFighterFacts {
                actor: *actor,
                alive: game_physiology::body(&self.host.storage, *actor)
                    .expect("body exists")
                    .vitals
                    .alive,
                force_can_act: self.force_can_combat_act(*actor),
                carried: matches!(
                    game_spatial::location(&self.host.storage, game_ecs::StableId(actor.0)),
                    Some(Location::Carried { .. })
                ),
            })
            .collect();
        let world = self.host.state.combat.as_ref().unwrap();
        let outcome = world
            .prepare_finish_round(&self.host.storage, id, self.host.clock().time, &facts)?
            .commit(world, &mut self.host.storage)?;
        for retreat in outcome.retreats {
            let game_conflict::RoundRetreat {
                actor,
                destination,
                movement,
            } = retreat;
            self.host.storage.set_activity(actor, Activity::Idle)?;
            match movement {
                game_conflict::RetreatMovement::Carried => {}
                game_conflict::RetreatMovement::Road => {
                    self.transport_retreat(actor, destination)?
                }
                game_conflict::RetreatMovement::Area => self.start_movement(actor, destination)?,
            }
        }
        self.emit(EventKind::RoundEnded {
            encounter: id,
            round: outcome.round,
        });
        for actor in outcome.idle {
            if matches!(
                self.host.storage.activity(actor),
                Some(Activity::Combat { .. })
            ) {
                self.host.storage.set_activity(actor, Activity::Idle)?;
            }
        }
        if outcome.ended {
            self.emit(EventKind::EncounterEnded { encounter: id });
            self.mission_react_to_encounter_end(id);
        } else {
            self.begin_combat_round(id)?;
        }
        Ok(())
    }
}

impl Simulation {
    pub(crate) fn can_release(&self, actor: PersonId, target: PersonId) -> bool {
        let Some(world) = &self.host.state.combat else {
            return false;
        };
        let Some(body) = game_physiology::body(&self.host.storage, actor) else {
            return false;
        };
        world.rules.release_available(
            game_conflict::ReleaseFacts {
                actor,
                target,
                body: game_conflict::TargetBody {
                    alive: body.vitals.alive,
                    health: body.vitals.health,
                },
                independent: self.independent(actor),
                available_area: self.force_area(actor).is_some(),
                actor_captive: game_conflict::captivity(&self.host.storage, actor).is_some(),
                contact: self.transport_same_place(actor, target),
                moving: self.transport_is_moving(actor),
            },
            game_conflict::captivity(&self.host.storage, target),
            self.inventory_index
                .get(&InventoryLocation::Carried(actor))
                .into_iter()
                .flatten()
                .filter_map(|id| game_cargo::inventory(&self.host.storage, *id)),
        )
    }
}

struct CombatDriver<'a>(&'a mut Simulation);
impl game_conflict::CombatDrivePort for CombatDriver<'_> {
    type Action = CombatAction;
    fn store(&self) -> &game_ecs::Store {
        &self.0.host.storage
    }
    fn ruler(&self) -> PersonId {
        self.0.host.state.ruler
    }
    fn controlled(&self, actor: PersonId) -> bool {
        self.0.past_life_controls_actor(actor)
    }
    fn can_act(&self, actor: PersonId) -> bool {
        self.0.can_combat_act(actor)
    }
    fn observe(&mut self, id: EncounterId, actor: PersonId) -> Result<(), Error> {
        self.0.observe_fighter(id, actor)
    }
    fn review_mission(&mut self, id: EncounterId, actor: PersonId) {
        self.0.review_mission_combat(id, actor);
    }
    fn mission_action(
        &mut self,
        id: EncounterId,
        actor: PersonId,
    ) -> Option<(Option<Point>, CombatAction)> {
        self.0.mission_combat_action(id, actor)
    }
    fn force_action(&self, id: EncounterId, actor: PersonId) -> Option<CombatAction> {
        self.0.force_combat_action(actor, id)
    }
    fn autonomy_facts(
        &self,
        id: EncounterId,
        actor: PersonId,
    ) -> Result<game_conflict::CombatAutonomyFacts, Error> {
        let sim = &self.0;
        let encounter =
            game_conflict::encounter(&sim.host.storage, id).ok_or(Error::Unavailable)?;
        let fighter = encounter.fighters.get(&actor).ok_or(Error::Unavailable)?;
        let targets = fighter
            .known
            .iter()
            .filter_map(|(target, known_position)| {
                let other = encounter.fighters.get(target)?;
                Some(game_conflict::CombatTargetFacts {
                    actor: *target,
                    known_position: *known_position,
                    current_position: other.position,
                    opposing: other.team != fighter.team,
                    surrendered: other.surrendered,
                    can_fight: sim.can_fight(*target),
                    captive: game_conflict::captivity(&sim.host.storage, *target).is_some(),
                    force_can_act: sim.force_can_combat_act(*target),
                })
            })
            .collect();
        let personality = match sim.invocation_personality(actor) {
            Some(personality) => personality,
            None => game_characters::decisions::mind::<game_api::Command>(&sim.host.storage, actor)
                .ok_or(Error::Unavailable)?
                .personality
                .clone(),
        };
        let rescue_target = sim.force_rescue_target(actor);
        Ok(game_conflict::CombatAutonomyFacts {
            actor,
            team: fighter.team,
            position: fighter.position,
            health: game_physiology::body(&sim.host.storage, actor)
                .ok_or(Error::Unavailable)?
                .vitals
                .health,
            max_health: sim
                .host
                .state
                .physical
                .as_ref()
                .ok_or(Error::Unavailable)?
                .rules
                .max_health,
            risk_tolerance: personality.risk_tolerance,
            guardian: sim.person(actor).role == Role::Guardian,
            ruler: sim.host.state.ruler,
            rescue_target,
            can_release: rescue_target.is_some_and(|target| sim.can_release(actor, target)),
            binding_available: sim.binding_stock(actor).is_some(),
            targets,
            search: fighter.search.clone(),
        })
    }
    fn bind_action(&self, action: game_conflict::CombatAutonomyAction) -> CombatAction {
        match action {
            game_conflict::CombatAutonomyAction::Defend => CombatAction::Defend,
            game_conflict::CombatAutonomyAction::Surrender => CombatAction::Surrender,
            game_conflict::CombatAutonomyAction::Capture(target) => {
                CombatAction::Capture { target }
            }
            game_conflict::CombatAutonomyAction::Attack(target) => CombatAction::Attack { target },
            game_conflict::CombatAutonomyAction::Release(target) => {
                CombatAction::Release { target }
            }
        }
    }
    fn wait_action(&self) -> CombatAction {
        CombatAction::Wait
    }
    fn apply_turn(
        &mut self,
        id: EncounterId,
        actor: PersonId,
        movement: Option<Point>,
        action: CombatAction,
        search: Option<game_conflict::SearchState>,
    ) -> Result<(), Error> {
        self.0
            .execute_turn_with_search(id, actor, movement, action, search)
    }
    fn skip(&mut self, id: EncounterId) -> Result<(), Error> {
        self.0
            .host
            .state
            .combat
            .as_ref()
            .unwrap()
            .skip_turn(&mut self.0.host.storage, id)
    }
    fn schedule(&mut self, id: EncounterId, ends: SimTime) {
        if !self.0.host.state.due.get(&ends).is_some_and(|events| {
            events
                .iter()
                .any(|event| matches!(event,Due::CombatRound(encounter) if *encounter==id))
        }) {
            self.0
                .host
                .state
                .due
                .entry(ends)
                .or_default()
                .push(Due::CombatRound(id));
        }
    }
}

pub(crate) fn validate(s: &CanonicalState) -> Result<(), Error> {
    let Some(world) = &s.state.combat else {
        return game_conflict::validate_absence(s.components);
    };
    let locations = transport::resolved_locations_for(s)?;
    let people = s
        .people()
        .map(|person| {
            let (anchor, _) =
                game_spatial::resolved_anchor(s.components, game_ecs::StableId(person.id.0))?;
            let combat = match s.components.activity(person.id) {
                Some(Activity::Combat { encounter }) => Some(*encounter),
                _ => None,
            };
            Ok((
                person.id,
                game_conflict::SavedPerson {
                    alive: game_physiology::body(s.components, person.id)
                        .ok_or(Error::Unavailable)?
                        .vitals
                        .alive,
                    location: locations[&person.id].clone(),
                    combat,
                    anchor: PersonId(anchor.0),
                },
            ))
        })
        .collect::<Result<BTreeMap<_, _>, Error>>()?;
    let facts = game_conflict::SaveFacts {
        people,
        areas: s.state.areas.iter().map(|area| area.id).collect(),
        links: s
            .state
            .areas
            .iter()
            .flat_map(|area| {
                area.links
                    .iter()
                    .map(move |link| ((area.id, link.destination), link.duration))
            })
            .collect(),
        deadlines: s
            .state
            .due
            .iter()
            .flat_map(|(at, events)| {
                events.iter().filter_map(move |event| {
                    if let Due::CombatRound(id) = event {
                        Some((*id, *at))
                    } else {
                        None
                    }
                })
            })
            .collect(),
        max_health: s
            .state
            .physical
            .as_ref()
            .ok_or_else(|| Error::InvalidSave("combat without physical runtime".into()))?
            .rules
            .max_health,
    };
    world.validate(s.components, s.time, &facts)
}

#[cfg(test)]
mod command_publication_tests {
    use super::*;

    fn opening(seed: u64, property: bool) -> (Simulation, EncounterId, PersonId) {
        let mut sim = if property {
            Simulation::property_fixture(seed, Execution::default()).unwrap()
        } else {
            Simulation::combat_fixture(seed, Execution::default()).unwrap()
        };
        let actor = sim.host.state.ruler;
        let other = sim.person_at(3).id;
        let encounter = sim
            .debug_start_encounter(vec![(actor, 0), (other, 1)])
            .unwrap();
        assert_eq!(
            game_conflict::encounter(&sim.host.storage, encounter)
                .unwrap()
                .order
                .first(),
            Some(&actor)
        );
        (sim, encounter, other)
    }

    fn waiting(sim: &Simulation, encounter: EncounterId) -> Submission {
        Submission {
            caller_sequence: sim.next_caller_sequence().unwrap(),
            command: Command::CombatTurn {
                encounter,
                movement: None,
                action: CombatAction::Wait,
            },
        }
    }

    #[test]
    fn later_npc_failure_preserves_player_turn_rows_events_counters_and_receipts() {
        let (mut sim, encounter, npc) = opening(211, false);
        let skills = game_skills::Runtime::default();
        game_skills::insert_skills(&mut sim.host.storage, npc, game_skills::Skills::default())
            .unwrap();
        skills
            .credit(
                &mut sim.host.storage,
                npc,
                Skill::Combat,
                skills.rules.per_level.0,
                None,
            )
            .unwrap();
        sim.host.state.skills = Some(skills);
        sim.host.state.combat.as_mut().unwrap().rules.damage = u64::MAX;
        sim.host
            .state
            .combat
            .as_ref()
            .unwrap()
            .rules
            .validate()
            .unwrap();
        let submission = waiting(&sim, encounter);
        let before = sim.host.clone();

        // The player has no damage bonus and its Wait is valid. The following
        // skilled NPC cannot represent the effective damage under these rules.
        assert_eq!(sim.submit_combat(submission), Err(Error::Overflow));
        assert_eq!(sim.host, before);

        sim.execute_turn(encounter, sim.host.state.ruler, None, CombatAction::Wait)
            .unwrap();
        assert_eq!(sim.drive_combat_npcs(encounter), Err(Error::Overflow));
        assert_ne!(sim.host, before);
    }

    #[test]
    fn later_property_receipt_failure_preserves_the_complete_combat_command() {
        let (mut sim, encounter, _) = opening(212, true);
        let actor = sim.host.state.ruler;
        let source = sim.person(actor).inventory;
        // A queued material receipt is admitted by Property after the combat
        // turns. It exceeds the recorded portions and must reject the command.
        sim.host
            .append_many(vec![EventKind::Consumed {
                actor,
                source,
                quantity: Quantity(u64::MAX),
            }])
            .unwrap();
        let submission = waiting(&sim, encounter);
        let before = sim.host.clone();
        assert!(sim.submit_combat(submission).is_err());
        assert_eq!(sim.host, before);

        sim.execute_turn(encounter, actor, None, CombatAction::Wait)
            .unwrap();
        sim.drive_combat_npcs(encounter).unwrap();
        assert!(sim.update_property_events().is_err());
        assert_ne!(sim.host, before);
    }

    #[test]
    fn combined_command_preserves_sequential_turn_maintenance_and_receipt_order() {
        fn enforcement_opening() -> (Simulation, EncounterId) {
            let mut sim = Simulation::enforcement_fixture(213, Execution::default()).unwrap();
            let actor = sim.host.state.ruler;
            let opponent = sim
                .people()
                .find(|person| person.id != actor && sim.can_combat_act(person.id))
                .unwrap()
                .id;
            let encounter = sim
                .debug_start_encounter(vec![(actor, 0), (opponent, 1)])
                .unwrap();
            assert!(
                game_society::enforcement::policies(&sim.host.storage)
                    .next()
                    .is_some()
            );
            (sim, encounter)
        }
        let (mut combined, encounter) = enforcement_opening();
        let (mut sequential, sequential_encounter) = enforcement_opening();
        assert_eq!(encounter, sequential_encounter);
        // The old post-command observer refreshes healthy people in every
        // occupied area, including people outside this encounter's native scene.
        let observers = combined
            .enforcement_observer_people()
            .into_iter()
            .filter(|person| {
                matches!(person.location, Location::AtArea(_))
                    && person.vitals.alive
                    && person.vitals.health > 0
            })
            .map(|person| person.id)
            .collect::<Vec<_>>();
        assert!(observers.len() > 2);
        let submission = waiting(&combined, encounter);
        let receipt = combined.submit_combat(submission.clone()).unwrap();
        sequential
            .execute_turn(
                encounter,
                sequential.host.state.ruler,
                None,
                CombatAction::Wait,
            )
            .unwrap();
        sequential.drive_combat_npcs(encounter).unwrap();
        sequential.update_property_events().unwrap();
        sequential.reconcile_agreements().unwrap();
        sequential.observe_enforcement();
        sequential.host.state.next_command += 1;
        sequential
            .host
            .state
            .accepted
            .insert(submission.caller_sequence, (submission.command, receipt));
        sequential.host.bump_revision(1).unwrap();
        assert_eq!(combined.host, sequential.host);
        assert_eq!(combined.inventory_index, sequential.inventory_index);
        assert_eq!(combined.pregnancy_index, sequential.pregnancy_index);
        assert_eq!(combined.care_index, sequential.care_index);
        assert_eq!(combined.dependent_index, sequential.dependent_index);
        for observer in observers {
            assert!(game_society::enforcement::memory(&combined.host.storage, observer).is_some());
        }
    }
}

#[cfg(test)]
mod mortality_tests {
    use super::*;
    #[test]
    fn actual_lethal_attack_publishes_one_death_and_clears_activity_and_age_deadlines() {
        let mut sim = Simulation::combat_fixture(19, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        let target = sim.person_at(3).id;
        sim.host.state.combat.as_mut().unwrap().rules.damage =
            sim.host.state.physical.as_ref().unwrap().rules.max_health;
        let encounter = sim
            .debug_start_encounter(vec![(actor, 0), (target, 1)])
            .unwrap();
        sim.execute_turn(encounter, actor, None, CombatAction::Attack { target })
            .unwrap();
        assert!(
            !game_physiology::body(&sim.host.storage, target)
                .unwrap()
                .vitals
                .alive
        );
        assert_eq!(sim.host.storage.activity(target), Some(&Activity::Idle));
        assert!(!sim.host.state.due.values().flatten().any(
            |deadline| matches!(deadline,Due::Age(person)|Due::Arrive(person) if *person==target)
        ));
        sim.debug_injure(target, 1).unwrap();
        assert_eq!(
            sim.host
                .events()
                .iter()
                .filter(|event| matches!(event.event,EventKind::Died{actor} if actor==target))
                .count(),
            1
        );
    }
}

#[cfg(test)]
mod reinforcement_tests {
    use super::*;

    #[test]
    fn a_real_arrival_joins_at_an_entrance_and_waits_for_the_next_initiative_round() {
        let mut sim = Simulation::combat_fixture(77, Execution::default()).unwrap();
        let ruler = sim.host.state.ruler;
        let opponent = sim.person_at(1).id;
        let reinforcement = sim.person_at(4).id;
        sim.host.state.combat.as_mut().unwrap().rules.damage = 1;
        sim.debug_set_personality(
            reinforcement,
            decisions::Personality::restful(),
            decisions::Aspiration::Comfort,
        )
        .unwrap();
        sim.debug_move_actor(reinforcement, AreaId(2)).unwrap();
        sim.advance(SimDuration(120_000)).unwrap();
        sim.debug_move_actor(reinforcement, AreaId(1)).unwrap();
        let arrival = match *game_spatial::location(
            &sim.host.storage,
            game_ecs::StableId((sim.person(reinforcement).id).0),
        )
        .expect("location exists")
        {
            Location::InTransit { arrives, .. } => arrives,
            _ => panic!("reinforcements must travel"),
        };
        let encounter = sim
            .debug_start_encounter(vec![(ruler, 0), (opponent, 1)])
            .unwrap();
        let before = sim.canonical_snapshot().unwrap();
        assert_eq!(
            sim.admit_mission_reinforcement(encounter, reinforcement, 0, AreaId(2)),
            Err(Error::Unavailable)
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
        while sim.time() < arrival {
            if sim.awaiting_combat_input().is_some() {
                sim.execute_turn(encounter, ruler, None, CombatAction::Defend)
                    .unwrap();
            }
            sim.advance(SimDuration(6_000)).unwrap();
        }
        sim.admit_mission_reinforcement(encounter, reinforcement, 0, AreaId(2))
            .unwrap();
        let battle = &game_conflict::encounter(&sim.host.storage, encounter).unwrap();
        assert!(!battle.order.contains(&reinforcement));
        assert_eq!(
            battle.fighters[&reinforcement].joins_round,
            battle.round + 1
        );
        let entrance = game_conflict::area_map(&sim.host.storage, AreaId(1))
            .unwrap()
            .exits
            .iter()
            .find(|(area, _)| *area == AreaId(2))
            .unwrap()
            .1;
        assert_eq!(battle.fighters[&reinforcement].position, entrance);
        sim = Simulation::load(
            &sim.save().unwrap(),
            Execution {
                workers: 2,
                reverse_jobs: true,
            },
        )
        .unwrap();
        sim.execute_turn(encounter, ruler, None, CombatAction::Defend)
            .unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
        assert!(
            game_conflict::encounter(&sim.host.storage, encounter)
                .unwrap()
                .order
                .contains(&reinforcement)
        );
    }
}

#[cfg(test)]
mod confinement_tests {
    use super::*;
    fn fixture() -> Simulation {
        let mut sim = Simulation::force_fixture(88, Execution::default()).unwrap();
        for person in sim.people().cloned().collect::<Vec<_>>() {
            sim.debug_actor_command(person.id, Command::Cancel).unwrap();
            sim.debug_set_personality(
                person.id,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )
            .unwrap();
        }
        sim
    }
    fn full_target(sim: &mut Simulation, target: PersonId) {
        let quantity = sim.delivery_carrying_room(target);
        sim.debug_add_inventory(
            Good::Timber,
            InventoryLocation::Carried(target),
            quantity,
            quantity,
            BTreeSet::from([target]),
        )
        .unwrap();
    }
    fn surrendered_encounter(
        sim: &mut Simulation,
        actor: PersonId,
        target: PersonId,
    ) -> (EncounterId, Option<Point>, Point) {
        // Author a clear local battlefield before either participant arrives;
        // this proof exercises binding and initiative, rather than navigation.
        sim.ensure_combat_location_map(&Location::AtArea(AreaId(1)))
            .unwrap();
        game_conflict::area_map_mut(&mut sim.host.storage, AreaId(1))
            .unwrap()
            .obstacles
            .clear();
        let mut cautious = decisions::Personality::restful();
        cautious.risk_tolerance = 0;
        sim.debug_set_personality(target, cautious, decisions::Aspiration::Comfort)
            .unwrap();
        let maximum = sim.host.state.physical.as_ref().unwrap().rules.max_health;
        sim.debug_injure(target, maximum * 3 / 5).unwrap();
        game_conflict::profile_mut(&mut sim.host.storage, target)
            .unwrap()
            .initiative = 1_000_000;
        game_conflict::profile_mut(&mut sim.host.storage, actor)
            .unwrap()
            .initiative = 900_000;
        // Starting the encounter really gives the faster NPC its own turn. It
        // moves and chooses surrender from its actual injury and personality.
        let encounter = sim
            .debug_start_encounter(vec![(actor, 0), (target, 1)])
            .unwrap();
        let world = sim.host.state.combat.as_ref().unwrap();
        let battle = &game_conflict::encounter(&sim.host.storage, encounter).unwrap();
        assert_eq!(battle.order.get(battle.cursor), Some(&actor));
        assert!(battle.fighters[&target].surrendered);
        assert!(sim.host.events().iter().any(|event| matches!(event.event,
            EventKind::Surrendered { encounter: id, actor: who } if id == encounter && who == target)));
        let position = battle.fighters[&target].position;
        let movement = game_conflict::map(&sim.host.storage, battle.map)
            .unwrap()
            .navigation_step(
                battle.fighters[&actor].position,
                position,
                world.rules.movement,
                world.rules.reach,
            );
        assert!(world.reachable(
            &sim.host.storage,
            encounter,
            movement.unwrap_or(battle.fighters[&actor].position),
            target
        ));
        (encounter, movement, position)
    }
    #[test]
    fn actual_initiative_capture_and_timed_restraint_share_conserved_forced_overload() {
        let mut outcomes = Vec::new();
        for tactical in [false, true] {
            let mut sim = fixture();
            let actor = sim.host.state.ruler;
            let target = sim.person_at(3).id;
            full_target(&mut sim, target);
            let source = sim.binding_stock(actor).unwrap();
            let opening = game_cargo::inventory(&sim.host.storage, source)
                .expect("inventory exists")
                .quantity;
            let expected_position = if tactical {
                let (encounter, movement, position) =
                    surrendered_encounter(&mut sim, actor, target);
                sim.execute_turn(encounter, actor, movement, CombatAction::Capture { target })
                    .unwrap();
                position
            } else {
                sim.force_yield(target, actor).unwrap();
                sim.start_restraint(actor, target).unwrap();
                sim.advance(SimDuration(6000)).unwrap();
                Point { x: 6000, y: 8000 }
            };
            let captive = game_conflict::captivity(&sim.host.storage, target)
                .unwrap()
                .clone();
            let stock = game_cargo::inventory(&sim.host.storage, captive.binding)
                .expect("inventory exists");
            assert_eq!(
                game_cargo::inventory(&sim.host.storage, source)
                    .expect("inventory exists")
                    .quantity,
                Quantity(opening.0 - 1000)
            );
            assert_eq!(stock.good, Good::Binding);
            assert_eq!(stock.quantity, Quantity(1000));
            assert_eq!(stock.location, InventoryLocation::Carried(target));
            assert_eq!(stock.access, BTreeSet::from([actor]));
            let held: u128 = sim.inventory_index[&InventoryLocation::Carried(target)]
                .iter()
                .map(|id| {
                    u128::from(
                        game_cargo::inventory(&sim.host.storage, *(id))
                            .expect("inventory exists")
                            .quantity
                            .0,
                    )
                })
                .sum();
            assert_eq!(
                held,
                u128::from(sim.body_rules(target).carrying_capacity.0) + 1000
            );
            assert_eq!(captive.position, expected_position);
            assert!(
                sim.host
                    .state
                    .combat
                    .as_ref()
                    .unwrap()
                    .map(&sim.host.storage, captive.map)
                    .is_some()
            );
            Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
            outcomes.push((
                captive.actor,
                captive.captor,
                captive.area,
                stock.good,
                stock.quantity,
                stock.access.clone(),
            ));
        }
        assert_eq!(outcomes[0], outcomes[1]);
    }
    #[test]
    fn confinement_id_preflight_preserves_tactical_movement_and_timed_map_and_rope() {
        let mut sim = fixture();
        let actor = sim.host.state.ruler;
        let target = sim.person_at(3).id;
        sim.force_yield(target, actor).unwrap();
        {
            let mut authored: serde_json::Value =
                serde_json::from_slice(&sim.save().unwrap()).unwrap();
            authored["state"]["next_id"] = serde_json::json!(u64::MAX - 1);
            sim = Simulation::load(
                &serde_json::to_vec(&authored).unwrap(),
                sim.execution.clone(),
            )
            .unwrap();
        }
        let before = sim.canonical_snapshot().unwrap();
        assert_eq!(sim.start_restraint(actor, target), Err(Error::Overflow));
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
        let mut sim = fixture();
        let actor = sim.host.state.ruler;
        let target = sim.person_at(3).id;
        let (encounter, movement, _) = surrendered_encounter(&mut sim, actor, target);
        {
            let mut authored: serde_json::Value =
                serde_json::from_slice(&sim.save().unwrap()).unwrap();
            authored["state"]["next_id"] = serde_json::json!(u64::MAX);
            sim = Simulation::load(
                &serde_json::to_vec(&authored).unwrap(),
                sim.execution.clone(),
            )
            .unwrap();
        }
        let before = sim.canonical_snapshot().unwrap();
        assert_eq!(
            sim.execute_turn(encounter, actor, movement, CombatAction::Capture { target }),
            Err(Error::Overflow)
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
    }
}
