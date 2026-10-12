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
        sim.host.state.scenario = ScenarioInfo {
            id: "force-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        sim.host.state.force = Some(World {
            yielded: BTreeMap::new(),
            rules: Rules::default(),
            escapes: BTreeMap::new(),
            restraints: BTreeMap::new(),
            last_event: sim.host.next_event() - 1,
        });
        Ok(sim)
    }
    fn ensure_force_world(&mut self) {
        if self.host.state.force.is_none() {
            self.host.state.force = Some(World {
                yielded: BTreeMap::new(),
                rules: Rules::default(),
                escapes: BTreeMap::new(),
                restraints: BTreeMap::new(),
                last_event: self.host.next_event() - 1,
            });
        }
    }
    pub(crate) fn force_yield(&mut self, actor: PersonId, captor: PersonId) -> Result<(), Error> {
        let owner_now = self.host.clock().time;
        self.host.preflight_records(0, 0, 1)?;
        if actor == captor
            || !self.independent(actor)
            || self.in_combat(actor)
            || self.force_area(actor).is_none()
            || !self.transport_same_place(actor, captor)
            || game_physiology::body(&self.host.storage, actor)
                .unwrap()
                .vitals
                .health
                == 0
            || self.host.state.combat.is_none()
        {
            return Err(Error::Unavailable);
        }
        self.ensure_event_capacity(1)?;
        let area = self.force_area(actor).unwrap();
        self.ensure_force_world();
        self.host.state.force.as_mut().unwrap().yielded.insert(
            actor,
            Yielded {
                captors: BTreeSet::from([captor]),
                area,
                at: owner_now,
            },
        );
        self.emit(EventKind::Yielded { actor, captor });
        self.host.bump_revision(1)?;
        Ok(())
    }
    pub(crate) fn force_withdraw_yield(&mut self, actor: PersonId) -> Result<(), Error> {
        self.host.preflight_records(0, 0, 1)?;
        if self.force_area(actor).is_none()
            || self.in_combat(actor)
            || !self.independent(actor)
            || !self
                .host
                .state
                .force
                .as_ref()
                .is_some_and(|world| world.yielded.contains_key(&actor))
        {
            return Err(Error::Unavailable);
        }
        self.ensure_event_capacity(1)?;
        self.host
            .state
            .force
            .as_mut()
            .unwrap()
            .yielded
            .remove(&actor);
        self.emit(EventKind::YieldWithdrawn { actor });
        self.host.bump_revision(1)?;
        Ok(())
    }
    pub(crate) fn force_note_surrender(&mut self, encounter: EncounterId, actor: PersonId) {
        let owner_now = self.host.clock().time;
        let battle = &self.host.state.combat.as_ref().unwrap().encounters[&encounter];
        let captors = battle
            .fighters
            .values()
            .filter(|fighter| fighter.team != battle.fighters[&actor].team && !fighter.withdrawn)
            .map(|fighter| fighter.actor)
            .collect::<BTreeSet<_>>();
        let area = battle.area;
        self.ensure_force_world();
        self.host.state.force.as_mut().unwrap().yielded.insert(
            actor,
            Yielded {
                captors,
                area,
                at: owner_now,
            },
        );
    }
    pub(crate) fn force_area(&self, actor: PersonId) -> Option<AreaId> {
        let body = game_physiology::body(&self.host.storage, actor)?;
        if !body.vitals.alive {
            return None;
        }
        match self.resolved_location(actor) {
            Location::AtArea(area) | Location::StoppedOnRoad { from: area, .. } => Some(area),
            Location::InTransit { from, .. }
                if matches!(
                    *game_spatial::location(
                        &self.host.storage,
                        game_ecs::StableId(
                            (self.host.state.people[*self.person_index.get(&actor)?].id).0
                        )
                    )
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
            && self.host.state.combat.as_ref().is_some_and(|world| {
                game_physiology::body(&self.host.storage, actor)
                    .unwrap()
                    .vitals
                    .health
                    > world.rules.incapacitation
            })
    }
    pub(crate) fn force_tool(&self, actor: PersonId) -> Option<InventoryId> {
        self.inventory_index
            .get(&InventoryLocation::Carried(actor))?
            .iter()
            .copied()
            .find(|id| {
                game_cargo::inventory(&self.host.storage, *(id))
                    .expect("inventory exists")
                    .good
                    == Good::Tool
                    && game_cargo::inventory(&self.host.storage, *(id))
                        .expect("inventory exists")
                        .quantity
                        .0
                        >= 1_000
            })
    }
    fn force_binding(&self, actor: PersonId) -> Option<InventoryId> {
        self.inventory_index
            .get(&InventoryLocation::Carried(actor))?
            .iter()
            .copied()
            .find(|id| {
                game_cargo::inventory(&self.host.storage, *(id))
                    .expect("inventory exists")
                    .good
                    == Good::Binding
                    && game_cargo::inventory(&self.host.storage, *(id))
                        .expect("inventory exists")
                        .quantity
                        .0
                        >= 1_000
            })
    }
    fn force_restrainable(&self, actor: PersonId, target: PersonId) -> bool {
        self.force_area(target).is_some()
            && self.host.state.combat.as_ref().is_some_and(|world| {
                !world.captivity.contains_key(&target)
                    && (game_physiology::body(&self.host.storage, target)
                        .unwrap()
                        .vitals
                        .health
                        <= world.rules.incapacitation
                        || self
                            .host
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
        self.host
            .state
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
        self.host.preflight_records(0, 0, 1)?;
        let rules = self
            .host
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
            self.host
                .clock()
                .time
                .0
                .checked_add(rules.restraint_time.0)
                .ok_or(Error::Overflow)?,
        );
        let area = self.force_area(target).unwrap();
        let location = self.resolved_location(actor);
        let position = self.force_position(target, area);
        self.host.next_id().checked_add(2).ok_or(Error::Overflow)?;
        self.ensure_event_capacity(3)?;
        self.ensure_force_world();
        self.interrupt_actor_activity(actor);
        self.host.state.force.as_mut().unwrap().restraints.insert(
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
        *self.host.storage.activity_mut(actor).unwrap() = Activity::Restrain {
            target,
            binding,
            completes,
        };
        self.host
            .state
            .due
            .entry(completes)
            .or_default()
            .push(Due::Physical(actor));
        self.host.bump_revision(1)?;
        Ok(())
    }
    pub(crate) fn start_escape(&mut self, actor: PersonId) -> Result<(), Error> {
        let owner_now = self.host.clock().time;
        self.host.preflight_records(0, 0, 1)?;
        let rules = self
            .host
            .state
            .force
            .as_ref()
            .map(|world| world.rules.clone())
            .unwrap_or_default();
        if !self.force_body_ready(actor)
            || self.in_combat(actor)
            || !self
                .host
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
            owner_now
                .0
                .checked_add(rules.escape_time.0)
                .ok_or(Error::Overflow)?,
        );
        self.preflight_force_intervention()?;
        self.ensure_event_capacity(128)?;
        self.ensure_force_world();
        self.interrupt_actor_activity(actor);
        self.host.state.force.as_mut().unwrap().escapes.insert(
            actor,
            Escape {
                target: actor,
                actor,
                tool: Some(tool),
                began: owner_now,
                completes,
                contested: false,
            },
        );
        *self.host.storage.activity_mut(actor).unwrap() = Activity::Escape { tool, completes };
        self.host
            .state
            .due
            .entry(completes)
            .or_default()
            .push(Due::Physical(actor));
        self.emit(EventKind::EscapeAttempted { actor });
        self.force_intervention(actor)?;
        self.host.bump_revision(1)?;
        Ok(())
    }
    pub(crate) fn preflight_force_intervention(&self) -> Result<(), Error> {
        self.host.next_id().checked_add(2).ok_or(Error::Overflow)?;
        if let Some(combat) = &self.host.state.combat {
            self.host
                .clock()
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
        let owner_now = self.host.clock().time;
        self.ensure_force_world();
        let tool = self.force_tool(actor);
        self.host.state.force.as_mut().unwrap().escapes.insert(
            target,
            Escape {
                target,
                actor,
                tool,
                began: owner_now,
                completes,
                contested: false,
            },
        );
        self.force_intervention(target)
    }
    fn force_intervention(&mut self, target: PersonId) -> Result<(), Error> {
        let captivity = self
            .host
            .state
            .combat
            .as_ref()
            .unwrap()
            .captivity
            .get(&target)
            .cloned()
            .ok_or(Error::Unavailable)?;
        let attempt = self.host.state.force.as_ref().unwrap().escapes[&target].clone();
        // The player chooses their own intervention. NPC captors and trusted
        // nearby allies choose whether they are willing to defend confinement.
        if attempt.actor == captivity.captor {
            return Ok(());
        }
        let opponents = self
            .host
            .state
            .people
            .iter()
            .filter(|person| {
                person.id != self.host.state.ruler
                    && person.id != attempt.actor
                    && person.id != target
                    && self.transport_same_place(person.id, target)
                    && self.independent(person.id)
                    && game_physiology::body(&self.host.storage, person.id)
                        .unwrap()
                        .vitals
                        .alive
                    && game_physiology::body(&self.host.storage, person.id)
                        .unwrap()
                        .vitals
                        .health
                        > self
                            .host
                            .state
                            .combat
                            .as_ref()
                            .unwrap()
                            .rules
                            .incapacitation
                    && !self.combat_restricts(person.id)
            })
            .filter(|person| {
                let mind = &game_characters::decisions::mind::<game_api::Command>(
                    &self.host.storage,
                    person.id,
                )
                .unwrap();
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
        self.host
            .state
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
                .host
                .state
                .combat
                .as_ref()
                .is_some_and(|world| world.captivity.contains_key(&actor))
    }
    pub(crate) fn force_can_combat_act(&self, actor: PersonId) -> bool {
        self.force_body_ready(actor)
            && self.host.state.force.as_ref().is_some_and(|world| {
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
            .host
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
        let Some(world) = &mut self.host.state.force else {
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
        let combat = self.host.state.combat.as_ref().ok_or(Error::Unavailable)?;
        self.host
            .state
            .physical
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let actor = game_physiology::body(&self.host.storage, captor).ok_or(Error::Unavailable)?;
        let victim = game_physiology::body(&self.host.storage, target).ok_or(Error::Unavailable)?;
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
                let key = transport::location_position(location, self.host.clock().time)
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
        self.host
            .next_id()
            .checked_add(allocation_count)
            .ok_or(Error::Overflow)?;
        let expected_binding = self
            .host
            .next_id()
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
        let binding = InventoryId(self.host.allocate_id().expect("preflight confinement ID").0);
        debug_assert_eq!(binding, receipt.binding);
        let receipt = validated.apply(
            &mut self.host.storage,
            &mut self.host.state.combat.as_mut().unwrap().captivity,
        )?;
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
        let activity = self.host.storage.activity(actor).unwrap().clone();
        match activity {
            Activity::Escape { tool, .. } => {
                let successful = self.force_body_ready(actor)
                    && game_cargo::inventory(&self.host.storage, tool).is_some_and(|stock| {
                        stock.good == Good::Tool
                            && stock.location == InventoryLocation::Carried(actor)
                            && stock.quantity.0 >= 1_000
                    })
                    && self
                        .host
                        .state
                        .combat
                        .as_ref()
                        .unwrap()
                        .captivity
                        .contains_key(&actor);
                self.host
                    .state
                    .force
                    .as_mut()
                    .unwrap()
                    .escapes
                    .remove(&actor);
                if successful {
                    self.release_captive(actor, actor);
                } else {
                    self.emit(EventKind::EscapeInterrupted { actor });
                }
            }
            Activity::Restrain {
                target, binding, ..
            } => {
                let Some(attempt) = self
                    .host
                    .state
                    .force
                    .as_mut()
                    .unwrap()
                    .restraints
                    .remove(&actor)
                else {
                    return;
                };
                let result = (|| -> Result<(), Error> {
                    let (context, scene) =
                        self.restraint_context(&attempt.location, attempt.position)?;
                    let new_map = scene.is_new();
                    let applied = InventoryId(
                        self.host
                            .next_id()
                            .checked_add(u64::from(new_map))
                            .ok_or(Error::Overflow)?,
                    );
                    let surrendered = self
                        .host
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
                    let source = game_cargo::inventory(&self.host.storage, binding)
                        .ok_or(Error::Unavailable)?;
                    let plan = game_conflict::prepare_confinement(
                        &self.host.state.combat.as_ref().unwrap().rules,
                        facts.clone(),
                        source,
                        applied,
                        context,
                        game_conflict::ConfinementMode::Timed {
                            completes: attempt.completes,
                            admitted_location: attempt.location.clone(),
                        },
                        self.host.clock().time,
                    )?;
                    let validated = plan.begin(
                        &self.host.storage,
                        &self.host.state.combat.as_ref().unwrap().captivity,
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
        *self.host.storage.activity_mut(actor).unwrap() = Activity::Idle;
    }
    pub(crate) fn force_guardian_rescue(
        &self,
        guardian: PersonId,
        target: PersonId,
    ) -> Option<Command> {
        self.host.state.combat.as_ref()?.captivity.get(&target)?;
        if !self.transport_same_place(guardian, target)
            || !self.force_body_ready(guardian)
            || self.combat_restricts(guardian)
        {
            return None;
        }
        if self.force_tool(guardian).is_some() {
            return Some(Command::FreeCaptive { target });
        }
        let _mind =
            game_characters::decisions::mind::<game_api::Command>(&self.host.storage, guardian)?;
        let stocks = game_characters::decisions::stocks(&self.host.storage, guardian)
            .expect("stock knowledge exists");
        let tool = stocks.known.values().find(|fact| {
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
                *self.host.storage.activity(actor).unwrap(),
                Activity::Idle | Activity::Rest
            )
            || self.in_combat(actor)
            || !self
                .host
                .state
                .combat
                .as_ref()?
                .captivity
                .contains_key(&actor)
            || self.force_tool(actor).is_none()
        {
            return None;
        }
        Some(Command::Escape)
    }
    pub(crate) fn force_allocation_bound(&self, due: &[Due]) -> u64 {
        let Some(world) = &self.host.state.force else {
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
        if self.host.state.combat.is_none() {
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
            .host
            .state
            .force
            .as_ref()
            .is_some_and(|world| world.yielded.contains_key(&actor))
        {
            commands.push(Command::WithdrawYield);
        }
        for target in self
            .host
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
        let Some(world) = &self.host.state.force else {
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
        self.host
            .next_id()
            .checked_add(count)
            .ok_or(Error::Overflow)?;
        Ok(())
    }
    pub(crate) fn update_force(&mut self) {
        let Some(world) = &self.host.state.force else {
            return;
        };
        let released = world
            .escapes
            .keys()
            .filter(|actor| {
                !self
                    .host
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
            self.host
                .state
                .force
                .as_mut()
                .unwrap()
                .escapes
                .remove(&actor);
        }
        let finished = self
            .host
            .state
            .force
            .as_ref()
            .unwrap()
            .escapes
            .iter()
            .filter(|(target, attempt)| {
                attempt.contested
                    && !self
                        .host
                        .state
                        .combat
                        .as_ref()
                        .unwrap()
                        .encounters
                        .values()
                        .any(|encounter| {
                            encounter.active
                                && encounter.fighters.contains_key(target)
                                && encounter.fighters.contains_key(&attempt.actor)
                        })
            })
            .map(|(target, _)| *target)
            .collect::<Vec<_>>();
        for target in finished {
            self.host
                .state
                .force
                .as_mut()
                .unwrap()
                .escapes
                .remove(&target);
        }
        let cursor = self.host.state.force.as_ref().unwrap().last_event;
        let events = self
            .host
            .events()
            .iter()
            .filter(|event| event.sequence > cursor)
            .cloned()
            .collect::<Vec<_>>();
        for event in events {
            match event.event {
                EventKind::Departed { actor, .. } | EventKind::Struck { actor, .. } => {
                    let yielded = &mut self.host.state.force.as_mut().unwrap().yielded;
                    yielded.remove(&actor);
                    for declaration in yielded.values_mut() {
                        declaration.captors.remove(&actor);
                    }
                    yielded.retain(|_, declaration| !declaration.captors.is_empty());
                }
                _ => {}
            }
        }
        self.host.state.force.as_mut().unwrap().last_event = self.host.next_event() - 1;
    }
    pub(crate) fn force_rescue_target(&self, actor: PersonId) -> Option<PersonId> {
        self.host
            .state
            .force
            .as_ref()?
            .escapes
            .values()
            .find(|attempt| {
                attempt.actor == actor
                    && attempt.target != actor
                    && self
                        .host
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
        let battle = self
            .host
            .state
            .combat
            .as_ref()?
            .encounters
            .get(&encounter)?;
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
        let map = &self.host.state.combat.as_ref()?.maps[&battle.area];
        let dx = i128::from(mine.position.x) - i128::from(captive.position.x);
        let dy = i128::from(mine.position.y) - i128::from(captive.position.y);
        if self.force_tool(actor).is_some()
            && (dx * dx + dy * dy) as u128
                <= u128::from(self.host.state.combat.as_ref()?.rules.reach).pow(2)
            && map.clear(mine.position, captive.position)
        {
            Some(CombatAction::Release { target })
        } else {
            None
        }
    }
    fn force_visible(&self, observer: PersonId, target: PersonId) -> bool {
        let Some(combat) = &self.host.state.combat else {
            return false;
        };
        if !combat.captivity.contains_key(&target) {
            return false;
        }
        self.transport_same_place(observer, target)
    }
    pub(crate) fn force_view(&self, actor: PersonId) -> ForceView {
        if self.host.state.physical.is_none()
            || game_physiology::body(&self.host.storage, actor)
                .is_none_or(|body| !body.vitals.alive)
        {
            return ForceView::default();
        }
        let Some(combat) = &self.host.state.combat else {
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
                        .host
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
    let Some(world) = &state.state.force else {
        return Ok(());
    };
    let fail = || Error::InvalidSave("invalid confinement attempt or food supply".into());
    state.state.physical.as_ref().ok_or_else(fail)?;
    let combat = state.state.combat.as_ref().ok_or_else(fail)?;
    let people = state
        .state
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
            || !state.state.areas.iter().any(|area| area.id == yielded.area)
        {
            return Err(fail());
        }
    }
    let locations = transport::resolved_locations_for(state)?;
    for (actor, attempt) in &world.restraints {
        let body = game_physiology::body(state.components, *actor).ok_or_else(fail)?;
        if !people.contains_key(actor)
            || !people.contains_key(&attempt.target)
            || actor == &attempt.target
            || attempt.completes <= state.time
            || !body.vitals.alive
            || locations[actor] != attempt.location
            || locations[&attempt.target] != attempt.location
            || game_cargo::inventory(state.components, attempt.binding).is_none_or(|stock| {
                stock.good != Good::Binding || stock.location != InventoryLocation::Carried(*actor)
            })
            || *state.components.activity(*actor).ok_or_else(fail)?
                != (Activity::Restrain {
                    target: attempt.target,
                    binding: attempt.binding,
                    completes: attempt.completes,
                })
            || !state
                .state
                .due
                .get(&attempt.completes)
                .is_some_and(|events| {
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
            && game_cargo::inventory(state.components, tool).is_none_or(|stock| {
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
            .host
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
        game_cargo::inventories(&sim.host.storage)
            .map(|(_, row)| row)
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
            sim.host
                .state
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
        let captor = sim.host.state.ruler;
        let target = sim.host.state.people[3].id;
        let before = sim.canonical_snapshot().unwrap();
        assert_eq!(sim.start_restraint(captor, target), Err(Error::Unavailable));
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
        let source = sim.force_binding(captor).unwrap();
        let opening = game_cargo::inventory(&sim.host.storage, source)
            .expect("inventory exists")
            .quantity;
        capture_by_yield(&mut sim, captor, target);
        let captive = &sim.host.state.combat.as_ref().unwrap().captivity[&target];
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, source)
                .expect("inventory exists")
                .quantity
                .0,
            opening.0 - 1_000
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, captive.binding)
                .expect("inventory exists")
                .quantity
                .0,
            1_000
        );
        assert_eq!(captive.position, Point { x: 6_000, y: 8_000 });
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn yield_can_be_withdrawn_before_binding_and_departure_does_not_leave_a_capture_ticket() {
        let mut sim = fixture();
        let captor = sim.host.state.ruler;
        let target = sim.host.state.people[3].id;
        sim.force_yield(target, captor).unwrap();
        sim.start_restraint(captor, target).unwrap();
        sim.advance(SimDuration(3_000)).unwrap();
        sim.force_withdraw_yield(target).unwrap();
        sim.advance(SimDuration(3_000)).unwrap();
        assert!(
            !sim.host
                .state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&target)
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, sim.force_binding(captor).unwrap())
                .expect("inventory exists")
                .quantity
                .0,
            1_000
        );
        sim.force_yield(target, captor).unwrap();
        sim.debug_move_actor(target, AreaId(2)).unwrap();
        sim.advance(SimDuration(120_000)).unwrap();
        assert!(
            !sim.host
                .state
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
        let captor = sim.host.state.ruler;
        let target = sim.host.state.people[1].id;
        sim.carry_good(target, tool(&sim), Quantity(1_000)).unwrap();
        sim.advance(MINUTE).unwrap();
        capture_by_yield(&mut sim, captor, target);
        let binding = sim.host.state.combat.as_ref().unwrap().captivity[&target].binding;
        sim.start_escape(target).unwrap();
        sim.advance(SimDuration(5_999)).unwrap();
        assert!(
            sim.host
                .state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&target)
        );
        sim.advance(SimDuration(1)).unwrap();
        assert!(
            !sim.host
                .state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&target)
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, binding)
                .expect("inventory exists")
                .quantity
                .0,
            1_000
        );
        let mut no_tool = fixture();
        let captor = no_tool.host.state.ruler;
        let target = no_tool.host.state.people[1].id;
        capture_by_yield(&mut no_tool, captor, target);
        assert_eq!(no_tool.start_escape(target), Err(Error::Unavailable));
    }
    #[test]
    fn guardian_regeneration_in_place_does_not_release_real_bindings() {
        let mut sim = fixture();
        let captor = sim.host.state.ruler;
        let guardian = sim.host.state.people[1].id;
        let max = sim.host.state.physical.as_ref().unwrap().rules.max_health;
        sim.debug_injure(guardian, max).unwrap();
        sim.start_restraint(captor, guardian).unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
        let confinement = sim.host.state.combat.as_ref().unwrap().captivity[&guardian].clone();
        sim.advance(SimDuration(180_000)).unwrap();
        assert_eq!(
            game_physiology::body(&sim.host.storage, guardian)
                .unwrap()
                .vitals
                .health,
            max
        );
        assert_eq!(
            sim.host.state.combat.as_ref().unwrap().captivity[&guardian],
            confinement
        );
        assert_eq!(sim.start_escape(guardian), Err(Error::Unavailable));
    }
    #[test]
    fn earlier_real_combat_injury_can_prevent_the_captives_escape() {
        let mut sim = fixture();
        let captive = sim.host.state.ruler;
        let captor = sim.host.state.people[3].id;
        sim.carry_good(captive, tool(&sim), Quantity(1_000))
            .unwrap();
        sim.advance(MINUTE).unwrap();
        capture_by_yield(&mut sim, captor, captive);
        let mut hostile = decisions::Personality::industrious();
        hostile.aggression = 900;
        hostile.risk_tolerance = 900;
        sim.debug_set_personality(captor, hostile, decisions::Aspiration::Comfort)
            .unwrap();
        sim.host
            .state
            .combat
            .as_mut()
            .unwrap()
            .profiles
            .get_mut(&captor)
            .unwrap()
            .initiative = 300;
        sim.host.state.combat.as_mut().unwrap().rules.damage = 170_000;
        sim.start_escape(captive).unwrap();
        assert!(
            sim.host
                .state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&captive)
        );
        assert!(
            game_physiology::body(&sim.host.storage, captive)
                .unwrap()
                .vitals
                .health
                <= sim.host.state.combat.as_ref().unwrap().rules.incapacitation
        );
        sim.advance(SimDuration(6_000)).unwrap();
        assert!(
            sim.host
                .state
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
        let captor = a.host.state.ruler;
        let target = a.host.state.people[1].id;
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
        assert_eq!(a.canonical_state(), b.canonical_state());
    }
    #[test]
    fn closing_health_can_prevent_binding_or_an_escape_at_its_deadline() {
        let mut sim = fixture();
        let captor = sim.host.state.ruler;
        let guardian = sim.host.state.people[1].id;
        let max = sim.host.state.physical.as_ref().unwrap().rules.max_health;
        sim.debug_injure(guardian, max - 44_000).unwrap();
        sim.start_restraint(captor, guardian).unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
        assert_eq!(
            game_physiology::body(&sim.host.storage, guardian)
                .unwrap()
                .vitals
                .health,
            50_000
        );
        assert!(
            !sim.host
                .state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&guardian)
        );
        let mut weak = fixture();
        let captor = weak.host.state.ruler;
        let target = weak.host.state.people[3].id;
        weak.carry_good(target, tool(&weak), Quantity(1_000))
            .unwrap();
        weak.advance(MINUTE).unwrap();
        capture_by_yield(&mut weak, captor, target);
        let body = game_physiology::body_mut(&mut weak.host.storage, target).unwrap();
        body.vitals.health = 50_000;
        body.vitals.hunger = 700_000;
        weak.start_escape(target).unwrap();
        weak.advance(SimDuration(6_000)).unwrap();
        assert_eq!(
            game_physiology::body(&weak.host.storage, target)
                .unwrap()
                .vitals
                .health,
            44_000
        );
        assert!(
            weak.host
                .state
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
        let captive = sim.host.state.ruler;
        let captor = sim.host.state.people[3].id;
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
            !sim.host
                .state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&captive)
        );
        assert!(
            sim.host
                .events()
                .iter()
                .any(|event| matches!(event.event,EventKind::Struck {actor,..} if actor==captor))
        );
        assert!(sim.host.events().iter().any(|event|matches!(event.event,EventKind::Released {actor,target} if actor!=captive&&target==captive)));
    }
}

#[cfg(test)]
mod transport_force_tests {
    use super::*;

    fn carried_captive(npc_captor: bool) -> (Simulation, PersonId, PersonId) {
        let mut sim = Simulation::transport_fixture(91, Execution::default()).unwrap();
        let people: Vec<_> = sim
            .host
            .state
            .people
            .iter()
            .map(|person| person.id)
            .collect();
        for person in people {
            sim.debug_actor_command(person, Command::Cancel).unwrap();
            sim.debug_set_personality(
                person,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )
            .unwrap();
        }
        let ruler = sim.host.state.ruler;
        let companion = sim.host.state.people[3].id;
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
            game_transport::group(&sim.host.storage, captive)
                .unwrap()
                .authority,
            game_transport::Authority::Captive
        );
        sim.debug_move_actor(captor, AreaId(2)).unwrap();
        sim.advance(SimDuration(30_000)).unwrap();
        (sim, captor, captive)
    }

    #[test]
    fn a_carried_captive_keeps_actual_bindings_and_can_escape_with_a_reachable_tool() {
        let (mut sim, captor, captive) = carried_captive(false);
        let binding = sim.host.state.combat.as_ref().unwrap().captivity[&captive].binding;
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, binding)
                .expect("inventory exists")
                .location,
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
                    .host
                    .state
                    .combat
                    .as_ref()
                    .unwrap()
                    .captivity
                    .contains_key(&captive)
            );
            assert!(game_transport::group(&world.host.storage, captive).is_none());
            assert!(matches!(
                world.resolved_location(captive),
                Location::StoppedOnRoad {
                    progress: SimDuration(36_000),
                    ..
                }
            ));
            assert_eq!(
                game_cargo::inventory(&world.host.storage, binding)
                    .expect("inventory exists")
                    .quantity,
                Quantity(1_000)
            );
            Simulation::load(&world.save().unwrap(), Execution::default()).unwrap();
        }
        assert_eq!(sim.canonical_state(), restored.canonical_state());
    }

    #[test]
    fn a_willing_bearer_intervenes_in_a_real_roadside_initiative_encounter() {
        let (mut sim, captor, captive) = carried_captive(true);
        {
            let personality = &mut game_characters::decisions::mind_mut::<game_api::Command>(
                &mut sim.host.storage,
                captor,
            )
            .unwrap()
            .personality;
            personality.risk_tolerance = 1_000;
            personality.aggression = 1_000;
        }
        sim.start_escape(captive).unwrap();
        assert!(sim.host.state.force.as_ref().unwrap().escapes[&captive].contested);
        let battle = sim
            .host
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
            sim.host
                .state
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
        assert_eq!(sim.canonical_state(), restored.canonical_state());
    }
}
