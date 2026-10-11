//! Bindings are physical goods. Tools, bodily readiness, witnesses and actual
//! initiative encounters govern release; there is no automatic escape on healing.
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    pub restraint_time: SimDuration,
    pub escape_time: SimDuration,
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            restraint_time: SimDuration(6_000),
            escape_time: SimDuration(6_000),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Escape {
    pub target: PersonId,
    pub actor: PersonId,
    pub tool: Option<InventoryId>,
    pub began: SimTime,
    pub completes: SimTime,
    pub contested: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Restraint {
    pub target: PersonId,
    pub binding: InventoryId,
    pub area: AreaId,
    pub location: Location,
    pub position: Point,
    pub completes: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Yielded {
    pub captors: BTreeSet<PersonId>,
    pub area: AreaId,
    pub at: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    #[serde(default)]
    pub yielded: BTreeMap<PersonId, Yielded>,
    pub rules: Rules,
    pub escapes: BTreeMap<PersonId, Escape>,
    pub restraints: BTreeMap<PersonId, Restraint>,
    pub last_event: u64,
}

impl Simulation {
    pub fn force_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::lineage_fixture(seed, execution)?;
        sim.state.scenario = ScenarioInfo {
            id: "force-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        sim.state.force = Some(World {
            yielded: BTreeMap::new(),
            rules: Rules::default(),
            escapes: BTreeMap::new(),
            restraints: BTreeMap::new(),
            last_event: sim.state.next_event - 1,
        });
        Ok(sim)
    }
    fn ensure_force_world(&mut self) {
        if self.state.force.is_none() {
            self.state.force = Some(World {
                yielded: BTreeMap::new(),
                rules: Rules::default(),
                escapes: BTreeMap::new(),
                restraints: BTreeMap::new(),
                last_event: self.state.next_event - 1,
            });
        }
    }
    pub(crate) fn force_yield(&mut self, actor: PersonId, captor: PersonId) -> Result<(), Error> {
        if actor == captor
            || !self.independent(actor)
            || self.in_combat(actor)
            || self.force_area(actor).is_none()
            || !self.transport_same_place(actor, captor)
            || self.state.components.body(actor).unwrap().vitals.health == 0
            || self.state.combat.is_none()
        {
            return Err(Error::Unavailable);
        }
        self.ensure_event_capacity(1)?;
        let area = self.force_area(actor).unwrap();
        self.ensure_force_world();
        self.state.force.as_mut().unwrap().yielded.insert(
            actor,
            Yielded {
                captors: BTreeSet::from([captor]),
                area,
                at: self.state.time,
            },
        );
        self.emit(EventKind::Yielded { actor, captor });
        self.revision += 1;
        Ok(())
    }
    pub(crate) fn force_withdraw_yield(&mut self, actor: PersonId) -> Result<(), Error> {
        if self.force_area(actor).is_none()
            || self.in_combat(actor)
            || !self.independent(actor)
            || !self
                .state
                .force
                .as_ref()
                .is_some_and(|world| world.yielded.contains_key(&actor))
        {
            return Err(Error::Unavailable);
        }
        self.ensure_event_capacity(1)?;
        self.state.force.as_mut().unwrap().yielded.remove(&actor);
        self.emit(EventKind::YieldWithdrawn { actor });
        self.revision += 1;
        Ok(())
    }
    pub(crate) fn force_note_surrender(&mut self, encounter: EncounterId, actor: PersonId) {
        let battle = &self.state.combat.as_ref().unwrap().encounters[&encounter];
        let captors = battle
            .fighters
            .values()
            .filter(|fighter| fighter.team != battle.fighters[&actor].team && !fighter.withdrawn)
            .map(|fighter| fighter.actor)
            .collect::<BTreeSet<_>>();
        let area = battle.area;
        self.ensure_force_world();
        self.state.force.as_mut().unwrap().yielded.insert(
            actor,
            Yielded {
                captors,
                area,
                at: self.state.time,
            },
        );
    }
    fn force_area(&self, actor: PersonId) -> Option<AreaId> {
        let body = self.state.components.body(actor)?;
        if !body.vitals.alive {
            return None;
        }
        match self.resolved_location(actor) {
            Location::AtArea(area) | Location::StoppedOnRoad { from: area, .. } => Some(area),
            Location::InTransit { from, .. }
                if matches!(
                    *self
                        .state
                        .components
                        .location(self.state.people[*self.person_index.get(&actor)?].id)
                        .expect("location exists"),
                    Location::Carried { .. }
                ) =>
            {
                Some(from)
            }
            _ => None,
        }
    }
    pub(crate) fn force_body_ready(&self, actor: PersonId) -> bool {
        self.force_area(actor).is_some()
            && self.independent(actor)
            && self.state.combat.as_ref().is_some_and(|world| {
                self.state.components.body(actor).unwrap().vitals.health
                    > world.rules.incapacitation
            })
    }
    pub(crate) fn force_tool(&self, actor: PersonId) -> Option<InventoryId> {
        self.inventory_index
            .get(&InventoryLocation::Carried(actor))?
            .iter()
            .copied()
            .find(|id| {
                self.state.inventories[id].good == Good::Tool
                    && self.state.inventories[id].quantity.0 >= 1_000
            })
    }
    fn force_binding(&self, actor: PersonId) -> Option<InventoryId> {
        self.inventory_index
            .get(&InventoryLocation::Carried(actor))?
            .iter()
            .copied()
            .find(|id| {
                self.state.inventories[id].good == Good::Binding
                    && self.state.inventories[id].quantity.0 >= 1_000
            })
    }
    fn force_restrainable(&self, actor: PersonId, target: PersonId) -> bool {
        self.force_area(target).is_some()
            && self.state.combat.as_ref().is_some_and(|world| {
                !world.captivity.contains_key(&target)
                    && (self.state.components.body(target).unwrap().vitals.health
                        <= world.rules.incapacitation
                        || self
                            .state
                            .force
                            .as_ref()
                            .and_then(|world| world.yielded.get(&target))
                            .is_some_and(|yielded| {
                                yielded.captors.contains(&actor)
                                    && Some(yielded.area) == self.force_area(target)
                            }))
            })
    }
    fn force_position(&self, target: PersonId, area: AreaId) -> Point {
        self.state
            .combat
            .as_ref()
            .and_then(|world| {
                world
                    .encounters
                    .values()
                    .rev()
                    .find(|encounter| {
                        encounter.area == area && encounter.fighters.contains_key(&target)
                    })
                    .map(|encounter| encounter.fighters[&target].position)
            })
            .unwrap_or(Point { x: 6_000, y: 8_000 })
    }
    pub(crate) fn start_restraint(
        &mut self,
        actor: PersonId,
        target: PersonId,
    ) -> Result<(), Error> {
        let rules = self
            .state
            .force
            .as_ref()
            .map(|world| world.rules.clone())
            .unwrap_or_default();
        if actor == target
            || !self.force_body_ready(actor)
            || self.combat_restricts(actor)
            || self.in_combat(target)
            || !self.transport_same_place(actor, target)
            || !self.force_restrainable(actor, target)
        {
            return Err(Error::Unavailable);
        }
        let binding = self.force_binding(actor).ok_or(Error::Unavailable)?;
        let completes = SimTime(
            self.state
                .time
                .0
                .checked_add(rules.restraint_time.0)
                .ok_or(Error::Overflow)?,
        );
        let area = self.force_area(target).unwrap();
        let location = self.resolved_location(actor);
        let position = self.force_position(target, area);
        self.state.next_id.checked_add(2).ok_or(Error::Overflow)?;
        self.ensure_event_capacity(3)?;
        self.ensure_force_world();
        self.interrupt_actor_activity(actor);
        self.state.force.as_mut().unwrap().restraints.insert(
            actor,
            Restraint {
                target,
                binding,
                area,
                location,
                position,
                completes,
            },
        );
        *self.state.components.activity_mut(actor).unwrap() = Activity::Restrain {
            target,
            binding,
            completes,
        };
        self.state
            .due
            .entry(completes)
            .or_default()
            .push(Due::Physical(actor));
        self.revision += 1;
        Ok(())
    }
    pub(crate) fn start_escape(&mut self, actor: PersonId) -> Result<(), Error> {
        let rules = self
            .state
            .force
            .as_ref()
            .map(|world| world.rules.clone())
            .unwrap_or_default();
        if !self.force_body_ready(actor)
            || self.in_combat(actor)
            || !self
                .state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&actor)
        {
            return Err(Error::Unavailable);
        }
        let tool = self.force_tool(actor).ok_or(Error::Unavailable)?;
        let completes = SimTime(
            self.state
                .time
                .0
                .checked_add(rules.escape_time.0)
                .ok_or(Error::Overflow)?,
        );
        self.preflight_force_intervention()?;
        self.ensure_event_capacity(128)?;
        self.ensure_force_world();
        self.interrupt_actor_activity(actor);
        self.state.force.as_mut().unwrap().escapes.insert(
            actor,
            Escape {
                target: actor,
                actor,
                tool: Some(tool),
                began: self.state.time,
                completes,
                contested: false,
            },
        );
        *self.state.components.activity_mut(actor).unwrap() = Activity::Escape { tool, completes };
        self.state
            .due
            .entry(completes)
            .or_default()
            .push(Due::Physical(actor));
        self.emit(EventKind::EscapeAttempted { actor });
        self.force_intervention(actor)?;
        self.revision += 1;
        Ok(())
    }
    pub(crate) fn preflight_force_intervention(&self) -> Result<(), Error> {
        self.state.next_id.checked_add(2).ok_or(Error::Overflow)?;
        if let Some(combat) = &self.state.combat {
            self.state
                .time
                .0
                .checked_add(combat.rules.round.0)
                .ok_or(Error::Overflow)?;
        }
        self.ensure_event_capacity(128)
    }
    /// Called after a validated FreeCaptive begins, before advancing its clock.
    pub(crate) fn force_release_started(
        &mut self,
        actor: PersonId,
        target: PersonId,
        completes: SimTime,
    ) -> Result<(), Error> {
        self.ensure_force_world();
        let tool = self.force_tool(actor);
        self.state.force.as_mut().unwrap().escapes.insert(
            target,
            Escape {
                target,
                actor,
                tool,
                began: self.state.time,
                completes,
                contested: false,
            },
        );
        self.force_intervention(target)
    }
    fn force_intervention(&mut self, target: PersonId) -> Result<(), Error> {
        let captivity = self
            .state
            .combat
            .as_ref()
            .unwrap()
            .captivity
            .get(&target)
            .cloned()
            .ok_or(Error::Unavailable)?;
        let attempt = self.state.force.as_ref().unwrap().escapes[&target].clone();
        // The player chooses their own intervention. NPC captors and trusted
        // nearby allies choose whether they are willing to defend confinement.
        if attempt.actor == captivity.captor {
            return Ok(());
        }
        let opponents = self
            .state
            .people
            .iter()
            .filter(|person| {
                person.id != self.state.ruler
                    && person.id != attempt.actor
                    && person.id != target
                    && self.transport_same_place(person.id, target)
                    && self.independent(person.id)
                    && self.state.components.body(person.id).unwrap().vitals.alive
                    && self.state.components.body(person.id).unwrap().vitals.health
                        > self.state.combat.as_ref().unwrap().rules.incapacitation
                    && !self.combat_restricts(person.id)
            })
            .filter(|person| {
                let mind = &self.state.decisions.as_ref().unwrap().minds[&person.id];
                if person.id == captivity.captor {
                    mind.personality.risk_tolerance >= 500 && mind.personality.aggression >= 500
                } else {
                    mind.personality.risk_tolerance >= 700
                        && mind.personality.aggression >= 700
                        && mind
                            .relationships
                            .get(&captivity.captor)
                            .is_some_and(|relation| relation.trust >= 500)
                }
            })
            .map(|person| person.id)
            .take(8)
            .collect::<Vec<_>>();
        if opponents.is_empty() {
            return Ok(());
        }
        self.transport_stop_for_force(target)?;
        self.transport_stop_for_force(attempt.actor)?;
        for opponent in &opponents {
            self.transport_stop_for_force(*opponent)?;
        }
        self.state
            .force
            .as_mut()
            .unwrap()
            .escapes
            .get_mut(&target)
            .unwrap()
            .contested = true;
        let mut participants = vec![(attempt.actor, 0)];
        if attempt.actor != target {
            participants.push((target, 0));
        }
        participants.extend(opponents.into_iter().map(|actor| (actor, 1)));
        self.debug_start_encounter(participants)?;
        Ok(())
    }
    pub(crate) fn force_passive_participant(&self, actor: PersonId) -> bool {
        self.force_area(actor).is_some()
            && self
                .state
                .combat
                .as_ref()
                .is_some_and(|world| world.captivity.contains_key(&actor))
    }
    pub(crate) fn force_can_combat_act(&self, actor: PersonId) -> bool {
        self.force_body_ready(actor)
            && self.state.force.as_ref().is_some_and(|world| {
                world
                    .escapes
                    .get(&actor)
                    .is_some_and(|escape| escape.contested)
            })
    }
    pub(crate) fn force_action_allowed(
        &self,
        actor: PersonId,
        movement: Option<Point>,
        action: &CombatAction,
    ) -> bool {
        if !self
            .state
            .combat
            .as_ref()
            .is_some_and(|world| world.captivity.contains_key(&actor))
        {
            return true;
        }
        movement.is_none()
            && match action {
                CombatAction::Defend | CombatAction::Wait => true,
                CombatAction::Release { target } => {
                    *target == actor && self.force_tool(actor).is_some()
                }
                _ => false,
            }
    }
    /// Keep contested attempts when physical actions become combat actions.
    pub(crate) fn interrupt_force(&mut self, actor: PersonId) {
        let Some(world) = &mut self.state.force else {
            return;
        };
        world.restraints.remove(&actor);
        world.escapes.retain(|target, attempt| {
            (*target != actor && attempt.actor != actor) || attempt.contested
        });
    }
    pub(crate) fn confinement_facts(
        &self,
        captor: PersonId,
        target: PersonId,
        surrendered_to_captor: bool,
        contact: bool,
        admitted: Option<&Location>,
    ) -> Result<game_conflict::ConfinementFacts, Error> {
        let combat = self.state.combat.as_ref().ok_or(Error::Unavailable)?;
        self.state.physical.as_ref().ok_or(Error::Unavailable)?;
        let actor = self
            .state
            .components
            .body(captor)
            .ok_or(Error::Unavailable)?;
        let victim = self
            .state
            .components
            .body(target)
            .ok_or(Error::Unavailable)?;
        Ok(game_conflict::ConfinementFacts {
            captor,
            target,
            captor_alive: actor.vitals.alive,
            captor_health: actor.vitals.health,
            independent: self.independent(captor),
            captor_confined: combat.captivity.contains_key(&captor),
            target_alive: victim.vitals.alive,
            target_health: victim.vitals.health,
            target_confined: combat.captivity.contains_key(&target),
            surrendered_to_captor,
            contact,
            admitted_site_contact: admitted.is_none_or(|location| {
                let key = transport::location_position(location, self.state.time)
                    .map(|point| point.key());
                key.is_ok_and(|key| {
                    self.transport_position(captor)
                        .is_some_and(|point| point.key() == key)
                        && self
                            .transport_position(target)
                            .is_some_and(|point| point.key() == key)
                })
            }),
            captor_location: self.resolved_location(captor),
            target_location: self.resolved_location(target),
        })
    }
    fn restraint_context(
        &self,
        location: &Location,
        target_position: Point,
    ) -> Result<(game_conflict::ConfinementContext, combat::PreparedCombatMap), Error> {
        let plan = self.prepare_combat_location_map(location)?;
        let map = plan.map();
        if !map.contains(target_position) {
            return Err(Error::Unavailable);
        }
        let context = game_conflict::ConfinementContext {
            map: map.id,
            area: map.area,
            location: location.clone(),
            target_position,
        };
        Ok((context, plan))
    }
    pub(crate) fn apply_confinement(
        &mut self,
        plan: game_conflict::ValidatedConfinement,
        new_map: Option<combat::PreparedCombatMap>,
    ) -> Result<(), Error> {
        let receipt = plan.receipt().clone();
        let allocation_count = 1 + u64::from(new_map.is_some());
        self.state
            .next_id
            .checked_add(allocation_count)
            .ok_or(Error::Overflow)?;
        let expected_binding = self
            .state
            .next_id
            .checked_add(u64::from(new_map.is_some()))
            .ok_or(Error::Overflow)?;
        if receipt.binding != InventoryId(expected_binding) {
            return Err(Error::Unavailable);
        }
        self.ensure_event_capacity(3)?;
        let validated = plan;
        if let Some(scene) = new_map {
            self.install_combat_location_map(scene);
        }
        let binding = InventoryId(allocate(&mut self.state).expect("preflight confinement ID"));
        debug_assert_eq!(binding, receipt.binding);
        let receipt = validated.apply(
            &mut self.state.inventories,
            &mut self.state.combat.as_mut().unwrap().captivity,
        );
        self.inventory_index
            .entry(InventoryLocation::Carried(receipt.target))
            .or_default()
            .push(receipt.binding);
        if receipt.interrupt_target {
            self.interrupt_actor_activity(receipt.target);
        }
        self.emit(EventKind::Transferred {
            actor: receipt.captor,
            source: receipt.source,
            destination: receipt.binding,
            quantity: receipt.quantity,
        });
        match receipt.mode {
            game_conflict::ConfinementMode::Tactical { encounter } => {
                self.emit(EventKind::Captured {
                    encounter,
                    actor: receipt.captor,
                    target: receipt.target,
                    binding: receipt.binding,
                })
            }
            game_conflict::ConfinementMode::Timed { .. } => {
                self.reconcile_transport();
                self.emit(EventKind::Restrained {
                    actor: receipt.captor,
                    target: receipt.target,
                    binding: receipt.binding,
                });
            }
        }
        Ok(())
    }
    pub(crate) fn complete_force(&mut self, actor: PersonId) {
        let activity = self.state.components.activity(actor).unwrap().clone();
        match activity {
            Activity::Escape { tool, .. } => {
                let successful = self.force_body_ready(actor)
                    && self.state.inventories.get(&tool).is_some_and(|stock| {
                        stock.good == Good::Tool
                            && stock.location == InventoryLocation::Carried(actor)
                            && stock.quantity.0 >= 1_000
                    })
                    && self
                        .state
                        .combat
                        .as_ref()
                        .unwrap()
                        .captivity
                        .contains_key(&actor);
                self.state.force.as_mut().unwrap().escapes.remove(&actor);
                if successful {
                    self.release_captive(actor, actor);
                } else {
                    self.emit(EventKind::EscapeInterrupted { actor });
                }
            }
            Activity::Restrain {
                target, binding, ..
            } => {
                let Some(attempt) = self.state.force.as_mut().unwrap().restraints.remove(&actor)
                else {
                    return;
                };
                let result = (|| -> Result<(), Error> {
                    let (context, scene) =
                        self.restraint_context(&attempt.location, attempt.position)?;
                    let new_map = scene.is_new();
                    let applied = InventoryId(
                        self.state
                            .next_id
                            .checked_add(u64::from(new_map))
                            .ok_or(Error::Overflow)?,
                    );
                    let surrendered = self
                        .state
                        .force
                        .as_ref()
                        .and_then(|world| world.yielded.get(&target))
                        .is_some_and(|yielded| {
                            yielded.captors.contains(&actor)
                                && Some(yielded.area) == self.force_area(target)
                        });
                    let facts = self.confinement_facts(
                        actor,
                        target,
                        surrendered,
                        self.transport_same_place(actor, target),
                        Some(&attempt.location),
                    )?;
                    let source = self
                        .state
                        .inventories
                        .get(&binding)
                        .ok_or(Error::Unavailable)?;
                    let plan = game_conflict::prepare_confinement(
                        &self.state.combat.as_ref().unwrap().rules,
                        facts.clone(),
                        source,
                        applied,
                        context,
                        game_conflict::ConfinementMode::Timed {
                            completes: attempt.completes,
                            admitted_location: attempt.location.clone(),
                        },
                        self.state.time,
                    )?;
                    let validated = plan.begin(
                        &self.state.inventories,
                        &self.state.combat.as_ref().unwrap().captivity,
                        &facts,
                    )?;
                    self.apply_confinement(validated, new_map.then_some(scene))
                })();
                if result.is_err() {
                    self.emit(EventKind::Interrupted { actor });
                }
            }
            _ => return,
        }
        *self.state.components.activity_mut(actor).unwrap() = Activity::Idle;
    }
    pub(crate) fn force_guardian_rescue(
        &self,
        guardian: PersonId,
        target: PersonId,
    ) -> Option<Command> {
        self.state.combat.as_ref()?.captivity.get(&target)?;
        if !self.transport_same_place(guardian, target)
            || !self.force_body_ready(guardian)
            || self.combat_restricts(guardian)
        {
            return None;
        }
        if self.force_tool(guardian).is_some() {
            return Some(Command::FreeCaptive { target });
        }
        let mind = self.state.decisions.as_ref()?.minds.get(&guardian)?;
        let tool = mind.knowledge.values().find(|fact| {
            fact.good == Good::Tool
                && Some(fact.area) == self.force_area(guardian)
                && matches!(self.resolved_location(guardian), Location::AtArea(_))
                && fact.accessible
                && fact.quantity.0 >= 1_000
        })?;
        Some(Command::Carry {
            source: tool.inventory,
            quantity: Quantity(1_000),
        })
    }
    pub(crate) fn force_candidate(&self, actor: PersonId) -> Option<Command> {
        if !self.force_body_ready(actor)
            || !matches!(
                *self.state.components.activity(actor).unwrap(),
                Activity::Idle | Activity::Rest
            )
            || self.in_combat(actor)
            || !self.state.combat.as_ref()?.captivity.contains_key(&actor)
            || self.force_tool(actor).is_none()
        {
            return None;
        }
        Some(Command::Escape)
    }
    pub(crate) fn force_allocation_bound(&self, due: &[Due]) -> u64 {
        let Some(world) = &self.state.force else {
            return 0;
        };
        let attempts = due
            .iter()
            .filter_map(|event| {
                if let Due::Physical(actor) = event {
                    world.restraints.get(actor)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        // A closing restraint can allocate a scene map and its applied rope.
        attempts.len() as u64 * 2
    }
    pub(crate) fn available_force_commands(&self, actor: PersonId) -> Vec<Command> {
        if self.state.combat.is_none() {
            return Vec::new();
        }
        if self.force_area(actor).is_none() {
            return Vec::new();
        };
        if !self.independent(actor) || self.in_combat(actor) {
            return Vec::new();
        }
        let mut commands = Vec::new();
        if self.force_candidate(actor).is_some() {
            commands.push(Command::Escape);
        }
        if self
            .state
            .force
            .as_ref()
            .is_some_and(|world| world.yielded.contains_key(&actor))
        {
            commands.push(Command::WithdrawYield);
        }
        for target in self
            .state
            .people
            .iter()
            .filter(|person| person.id != actor && self.transport_same_place(actor, person.id))
            .take(8)
        {
            commands.push(Command::Yield { captor: target.id });
            if self.force_body_ready(actor)
                && !self.combat_restricts(actor)
                && !self.in_combat(target.id)
                && self.force_binding(actor).is_some()
                && self.force_restrainable(actor, target.id)
            {
                commands.push(Command::Restrain { target: target.id });
            }
        }
        commands
    }
    pub(crate) fn preflight_force(&self, due: &[Due]) -> Result<(), Error> {
        let Some(world) = &self.state.force else {
            return Ok(());
        };
        let count = due
            .iter()
            .filter_map(|event| {
                if let Due::Physical(actor) = event {
                    world.restraints.get(actor)
                } else {
                    None
                }
            })
            .try_fold(0_u64, |count, _| {
                count.checked_add(2).ok_or(Error::Overflow)
            })?;
        self.state
            .next_id
            .checked_add(count)
            .ok_or(Error::Overflow)?;
        Ok(())
    }
    pub(crate) fn update_force(&mut self) {
        let Some(world) = &self.state.force else {
            return;
        };
        let released = world
            .escapes
            .keys()
            .filter(|actor| {
                !self
                    .state
                    .combat
                    .as_ref()
                    .unwrap()
                    .captivity
                    .contains_key(actor)
            })
            .copied()
            .collect::<Vec<_>>();
        for actor in released {
            self.state.force.as_mut().unwrap().escapes.remove(&actor);
        }
        let finished =
            self.state
                .force
                .as_ref()
                .unwrap()
                .escapes
                .iter()
                .filter(|(target, attempt)| {
                    attempt.contested
                        && !self.state.combat.as_ref().unwrap().encounters.values().any(
                            |encounter| {
                                encounter.active
                                    && encounter.fighters.contains_key(target)
                                    && encounter.fighters.contains_key(&attempt.actor)
                            },
                        )
                })
                .map(|(target, _)| *target)
                .collect::<Vec<_>>();
        for target in finished {
            self.state.force.as_mut().unwrap().escapes.remove(&target);
        }
        let cursor = self.state.force.as_ref().unwrap().last_event;
        let events = self
            .state
            .events
            .iter()
            .filter(|event| event.id.0 > cursor)
            .cloned()
            .collect::<Vec<_>>();
        for event in events {
            match event.kind {
                EventKind::Departed { actor, .. } | EventKind::Struck { actor, .. } => {
                    let yielded = &mut self.state.force.as_mut().unwrap().yielded;
                    yielded.remove(&actor);
                    for declaration in yielded.values_mut() {
                        declaration.captors.remove(&actor);
                    }
                    yielded.retain(|_, declaration| !declaration.captors.is_empty());
                }
                _ => {}
            }
        }
        self.state.force.as_mut().unwrap().last_event = self.state.next_event - 1;
    }
    pub(crate) fn force_rescue_target(&self, actor: PersonId) -> Option<PersonId> {
        self.state
            .force
            .as_ref()?
            .escapes
            .values()
            .find(|attempt| {
                attempt.actor == actor
                    && attempt.target != actor
                    && self
                        .state
                        .combat
                        .as_ref()
                        .is_some_and(|world| world.captivity.contains_key(&attempt.target))
            })
            .map(|attempt| attempt.target)
    }
    pub(crate) fn force_combat_action(
        &self,
        actor: PersonId,
        encounter: EncounterId,
    ) -> Option<CombatAction> {
        let battle = self.state.combat.as_ref()?.encounters.get(&encounter)?;
        if self.force_passive_participant(actor) {
            return Some(if self.force_tool(actor).is_some() {
                CombatAction::Release { target: actor }
            } else {
                CombatAction::Defend
            });
        }
        let target = self.force_rescue_target(actor)?;
        let mine = battle.fighters.get(&actor)?;
        let captive = battle.fighters.get(&target)?;
        let map = &self.state.combat.as_ref()?.maps[&battle.area];
        let dx = i128::from(mine.position.x) - i128::from(captive.position.x);
        let dy = i128::from(mine.position.y) - i128::from(captive.position.y);
        if self.force_tool(actor).is_some()
            && (dx * dx + dy * dy) as u128
                <= u128::from(self.state.combat.as_ref()?.rules.reach).pow(2)
            && map.clear(mine.position, captive.position)
        {
            Some(CombatAction::Release { target })
        } else {
            None
        }
    }
    fn force_visible(&self, observer: PersonId, target: PersonId) -> bool {
        let Some(combat) = &self.state.combat else {
            return false;
        };
        if !combat.captivity.contains_key(&target) {
            return false;
        }
        self.transport_same_place(observer, target)
    }
    pub(crate) fn force_view(&self, actor: PersonId) -> ForceView {
        if self.state.physical.is_none()
            || self
                .state
                .components
                .body(actor)
                .is_none_or(|body| !body.vitals.alive)
        {
            return ForceView::default();
        }
        let Some(combat) = &self.state.combat else {
            return ForceView::default();
        };
        ForceView {
            confinements: combat
                .captivity
                .values()
                .filter(|captive| self.force_visible(actor, captive.actor))
                .take(32)
                .map(|captive| ConfinementView {
                    target: captive.actor,
                    captor: captive.captor,
                    location: self.resolved_location(captive.actor),
                    captured_position: captive.position,
                    can_escape: captive.actor == actor
                        && self.force_body_ready(actor)
                        && self.force_tool(actor).is_some(),
                    escaping: self
                        .state
                        .force
                        .as_ref()
                        .is_some_and(|world| world.escapes.contains_key(&captive.actor)),
                })
                .collect(),
        }
    }
}

pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let Some(world) = &state.force else {
        return Ok(());
    };
    let fail = || Error::InvalidSave("invalid confinement attempt or food supply".into());
    state.physical.as_ref().ok_or_else(fail)?;
    let combat = state.combat.as_ref().ok_or_else(fail)?;
    let people = state
        .people
        .iter()
        .map(|person| (person.id, person))
        .collect::<BTreeMap<_, _>>();
    if world.rules.restraint_time.0 == 0
        || world.rules.escape_time.0 == 0
        || world.last_event >= state.next_event
    {
        return Err(fail());
    }
    for (actor, yielded) in &world.yielded {
        if !people.contains_key(actor)
            || yielded.captors.is_empty()
            || yielded.captors.contains(actor)
            || yielded
                .captors
                .iter()
                .any(|captor| !people.contains_key(captor))
            || yielded.at > state.time
            || !state.areas.iter().any(|area| area.id == yielded.area)
        {
            return Err(fail());
        }
    }
    let locations = transport::resolved_locations_for(state)?;
    for (actor, attempt) in &world.restraints {
        let body = state.components.body(*actor).ok_or_else(fail)?;
        if !people.contains_key(actor)
            || !people.contains_key(&attempt.target)
            || actor == &attempt.target
            || attempt.completes <= state.time
            || !body.vitals.alive
            || locations[actor] != attempt.location
            || locations[&attempt.target] != attempt.location
            || state.inventories.get(&attempt.binding).is_none_or(|stock| {
                stock.good != Good::Binding || stock.location != InventoryLocation::Carried(*actor)
            })
            || *state.components.activity(*actor).ok_or_else(fail)?
                != (Activity::Restrain {
                    target: attempt.target,
                    binding: attempt.binding,
                    completes: attempt.completes,
                })
            || !state.due.get(&attempt.completes).is_some_and(|events| {
                events
                    .iter()
                    .any(|event| matches!(event,Due::Physical(id) if id==actor))
            })
        {
            return Err(fail());
        }
    }
    for (target, attempt) in &world.escapes {
        if *target != attempt.target
            || !people.contains_key(target)
            || !people.contains_key(&attempt.actor)
            || !combat.captivity.contains_key(target)
            || attempt.began > state.time
            || attempt.began >= attempt.completes
        {
            return Err(fail());
        }
        if let Some(tool) = attempt.tool
            && state.inventories.get(&tool).is_none_or(|stock| {
                stock.good != Good::Tool
                    || stock.location != InventoryLocation::Carried(attempt.actor)
            })
        {
            return Err(fail());
        }
        if attempt.contested {
            if !combat.encounters.values().any(|encounter| {
                encounter.active
                    && encounter.fighters.contains_key(target)
                    && encounter.fighters.contains_key(&attempt.actor)
            }) {
                return Err(fail());
            }
        } else {
            if attempt.completes <= state.time {
                return Err(fail());
            }
            let activity = state.components.activity(attempt.actor).ok_or_else(fail)?;
            if !matches!(*activity,Activity::Escape {completes,..}|Activity::Unbind {completes,..} if completes==attempt.completes)
            {
                return Err(fail());
            }
        }
    }
    for (actor, activity) in state.components.activities() {
        match *activity {
            Activity::Escape { .. }
                if !world
                    .escapes
                    .get(&actor)
                    .is_some_and(|attempt| attempt.actor == actor && !attempt.contested) =>
            {
                return Err(fail());
            }
            Activity::Restrain { .. } if !world.restraints.contains_key(&actor) => {
                return Err(fail());
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Simulation {
        let mut sim = Simulation::force_fixture(88, Execution::default()).unwrap();
        let people = sim
            .state
            .people
            .iter()
            .map(|person| person.id)
            .collect::<Vec<_>>();
        for person in people {
            sim.debug_actor_command(person, Command::Cancel).unwrap();
            sim.debug_set_personality(
                person,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )
            .unwrap();
        }
        sim
    }
    fn tool(sim: &Simulation) -> InventoryId {
        sim.state
            .inventories
            .values()
            .find(|stock| {
                stock.good == Good::Tool && stock.location == InventoryLocation::Area(AreaId(1))
            })
            .unwrap()
            .id
    }
    fn capture_by_yield(sim: &mut Simulation, captor: PersonId, target: PersonId) {
        sim.force_yield(target, captor).unwrap();
        sim.start_restraint(captor, target).unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
        assert!(
            sim.state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&target)
        );
    }
    #[test]
    fn direct_binding_requires_actual_yield_or_incapacity_and_consumes_real_binding() {
        let mut sim = fixture();
        let captor = sim.state.ruler;
        let target = sim.state.people[3].id;
        let before = sim.state.clone();
        assert_eq!(sim.start_restraint(captor, target), Err(Error::Unavailable));
        assert_eq!(sim.state, before);
        let source = sim.force_binding(captor).unwrap();
        let opening = sim.state.inventories[&source].quantity;
        capture_by_yield(&mut sim, captor, target);
        let captive = &sim.state.combat.as_ref().unwrap().captivity[&target];
        assert_eq!(sim.state.inventories[&source].quantity.0, opening.0 - 1_000);
        assert_eq!(sim.state.inventories[&captive.binding].quantity.0, 1_000);
        assert_eq!(captive.position, Point { x: 6_000, y: 8_000 });
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn yield_can_be_withdrawn_before_binding_and_departure_does_not_leave_a_capture_ticket() {
        let mut sim = fixture();
        let captor = sim.state.ruler;
        let target = sim.state.people[3].id;
        sim.force_yield(target, captor).unwrap();
        sim.start_restraint(captor, target).unwrap();
        sim.advance(SimDuration(3_000)).unwrap();
        sim.force_withdraw_yield(target).unwrap();
        sim.advance(SimDuration(3_000)).unwrap();
        assert!(
            !sim.state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&target)
        );
        assert_eq!(
            sim.state.inventories[&sim.force_binding(captor).unwrap()]
                .quantity
                .0,
            1_000
        );
        sim.force_yield(target, captor).unwrap();
        sim.debug_move_actor(target, AreaId(2)).unwrap();
        sim.advance(SimDuration(120_000)).unwrap();
        assert!(
            !sim.state
                .force
                .as_ref()
                .unwrap()
                .yielded
                .contains_key(&target)
        );
    }
    #[test]
    fn an_owned_reachable_tool_can_free_a_captive_but_area_stock_cannot() {
        let mut sim = fixture();
        let captor = sim.state.ruler;
        let target = sim.state.people[1].id;
        sim.carry_good(target, tool(&sim), Quantity(1_000)).unwrap();
        sim.advance(MINUTE).unwrap();
        capture_by_yield(&mut sim, captor, target);
        let binding = sim.state.combat.as_ref().unwrap().captivity[&target].binding;
        sim.start_escape(target).unwrap();
        sim.advance(SimDuration(5_999)).unwrap();
        assert!(
            sim.state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&target)
        );
        sim.advance(SimDuration(1)).unwrap();
        assert!(
            !sim.state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&target)
        );
        assert_eq!(sim.state.inventories[&binding].quantity.0, 1_000);
        let mut no_tool = fixture();
        let captor = no_tool.state.ruler;
        let target = no_tool.state.people[1].id;
        capture_by_yield(&mut no_tool, captor, target);
        assert_eq!(no_tool.start_escape(target), Err(Error::Unavailable));
    }
    #[test]
    fn guardian_regeneration_in_place_does_not_release_real_bindings() {
        let mut sim = fixture();
        let captor = sim.state.ruler;
        let guardian = sim.state.people[1].id;
        let max = sim.state.physical.as_ref().unwrap().rules.max_health;
        sim.debug_injure(guardian, max).unwrap();
        sim.start_restraint(captor, guardian).unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
        let confinement = sim.state.combat.as_ref().unwrap().captivity[&guardian].clone();
        sim.advance(SimDuration(180_000)).unwrap();
        assert_eq!(
            sim.state.components.body(guardian).unwrap().vitals.health,
            max
        );
        assert_eq!(
            sim.state.combat.as_ref().unwrap().captivity[&guardian],
            confinement
        );
        assert_eq!(sim.start_escape(guardian), Err(Error::Unavailable));
    }
    #[test]
    fn earlier_real_combat_injury_can_prevent_the_captives_escape() {
        let mut sim = fixture();
        let captive = sim.state.ruler;
        let captor = sim.state.people[3].id;
        sim.carry_good(captive, tool(&sim), Quantity(1_000))
            .unwrap();
        sim.advance(MINUTE).unwrap();
        capture_by_yield(&mut sim, captor, captive);
        let mut hostile = decisions::Personality::industrious();
        hostile.aggression = 900;
        hostile.risk_tolerance = 900;
        sim.debug_set_personality(captor, hostile, decisions::Aspiration::Comfort)
            .unwrap();
        sim.state
            .combat
            .as_mut()
            .unwrap()
            .profiles
            .get_mut(&captor)
            .unwrap()
            .initiative = 300;
        sim.state.combat.as_mut().unwrap().rules.damage = 170_000;
        sim.start_escape(captive).unwrap();
        assert!(
            sim.state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&captive)
        );
        assert!(
            sim.state.components.body(captive).unwrap().vitals.health
                <= sim.state.combat.as_ref().unwrap().rules.incapacitation
        );
        sim.advance(SimDuration(6_000)).unwrap();
        assert!(
            sim.state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&captive)
        );
    }
    #[test]
    fn escape_intervals_are_equivalent_after_partial_time_save_and_worker_changes() {
        let mut a = fixture();
        let captor = a.state.ruler;
        let target = a.state.people[1].id;
        a.carry_good(target, tool(&a), Quantity(1_000)).unwrap();
        a.advance(MINUTE).unwrap();
        capture_by_yield(&mut a, captor, target);
        a.start_escape(target).unwrap();
        let mut b = Simulation::load(
            &a.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        a.advance(SimDuration(6_000)).unwrap();
        b.advance(SimDuration(1_733)).unwrap();
        for _ in 0..30 {
            b.observe();
        }
        b = Simulation::load(
            &b.save().unwrap(),
            Execution {
                workers: 2,
                reverse_jobs: true,
            },
        )
        .unwrap();
        b.advance(SimDuration(4_267)).unwrap();
        assert_eq!(a.state, b.state);
    }
    #[test]
    fn closing_health_can_prevent_binding_or_an_escape_at_its_deadline() {
        let mut sim = fixture();
        let captor = sim.state.ruler;
        let guardian = sim.state.people[1].id;
        let max = sim.state.physical.as_ref().unwrap().rules.max_health;
        sim.debug_injure(guardian, max - 44_000).unwrap();
        sim.start_restraint(captor, guardian).unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
        assert_eq!(
            sim.state.components.body(guardian).unwrap().vitals.health,
            50_000
        );
        assert!(
            !sim.state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&guardian)
        );
        let mut weak = fixture();
        let captor = weak.state.ruler;
        let target = weak.state.people[3].id;
        weak.carry_good(target, tool(&weak), Quantity(1_000))
            .unwrap();
        weak.advance(MINUTE).unwrap();
        capture_by_yield(&mut weak, captor, target);
        let body = weak.state.components.body_mut(target).unwrap();
        body.vitals.health = 50_000;
        body.vitals.hunger = 700_000;
        weak.start_escape(target).unwrap();
        weak.advance(SimDuration(6_000)).unwrap();
        assert_eq!(
            weak.state.components.body(target).unwrap().vitals.health,
            44_000
        );
        assert!(
            weak.state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&target)
        );
    }
    #[test]
    fn loyal_guardian_fetches_a_tool_and_rescues_through_real_initiative_resistance() {
        let mut sim = fixture();
        let captive = sim.state.ruler;
        let captor = sim.state.people[3].id;
        capture_by_yield(&mut sim, captor, captive);
        let mut hostile = decisions::Personality::industrious();
        hostile.aggression = 900;
        hostile.risk_tolerance = 900;
        sim.debug_set_personality(captor, hostile, decisions::Aspiration::Comfort)
            .unwrap();
        sim.advance(SimDuration(114_000)).unwrap();
        assert!(sim.in_combat(captive));
        let encounter = sim.observe().combat.unwrap().encounter;
        sim.submit(Submission {
            caller_sequence: 10,
            command: Command::CombatTurn {
                encounter,
                movement: None,
                action: CombatAction::Defend,
            },
        })
        .unwrap();
        assert!(
            !sim.state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&captive)
        );
        assert!(
            sim.state
                .events
                .iter()
                .any(|event| matches!(event.kind,EventKind::Struck {actor,..} if actor==captor))
        );
        assert!(sim.state.events.iter().any(|event|matches!(event.kind,EventKind::Released {actor,target} if actor!=captive&&target==captive)));
    }
}

