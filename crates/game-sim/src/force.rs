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
    pub position: Point,
    pub completes: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoodSupply {
    pub requester: PersonId,
    pub source: InventoryId,
    pub quantity: Quantity,
    pub delivered: u64,
    pub accepted_at: SimTime,
    pub cargo: BTreeMap<InventoryId, u64>,
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
    pub food: BTreeMap<PersonId, FoodSupply>,
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
            food: BTreeMap::new(),
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
                food: BTreeMap::new(),
                last_event: self.state.next_event - 1,
            });
        }
    }
    pub(crate) fn force_yield(&mut self, actor: PersonId, captor: PersonId) -> Result<(), Error> {
        if actor == captor
            || !self.independent(actor)
            || self.in_combat(actor)
            || self.force_area(actor).is_none()
            || self.force_area(actor) != self.force_area(captor)
            || self.state.physical.as_ref().unwrap().actors[&actor]
                .vitals
                .health
                == 0
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
        let body = self.state.physical.as_ref()?.actors.get(&actor)?;
        if !body.vitals.alive {
            return None;
        }
        match self
            .state
            .people
            .get(*self.person_index.get(&actor)?)?
            .location
        {
            Location::AtArea(area) => Some(area),
            _ => None,
        }
    }
    fn force_body_ready(&self, actor: PersonId) -> bool {
        self.force_area(actor).is_some()
            && self.independent(actor)
            && self.state.combat.as_ref().is_some_and(|world| {
                self.state.physical.as_ref().unwrap().actors[&actor]
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
                    && (self.state.physical.as_ref().unwrap().actors[&target]
                        .vitals
                        .health
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
            || self.force_area(actor) != self.force_area(target)
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
        let position = self.force_position(target, area);
        self.state
            .next_id
            .checked_add(
                if self.state.combat.as_ref().unwrap().maps.contains_key(&area) {
                    1
                } else {
                    2
                },
            )
            .ok_or(Error::Overflow)?;
        self.ensure_event_capacity(3)?;
        self.ensure_force_world();
        self.interrupt_physical(actor);
        self.state.force.as_mut().unwrap().restraints.insert(
            actor,
            Restraint {
                target,
                binding,
                area,
                position,
                completes,
            },
        );
        self.state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .activity = Activity::Restrain {
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
        self.interrupt_physical(actor);
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
        self.state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .activity = Activity::Escape { tool, completes };
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
                    && self.force_area(person.id) == Some(captivity.area)
                    && self.force_body_ready(person.id)
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
    pub(crate) fn complete_force(&mut self, actor: PersonId) {
        let activity = self.state.physical.as_ref().unwrap().actors[&actor]
            .activity
            .clone();
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
                if self.force_body_ready(actor)
                    && self.force_restrainable(actor, target)
                    && self.force_area(actor) == Some(attempt.area)
                    && self.force_area(target) == Some(attempt.area)
                    && self.state.inventories.get(&binding).is_some_and(|stock| {
                        stock.location == InventoryLocation::Carried(actor)
                            && stock.good == Good::Binding
                            && stock.quantity.0 >= 1_000
                    })
                {
                    self.ensure_combat_map(attempt.area)
                        .expect("preflight restraint map");
                    self.state.inventories.get_mut(&binding).unwrap().quantity.0 -= 1_000;
                    let restraint = self
                        .add_inventory(
                            Good::Binding,
                            InventoryLocation::Area(attempt.area),
                            Quantity(1_000),
                            Quantity(1_000),
                            BTreeSet::from([actor]),
                        )
                        .expect("preflight restraint inventory");
                    self.interrupt_physical(target);
                    self.state.combat.as_mut().unwrap().captivity.insert(
                        target,
                        combat::Captivity {
                            actor: target,
                            captor: actor,
                            area: attempt.area,
                            position: attempt.position,
                            binding: restraint,
                        },
                    );
                    self.emit(EventKind::Transferred {
                        actor,
                        source: binding,
                        destination: restraint,
                        quantity: Quantity(1_000),
                    });
                    self.emit(EventKind::Restrained {
                        actor,
                        target,
                        binding: restraint,
                    });
                } else {
                    self.emit(EventKind::Interrupted { actor });
                }
            }
            _ => return,
        }
        self.state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .activity = Activity::Idle;
    }
    pub(crate) fn force_guardian_rescue(
        &self,
        guardian: PersonId,
        target: PersonId,
    ) -> Option<Command> {
        let captivity = self.state.combat.as_ref()?.captivity.get(&target)?;
        if self.force_area(guardian) != Some(captivity.area)
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
                && fact.area == captivity.area
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
                self.state.physical.as_ref().unwrap().actors[&actor].activity,
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
        let maps = attempts
            .iter()
            .filter(|attempt| {
                !self
                    .state
                    .combat
                    .as_ref()
                    .unwrap()
                    .maps
                    .contains_key(&attempt.area)
            })
            .map(|attempt| attempt.area)
            .collect::<BTreeSet<_>>();
        attempts.len() as u64 + maps.len() as u64
    }
    pub(crate) fn available_force_commands(&self, actor: PersonId) -> Vec<Command> {
        if self.state.combat.is_none() {
            return Vec::new();
        }
        let Some(area) = self.force_area(actor) else {
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
            .filter(|person| person.id != actor && self.force_area(person.id) == Some(area))
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
            .map(|attempt| {
                if self
                    .state
                    .combat
                    .as_ref()
                    .unwrap()
                    .maps
                    .contains_key(&attempt.area)
                {
                    1_u64
                } else {
                    2
                }
            })
            .sum::<u64>();
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
                EventKind::Transferred {
                    actor,
                    source,
                    destination,
                    quantity,
                } => {
                    if self.state.inventories[&source].good != Good::Food {
                        continue;
                    }
                    for (guardian, supply) in &mut self.state.force.as_mut().unwrap().food {
                        let staged = supply.cargo.remove(&source).unwrap_or(0);
                        let moved = staged.min(quantity.0);
                        if staged > moved {
                            supply.cargo.insert(source, staged - moved);
                        }
                        if *guardian != actor {
                            continue;
                        }
                        if self.state.inventories[&destination].location
                            == InventoryLocation::Carried(supply.requester)
                        {
                            supply.delivered = supply
                                .delivered
                                .saturating_add(moved)
                                .min(supply.quantity.0);
                        } else if self.state.inventories[&destination].location
                            == InventoryLocation::Carried(actor)
                        {
                            let recorded = supply.cargo.values().sum::<u64>();
                            let newly_loaded = if source == supply.source {
                                quantity.0.min(
                                    supply
                                        .quantity
                                        .0
                                        .saturating_sub(supply.delivered)
                                        .saturating_sub(recorded),
                                )
                            } else {
                                moved
                            };
                            if newly_loaded > 0 {
                                *supply.cargo.entry(destination).or_default() += newly_loaded;
                            }
                        }
                    }
                }
                EventKind::Consumed {
                    source, quantity, ..
                }
                | EventKind::Ate {
                    source, quantity, ..
                }
                | EventKind::Cared {
                    source, quantity, ..
                } => {
                    for supply in self.state.force.as_mut().unwrap().food.values_mut() {
                        if let Some(staged) = supply.cargo.get_mut(&source) {
                            *staged = staged.saturating_sub(quantity.0);
                            if *staged == 0 {
                                supply.cargo.remove(&source);
                            }
                        }
                    }
                }
                EventKind::Spoiled {
                    inventory,
                    quantity,
                } => {
                    for supply in self.state.force.as_mut().unwrap().food.values_mut() {
                        if let Some(staged) = supply.cargo.get_mut(&inventory) {
                            *staged = staged.saturating_sub(quantity.0);
                            if *staged == 0 {
                                supply.cargo.remove(&inventory);
                            }
                        }
                    }
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
    pub(crate) fn request_guardian_food(
        &mut self,
        guardian: PersonId,
        requester: PersonId,
        source: InventoryId,
        quantity: Quantity,
    ) -> Result<(), Error> {
        let lineage = self.state.lineage.as_ref().ok_or(Error::Unavailable)?;
        if lineage.current() != Some(requester)
            || !lineage.guardians.contains_key(&guardian)
            || !self.independent(requester)
            || self.in_combat(requester)
            || self.force_area(requester).is_none()
            || self.force_area(requester) != self.force_area(guardian)
            || quantity.0 == 0
            || !self
                .state
                .physical
                .as_ref()
                .unwrap()
                .actors
                .get(&requester)
                .is_some_and(|body| {
                    body.vitals.health > 0
                        && matches!(body.activity, Activity::Idle | Activity::Rest)
                })
        {
            return Err(Error::Unavailable);
        }
        self.ensure_event_capacity(8)?;
        let reason = self.guardian_food_refusal(guardian, requester, source, quantity);
        if let Some(reason) = reason {
            self.refuse_guardian_food(guardian, requester, reason);
            return Ok(());
        }
        self.state
            .time
            .0
            .checked_add(self.state.physical.as_ref().unwrap().rules.loading_time.0)
            .ok_or(Error::Overflow)?;
        self.state.next_id.checked_add(1).ok_or(Error::Overflow)?;
        let had_force = self.state.force.is_some();
        self.ensure_force_world();
        let already_carried =
            self.state.inventories[&source].location == InventoryLocation::Carried(guardian);
        let previous = self.state.force.as_mut().unwrap().food.insert(
            guardian,
            FoodSupply {
                requester,
                source,
                quantity,
                delivered: 0,
                accepted_at: self.state.time,
                cargo: if already_carried {
                    BTreeMap::from([(source, quantity.0)])
                } else {
                    BTreeMap::new()
                },
            },
        );
        let Some(command) = self.guardian_food_command(guardian) else {
            if let Some(previous) = previous {
                self.state
                    .force
                    .as_mut()
                    .unwrap()
                    .food
                    .insert(guardian, previous);
            } else {
                self.state.force.as_mut().unwrap().food.remove(&guardian);
            }
            if !had_force {
                self.state.force = None;
            }
            self.refuse_guardian_food(
                guardian,
                requester,
                "I cannot begin a safe delivery with the current carrying space",
            );
            return Ok(());
        };
        let accepted = match command {
            Command::Move { destination } => self.start_movement(guardian, destination),
            Command::Carry { source, quantity } => self.carry_good(guardian, source, quantity),
            command => self.debug_physical_command(guardian, command),
        };
        if let Err(error) = accepted {
            if let Some(previous) = previous {
                self.state
                    .force
                    .as_mut()
                    .unwrap()
                    .food
                    .insert(guardian, previous);
            } else {
                self.state.force.as_mut().unwrap().food.remove(&guardian);
            }
            if !had_force {
                self.state.force = None;
            }
            if error == Error::Overflow {
                return Err(error);
            }
            self.refuse_guardian_food(
                guardian,
                requester,
                "The actual stock, permission or carrying space prevents this delivery",
            );
            return Ok(());
        }
        let guard = self
            .state
            .lineage
            .as_mut()
            .unwrap()
            .guardians
            .get_mut(&guardian)
            .unwrap();
        guard.assignment = Some(lineage::Assignment {
            request: GuardianRequest::SupplyFood { source, quantity },
            accepted_at: self.state.time,
        });
        guard.explanation="I will bring provisions using my own access and actual carrying, without requiring an adult command relationship".into();
        self.emit(EventKind::GuardianAccepted {
            guardian,
            requester,
        });
        self.revision += 1;
        Ok(())
    }
    fn refuse_guardian_food(&mut self, guardian: PersonId, requester: PersonId, reason: &str) {
        self.state
            .lineage
            .as_mut()
            .unwrap()
            .guardians
            .get_mut(&guardian)
            .unwrap()
            .explanation = reason.into();
        self.emit(EventKind::GuardianRefused {
            guardian,
            requester,
            reason: reason.into(),
        });
        self.revision += 1;
    }
    fn guardian_food_refusal(
        &self,
        guardian: PersonId,
        requester: PersonId,
        source: InventoryId,
        quantity: Quantity,
    ) -> Option<&'static str> {
        if !self.force_body_ready(guardian) || self.combat_restricts(guardian) {
            return Some("My body or confinement prevents this assistance");
        }
        if !matches!(
            self.state.physical.as_ref().unwrap().actors[&guardian].activity,
            Activity::Idle | Activity::Rest
        ) {
            return Some("I must finish my current physical assistance first");
        }
        let mind = &self.state.decisions.as_ref()?.minds[&guardian];
        let Some(food) = mind.knowledge.get(&source) else {
            return Some("I do not know that food source");
        };
        if food.good != Good::Food || !food.accessible || food.quantity < quantity {
            return Some("I do not know enough usable food at that source");
        }
        if decisions::known_path(&mind.routes, self.force_area(guardian)?, food.area).is_err() {
            return Some("I do not know a traversable route to that source");
        }
        if food.area != self.force_area(requester)?
            && mind
                .child_signals
                .get(&requester)
                .is_some_and(|signal| signal.urgency > 0 && signal.observed_at == self.state.time)
        {
            return Some("I will not leave while you visibly need immediate care");
        }
        let held = self
            .state
            .inventories
            .values()
            .filter(|stock| stock.location == InventoryLocation::Carried(guardian))
            .map(|stock| u128::from(stock.quantity.0))
            .sum::<u128>();
        let already_carried = self
            .state
            .inventories
            .get(&source)
            .is_some_and(|stock| stock.location == InventoryLocation::Carried(guardian));
        if (!already_carried
            && held + u128::from(quantity.0)
                > u128::from(
                    self.state
                        .physical
                        .as_ref()
                        .unwrap()
                        .rules
                        .carrying_capacity
                        .0,
                ))
            || quantity
                > self
                    .state
                    .physical
                    .as_ref()
                    .unwrap()
                    .rules
                    .carrying_capacity
        {
            return Some("That amount exceeds my available carrying capacity");
        }
        None
    }
    pub(crate) fn force_cancel_food(&mut self, guardian: PersonId) {
        if let Some(world) = &mut self.state.force {
            world.food.remove(&guardian);
        }
    }
    pub(crate) fn guardian_food_command(&self, guardian: PersonId) -> Option<Command> {
        let supply = self.state.force.as_ref()?.food.get(&guardian)?;
        if supply.delivered >= supply.quantity.0
            || !self.force_body_ready(guardian)
            || self.combat_restricts(guardian)
            || self.force_area(supply.requester).is_none()
        {
            return None;
        }
        let area = self.force_area(guardian)?;
        let mind = &self.state.decisions.as_ref()?.minds[&guardian];
        if let Some((cargo, quantity)) = supply.cargo.iter().find(|(_, quantity)| **quantity > 0) {
            let destination =
                self.state.people[*self.person_index.get(&supply.requester)?].inventory;
            let target = self.force_area(supply.requester)?;
            if area != target {
                return decisions::known_path(&mind.routes, area, target)
                    .ok()
                    .map(|_| Command::Move {
                        destination: target,
                    });
            }
            let offered = (*quantity).min(supply.quantity.0 - supply.delivered);
            if !self.transfer_capacity(destination, offered) {
                return None;
            }
            return Some(Command::Load {
                source: *cargo,
                destination,
                quantity: Quantity(offered),
            });
        }
        let food = mind.knowledge.get(&supply.source)?;
        if food.area != area {
            return decisions::known_path(&mind.routes, area, food.area)
                .ok()
                .map(|_| Command::Move {
                    destination: food.area,
                });
        }
        (food.accessible && food.quantity.0 >= supply.quantity.0 - supply.delivered).then_some(
            Command::Carry {
                source: supply.source,
                quantity: Quantity(supply.quantity.0 - supply.delivered),
            },
        )
    }
    pub(crate) fn force_accepts_food_delivery(
        &self,
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    ) -> bool {
        self.state
            .force
            .as_ref()
            .and_then(|world| world.food.get(&actor))
            .is_some_and(|supply| {
                quantity.0 > 0
                    && quantity.0 <= supply.quantity.0 - supply.delivered
                    && supply
                        .cargo
                        .get(&source)
                        .is_some_and(|staged| *staged >= quantity.0)
                    && self
                        .state
                        .inventories
                        .get(&destination)
                        .is_some_and(|stock| {
                            stock.good == Good::Food
                                && stock.location == InventoryLocation::Carried(supply.requester)
                        })
                    && self.force_area(actor).is_some()
                    && self.force_area(actor) == self.force_area(supply.requester)
            })
    }

    fn force_visible(&self, observer: PersonId, target: PersonId) -> bool {
        let Some(combat) = &self.state.combat else {
            return false;
        };
        let Some(captive) = combat.captivity.get(&target) else {
            return false;
        };
        if self.force_area(observer) != Some(captive.area) {
            return false;
        }
        if let Some(encounter) = combat
            .encounters
            .values()
            .find(|encounter| encounter.active && encounter.fighters.contains_key(&observer))
        {
            return combat.maps[&captive.area]
                .clear(encounter.fighters[&observer].position, captive.position);
        }
        true
    }
    pub(crate) fn force_view(&self, actor: PersonId) -> ForceView {
        let Some(area) = self.force_area(actor) else {
            return ForceView::default();
        };
        let Some(combat) = &self.state.combat else {
            return ForceView::default();
        };
        ForceView {
            confinements: combat
                .captivity
                .values()
                .filter(|captive| captive.area == area && self.force_visible(actor, captive.actor))
                .take(32)
                .map(|captive| ConfinementView {
                    target: captive.actor,
                    captor: captive.captor,
                    area,
                    position: captive.position,
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
    let physical = state.physical.as_ref().ok_or_else(fail)?;
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
    for (actor, attempt) in &world.restraints {
        let body = physical.actors.get(actor).ok_or_else(fail)?;
        if !people.contains_key(actor)
            || !people.contains_key(&attempt.target)
            || actor == &attempt.target
            || attempt.completes <= state.time
            || !body.vitals.alive
            || people[actor].location != Location::AtArea(attempt.area)
            || people[&attempt.target].location != Location::AtArea(attempt.area)
            || state.inventories.get(&attempt.binding).is_none_or(|stock| {
                stock.good != Good::Binding || stock.location != InventoryLocation::Carried(*actor)
            })
            || body.activity
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
            let body = physical.actors.get(&attempt.actor).ok_or_else(fail)?;
            if !matches!(body.activity,Activity::Escape {completes,..}|Activity::Unbind {completes,..} if completes==attempt.completes)
            {
                return Err(fail());
            }
        }
    }
    for (actor, body) in &physical.actors {
        match body.activity {
            Activity::Escape { .. }
                if !world
                    .escapes
                    .get(actor)
                    .is_some_and(|attempt| attempt.actor == *actor && !attempt.contested) =>
            {
                return Err(fail());
            }
            Activity::Restrain { .. } if !world.restraints.contains_key(actor) => {
                return Err(fail());
            }
            _ => {}
        }
    }
    for (guardian, supply) in &world.food {
        if !people.contains_key(guardian)
            || !people.contains_key(&supply.requester)
            || guardian == &supply.requester
            || people[guardian].role != Role::Guardian
            || supply.quantity.0 == 0
            || supply.delivered > supply.quantity.0
            || supply.accepted_at > state.time
            || state
                .inventories
                .get(&supply.source)
                .is_none_or(|stock| stock.good != Good::Food)
        {
            return Err(fail());
        }
        let total = supply
            .cargo
            .values()
            .map(|quantity| u128::from(*quantity))
            .sum::<u128>();
        if total > u128::from(supply.quantity.0 - supply.delivered)
            || supply.cargo.iter().any(|(inventory, quantity)| {
                *quantity == 0
                    || state.inventories.get(inventory).is_none_or(|stock| {
                        stock.good != Good::Food
                            || stock.location != InventoryLocation::Carried(*guardian)
                            || stock.quantity.0 < *quantity
                    })
            })
        {
            return Err(fail());
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
            sim.debug_physical_command(person, Command::Cancel).unwrap();
            sim.debug_set_personality(
                person,
                decisions::Personality::restful(),
                decisions::Dream::Comfort,
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
            sim.state.physical.as_ref().unwrap().actors[&guardian]
                .vitals
                .health,
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
        sim.debug_set_personality(captor, hostile, decisions::Dream::Comfort)
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
            sim.state.physical.as_ref().unwrap().actors[&captive]
                .vitals
                .health
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
    fn guardian_supply_uses_actual_cargo_without_granting_new_incarnation_estate_access() {
        let mut sim = Simulation::force_fixture(91, Execution::default()).unwrap();
        let predecessor = sim.state.ruler;
        let guardian = sim.state.people[1].id;
        let maximum = sim.state.physical.as_ref().unwrap().rules.max_health;
        sim.debug_injure(predecessor, maximum).unwrap();
        let born = sim.advance(SimDuration(900_000)).unwrap();
        assert!(matches!(born.reason, StopReason::SuccessorBorn { .. }));
        let adulthood = sim.state.development.as_ref().unwrap().rules.adulthood;
        sim.advance(adulthood).unwrap();
        let requester = sim.state.ruler;
        assert_ne!(requester, predecessor);
        assert_eq!(
            sim.state.development.as_ref().unwrap().lives[&requester].stage,
            LifeStage::Adult
        );
        sim.debug_physical_command(guardian, Command::Cancel)
            .unwrap();
        let source = sim
            .state
            .inventories
            .values()
            .find(|stock| {
                stock.good == Good::Food && stock.location == InventoryLocation::Area(AreaId(1))
            })
            .unwrap()
            .id;
        let destination = sim.state.people[sim.person_index[&requester]].inventory;
        assert!(!sim.has_stock_access(requester, &sim.state.inventories[&source]));
        let opening = sim.state.inventories[&destination].quantity.0;
        sim.request_guardian_food(guardian, requester, source, Quantity(1_000))
            .unwrap();
        assert!(sim.state.events.iter().any(|event|matches!(event.kind,EventKind::GuardianAccepted {guardian:id,requester:person} if id==guardian&&person==requester)));
        assert_eq!(sim.state.inventories[&destination].quantity.0, opening);
        sim.advance(MINUTE).unwrap();
        assert_eq!(sim.state.inventories[&destination].quantity.0, opening);
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            sim.state.inventories[&destination].quantity.0,
            opening + 1_000
        );
        assert_eq!(
            sim.state.force.as_ref().unwrap().food[&guardian].delivered,
            1_000
        );
        assert!(!sim.has_stock_access(requester, &sim.state.inventories[&source]));
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn unknown_or_unsafe_food_requests_are_explicitly_refused_without_stock_changes() {
        let mut sim = fixture();
        let requester = sim.state.ruler;
        let guardian = sim.state.people[1].id;
        let opening = sim.state.inventories.clone();
        sim.request_guardian_food(guardian, requester, InventoryId(999_999), Quantity(1_000))
            .unwrap();
        assert_eq!(sim.state.inventories, opening);
        assert!(sim.state.events.iter().any(
            |event| matches!(event.kind,EventKind::GuardianRefused {guardian:id,..} if id==guardian)
        ));
        assert!(
            sim.state.lineage.as_ref().unwrap().guardians[&guardian]
                .assignment
                .is_none()
        );
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
            sim.state.physical.as_ref().unwrap().actors[&guardian]
                .vitals
                .health,
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
        let body = weak
            .state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&target)
            .unwrap();
        body.vitals.health = 50_000;
        body.vitals.hunger = 700_000;
        weak.start_escape(target).unwrap();
        weak.advance(SimDuration(6_000)).unwrap();
        assert_eq!(
            weak.state.physical.as_ref().unwrap().actors[&target]
                .vitals
                .health,
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
        sim.debug_set_personality(captor, hostile, decisions::Dream::Comfort)
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