#[cfg(test)]
mod transport_force_tests {
    use super::*;

    fn carried_captive(npc_captor: bool) -> (Simulation, PersonId, PersonId) {
        let mut sim = Simulation::transport_fixture(91, Execution::default()).unwrap();
        let people: Vec<_> = sim.state.people.iter().map(|person| person.id).collect();
        for person in people {
            sim.debug_actor_command(person, Command::Cancel).unwrap();
            sim.debug_set_personality(
                person,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )
            .unwrap();
        }
        let ruler = sim.state.ruler;
        let companion = sim.state.people[3].id;
        let (captor, captive) = if npc_captor {
            (companion, ruler)
        } else {
            (ruler, companion)
        };
        sim.debug_add_inventory(
            Good::Tool,
            InventoryLocation::Carried(captive),
            Quantity(1_000),
            Quantity(1_000),
            BTreeSet::from([captive]),
        )
        .unwrap();
        sim.force_yield(captive, captor).unwrap();
        sim.start_restraint(captor, captive).unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
        sim.transport_command(captor, &TransportAction::OfferCarry { passenger: captive })
            .unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
        assert_eq!(
            sim.state.transport.as_ref().unwrap().groups[&captive].authority,
            game_transport::Authority::Captive
        );
        sim.debug_move_actor(captor, AreaId(2)).unwrap();
        sim.advance(SimDuration(30_000)).unwrap();
        (sim, captor, captive)
    }

    #[test]
    fn a_carried_captive_keeps_actual_bindings_and_can_escape_with_a_reachable_tool() {
        let (mut sim, captor, captive) = carried_captive(false);
        let binding = sim.state.combat.as_ref().unwrap().captivity[&captive].binding;
        assert_eq!(
            sim.state.inventories[&binding].location,
            InventoryLocation::Carried(captive)
        );
        assert!(matches!(
            sim.force_view(captor).confinements[0].location,
            Location::InTransit { .. }
        ));
        let mut restored = Simulation::load(
            &sim.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        for world in [&mut sim, &mut restored] {
            world.start_escape(captive).unwrap();
            world.advance(SimDuration(6_000)).unwrap();
            assert!(
                !world
                    .state
                    .combat
                    .as_ref()
                    .unwrap()
                    .captivity
                    .contains_key(&captive)
            );
            assert!(
                !world
                    .state
                    .transport
                    .as_ref()
                    .unwrap()
                    .groups
                    .contains_key(&captive)
            );
            assert!(matches!(
                world.resolved_location(captive),
                Location::StoppedOnRoad {
                    progress: SimDuration(36_000),
                    ..
                }
            ));
            assert_eq!(world.state.inventories[&binding].quantity, Quantity(1_000));
            Simulation::load(&world.save().unwrap(), Execution::default()).unwrap();
        }
        assert_eq!(sim.state, restored.state);
    }

    #[test]
    fn a_willing_bearer_intervenes_in_a_real_roadside_initiative_encounter() {
        let (mut sim, captor, captive) = carried_captive(true);
        {
            let personality = &mut sim
                .state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&captor)
                .unwrap()
                .personality;
            personality.risk_tolerance = 1_000;
            personality.aggression = 1_000;
        }
        sim.start_escape(captive).unwrap();
        assert!(sim.state.force.as_ref().unwrap().escapes[&captive].contested);
        let battle = sim
            .state
            .combat
            .as_ref()
            .unwrap()
            .encounters
            .values()
            .find(|battle| battle.active)
            .unwrap();
        assert!(battle.fighters.contains_key(&captor));
        assert!(battle.fighters.contains_key(&captive));
        assert_eq!(
            battle.location,
            Location::StoppedOnRoad {
                from: AreaId(1),
                to: AreaId(2),
                progress: SimDuration(30_000),
                duration: SimDuration(120_000)
            }
        );
        assert!(
            sim.state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&captive)
        );
        assert!(matches!(
            sim.resolved_location(captor),
            Location::StoppedOnRoad { .. }
        ));
        let restored = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        assert_eq!(sim.state, restored.state);
    }
}
