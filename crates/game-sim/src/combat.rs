//! Initiative turns refine existing people and areas. One shared round deadline
//! advances the world once, including when several encounters run concurrently.
use crate::*;

use game_conflict::distance;
pub use game_conflict::{Captivity, Encounter, Fighter, Map, Profile, Rules, World};

impl Simulation {
    pub fn combat_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::life_fixture(seed, execution)?;
        sim.state.scenario = scenarios().remove(4);
        // These rates and starting bindings belong to this explicit fixture.
        let rules = Rules {
            round: SimDuration(6_000),
            movement: 5_000,
            reach: 2_000,
            damage: 45_000,
            defence: 30_000,
            incapacitation: 45_000,
        };
        let profiles = sim
            .state
            .people
            .iter()
            .map(|p| {
                (
                    p.id,
                    Profile {
                        initiative: if p.role == Role::Ruler { 200 } else { 100 },
                    },
                )
            })
            .collect();
        sim.state.combat = Some(World {
            rules,
            profiles,
            maps: BTreeMap::new(),
            road_maps: BTreeMap::new(),
            encounters: BTreeMap::new(),
            captivity: BTreeMap::new(),
        });
        for actor in [sim.state.ruler, sim.state.people[3].id] {
            sim.debug_add_inventory(
                Good::Binding,
                InventoryLocation::Carried(actor),
                Quantity(1_000),
                Quantity(1_000),
                BTreeSet::from([actor]),
            )?;
        }
        let target = sim.state.people[3].id;
        let mut personality = decisions::Personality::industrious();
        personality.aggression = 900;
        personality.risk_tolerance = 900;
        let dream = sim.state.decisions.as_ref().unwrap().minds[&target].dream;
        sim.debug_set_personality(target, personality, dream)?;
        Ok(sim)
    }
    pub(crate) fn in_combat(&self, actor: PersonId) -> bool {
        self.state.physical.as_ref().is_some_and(|w| {
            matches!(
                w.actors.get(&actor).map(|a| &a.activity),
                Some(Activity::Combat { .. })
            )
        })
    }
    pub(crate) fn combat_restricts(&self, actor: PersonId) -> bool {
        self.in_combat(actor)
            || self.state.combat.as_ref().is_some_and(|w| {
                w.captivity.contains_key(&actor)
                    || self.state.physical.as_ref().is_some_and(|b| {
                        b.actors
                            .get(&actor)
                            .is_some_and(|a| a.vitals.health <= w.rules.incapacitation)
                    })
            })
    }
    pub(crate) fn can_fight(&self, actor: PersonId) -> bool {
        let w = self.state.combat.as_ref().unwrap();
        let v = &self.state.physical.as_ref().unwrap().actors[&actor].vitals;
        w.rules.can_fight(
            game_conflict::TargetBody {
                alive: v.alive,
                health: v.health,
            },
            w.captivity.contains_key(&actor),
        )
    }
    fn can_combat_act(&self, actor: PersonId) -> bool {
        self.independent(actor) && (self.can_fight(actor) || self.force_can_combat_act(actor))
    }
    pub(crate) fn admit_mission_reinforcement(
        &mut self,
        id: EncounterId,
        actor: PersonId,
        team: u32,
        from: AreaId,
    ) -> Result<(), Error> {
        let world = self.state.combat.as_ref().ok_or(Error::Unavailable)?;
        let encounter = world.encounters.get(&id).ok_or(Error::Unavailable)?;
        if !encounter.active
            || encounter.fighters.contains_key(&actor)
            || !encounter
                .fighters
                .values()
                .any(|fighter| fighter.team == team)
            || !self.can_fight(actor)
            || self.in_combat(actor)
            || self.transport_position(actor).map(|p| p.key())
                != transport::location_position(&encounter.location, self.state.time)
                    .ok()
                    .map(|p| p.key())
        {
            return Err(Error::Unavailable);
        }
        let mut anchor = actor;
        while let Location::Carried { carrier } =
            self.state.people[self.person_index[&anchor]].location
        {
            anchor = carrier;
        }
        if anchor != actor && !encounter.fighters.contains_key(&anchor) {
            return Err(Error::Unavailable);
        }
        let area = encounter.area;
        let arrived = self.state.events.iter().rev().take_while(|e| e.time == self.state.time)
            .any(|event| matches!(event.kind, EventKind::Arrived { actor: person, area: destination } if person == actor && destination == area));
        let departed = self.state.events.iter().rev().any(|event| matches!(event.kind,
            EventKind::Departed { actor: person, from: origin, to, arrives } if person == actor && origin == from && to == area && arrives == self.state.time));
        if !arrived || !departed {
            return Err(Error::Unavailable);
        }
        let joins_round = encounter.round.checked_add(1).ok_or(Error::Overflow)?;
        let map = world.map(encounter.map).ok_or(Error::Unavailable)?;
        let position = map
            .exits
            .iter()
            .find(|(area, _)| *area == from)
            .map(|(_, point)| *point)
            .unwrap_or(Point {
                x: 0,
                y: map.height / 2,
            });
        if !map.clear(position, position) {
            return Err(Error::Unavailable);
        }
        self.ensure_event_capacity(1)?;
        self.interrupt_actor_activity(actor);
        self.state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .activity = Activity::Combat { encounter: id };
        self.state
            .combat
            .as_mut()
            .unwrap()
            .encounters
            .get_mut(&id)
            .unwrap()
            .fighters
            .insert(
                actor,
                Fighter {
                    actor,
                    joins_round,
                    team,
                    position,
                    surrendered: false,
                    withdrawn: false,
                    defending: false,
                    retreat: None,
                    known: BTreeMap::new(),
                    search: game_conflict::SearchState::default(),
                },
            );
        self.emit(EventKind::CombatReinforcement {
            encounter: id,
            actor,
            joins_round,
        });
        self.synchronize_carried_fighters(id);
        Ok(())
    }

    pub(crate) fn admit_due_mission_reinforcements(&mut self) {
        for (encounter, actor, team, from) in self.mission_reinforcements() {
            let _ = self.admit_mission_reinforcement(encounter, actor, team, from);
        }
    }

    pub(crate) fn awaiting_combat_input(&self) -> Option<EncounterId> {
        self.state
            .combat
            .as_ref()?
            .encounters
            .values()
            .find(|e| {
                e.active
                    && !self.past_life_controls_actor(self.state.ruler)
                    && e.order.get(e.cursor) == Some(&self.state.ruler)
            })
            .map(|e| e.id)
    }
    pub(crate) fn tactical_view(&self) -> Option<TacticalView> {
        let w = self.state.combat.as_ref()?;
        let e = w
            .encounters
            .values()
            .rev()
            .find(|e| e.fighters.contains_key(&self.state.ruler))?;
        if self.transport_position(self.state.ruler).map(|p| p.key())
            != transport::location_position(&e.location, self.state.time)
                .ok()
                .map(|p| p.key())
            || e.fighters[&self.state.ruler].withdrawn
            || !self.state.physical.as_ref().unwrap().actors[&self.state.ruler]
                .vitals
                .alive
        {
            return None;
        }
        let map = w.map(e.map)?;
        let mine = e.fighters[&self.state.ruler].position;
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
            fighters: e
                .fighters
                .values()
                .filter(|f| {
                    f.actor == self.state.ruler
                        || self.transport_position(f.actor).map(|p| p.key())
                            == transport::location_position(&e.location, self.state.time)
                                .ok()
                                .map(|p| p.key())
                            && map.clear(mine, f.position)
                })
                .map(|f| FighterView {
                    actor: f.actor,
                    position: f.position,
                    team: f.team,
                    incapacitated: !self.can_combat_act(f.actor),
                    surrendered: f.surrendered,
                    captured: w.captivity.contains_key(&f.actor),
                })
                .collect(),
        })
    }
    pub(crate) fn submit_combat(
        &mut self,
        submission: Submission,
    ) -> Result<CommandReceipt, Error> {
        let next = self
            .state
            .next_command
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        self.preflight_agreements()?;
        match &submission.command {
            Command::Engage { target } => {
                self.debug_start_encounter(vec![(self.state.ruler, 0), (*target, 1)])?;
            }
            Command::CombatTurn {
                encounter,
                movement,
                action,
            } => {
                self.execute_turn(*encounter, self.state.ruler, *movement, action.clone())?;
                self.drive_combat_npcs(*encounter)?;
            }
            _ => return Err(Error::Unavailable),
        }
        self.update_property_events();
        self.reconcile_agreements()?;
        self.observe_enforcement();
        let receipt = CommandReceipt {
            caller_sequence: submission.caller_sequence,
            authoritative_sequence: self.state.next_command,
            accepted_at: self.state.time,
        };
        self.state.next_command = next;
        self.state.accepted.insert(
            submission.caller_sequence,
            (submission.command, receipt.clone()),
        );
        self.revision += 1;
        Ok(receipt)
    }
    pub(crate) fn ensure_combat_map(&mut self, area: AreaId) -> Result<CombatMapId, Error> {
        let combat = self.state.combat.as_ref().ok_or(Error::Unavailable)?;
        if let Some(map) = combat.maps.get(&area) {
            return Ok(map.id);
        }
        if !self.state.areas.iter().any(|node| node.id == area) {
            return Err(Error::UnknownArea);
        }
        self.state.next_id.checked_add(1).ok_or(Error::Overflow)?;
        let id = CombatMapId(allocate(&mut self.state)?);
        let offset = ((self.state.seed ^ area.0).wrapping_mul(0x9e3779b97f4a7c15) % 2_000) as i64;
        let exits = self
            .state
            .areas
            .iter()
            .find(|a| a.id == area)
            .unwrap()
            .links
            .iter()
            .enumerate()
            .map(|(i, l)| {
                (
                    l.destination,
                    Point {
                        x: if i.is_multiple_of(2) { 0 } else { 24_000 },
                        y: 8_000,
                    },
                )
            })
            .collect();
        let map = Map {
            id,
            version: 1,
            area,
            location: Location::AtArea(area),
            width: 24_000,
            height: 16_000,
            obstacles: vec![ObstacleView {
                index: 0,
                minimum: Point {
                    x: 12_000 + offset,
                    y: 4_000,
                },
                maximum: Point {
                    x: 14_000 + offset,
                    y: 10_000,
                },
                integrity: 180_000,
            }],
            exits,
        };
        self.state.combat.as_mut().unwrap().maps.insert(area, map);
        Ok(id)
    }

    pub fn debug_start_encounter(
        &mut self,
        participants: Vec<(PersonId, u32)>,
    ) -> Result<EncounterId, Error> {
        let w = self.state.combat.as_ref().ok_or(Error::Unavailable)?;
        if !(2..=32).contains(&participants.len())
            || participants
                .iter()
                .map(|(_, team)| *team)
                .collect::<BTreeSet<_>>()
                .len()
                < 2
        {
            return Err(Error::InvalidFixture(
                "encounter needs opposing participants".into(),
            ));
        }
        let mut ids = BTreeSet::new();
        let mut location = None;
        let mut place = None;
        for (actor, _) in &participants {
            self.person_index.get(actor).ok_or(Error::Unavailable)?;
            let at = self.resolved_location(*actor);
            if !matches!(at, Location::AtArea(_) | Location::StoppedOnRoad { .. }) {
                return Err(Error::Unavailable);
            }
            let key = transport::location_position(&at, self.state.time)?.key();
            if place.is_some_and(|p| p != key)
                || !ids.insert(*actor)
                || !(self.can_combat_act(*actor) || self.force_passive_participant(*actor))
                || self.in_combat(*actor)
                || (!self.independent(*actor) && !self.force_passive_participant(*actor))
            {
                return Err(Error::Unavailable);
            }
            place = Some(key);
            location = Some(at);
        }
        let location = location.unwrap();
        let area = match place.unwrap() {
            game_transport::PositionKey::Area(area) => area,
            game_transport::PositionKey::Road { from, .. } => from,
        };
        for actor in &ids {
            let mut anchor = *actor;
            while let Location::Carried { carrier } =
                self.state.people[self.person_index[&anchor]].location
            {
                anchor = carrier;
            }
            if !ids.contains(&anchor) {
                return Err(Error::Unavailable);
            }
        }
        let ends = SimTime(
            self.state
                .time
                .0
                .checked_add(w.rules.round.0)
                .ok_or(Error::Overflow)?,
        );
        self.state.next_id.checked_add(2).ok_or(Error::Overflow)?;
        self.ensure_event_capacity(participants.len() * 8 + 2)?;
        let map = self.ensure_combat_location_map(&location)?;
        let id = EncounterId(allocate(&mut self.state)?);
        let mut fighters = BTreeMap::new();
        for (i, (actor, team)) in participants.into_iter().enumerate() {
            self.interrupt_actor_activity(actor);
            self.state
                .physical
                .as_mut()
                .unwrap()
                .actors
                .get_mut(&actor)
                .unwrap()
                .activity = Activity::Combat { encounter: id };
            let position = self
                .state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .get(&actor)
                .filter(|c| c.map == map)
                .map(|captive| captive.position)
                .unwrap_or(Point {
                    x: 4_000 + if team == 0 { 0 } else { 2_000 },
                    y: 8_000 + (i as i64 / 2) * 300,
                });
            fighters.insert(
                actor,
                Fighter {
                    actor,
                    joins_round: 0,
                    team,
                    position,
                    surrendered: false,
                    withdrawn: false,
                    defending: false,
                    retreat: None,
                    known: BTreeMap::new(),
                    search: game_conflict::SearchState::default(),
                },
            );
        }
        self.state.combat.as_mut().unwrap().encounters.insert(
            id,
            Encounter {
                id,
                area,
                location,
                map,
                round: 0,
                started: self.state.time,
                ends,
                fighters,
                order: Vec::new(),
                cursor: 0,
                active: true,
            },
        );
        self.synchronize_carried_fighters(id);
        self.emit(EventKind::EncounterStarted {
            encounter: id,
            area,
        });
        self.begin_combat_round(id)?;
        self.revision += 1;
        Ok(id)
    }
    pub(crate) fn ensure_combat_location_map(
        &mut self,
        location: &Location,
    ) -> Result<CombatMapId, Error> {
        if let Location::AtArea(area) = *location {
            return self.ensure_combat_map(area);
        }
        let key = transport::location_position(location, self.state.time)?.key();
        let world = self.state.combat.as_ref().unwrap();
        if let Some(map) = world
            .road_maps
            .values()
            .find(|map| {
                transport::location_position(&map.location, self.state.time)
                    .is_ok_and(|p| p.key() == key)
            })
            .map(|map| map.id)
        {
            return Ok(map);
        }
        let Location::StoppedOnRoad { from, to, .. } = *location else {
            return Err(Error::Unavailable);
        };
        let area = from.min(to);
        let id = CombatMapId(allocate(&mut self.state)?);
        self.state.combat.as_mut().unwrap().road_maps.insert(
            id,
            Map {
                id,
                version: 1,
                area,
                location: location.clone(),
                width: 24_000,
                height: 16_000,
                obstacles: vec![ObstacleView {
                    index: 0,
                    minimum: Point {
                        x: 12_000,
                        y: 4_000,
                    },
                    maximum: Point {
                        x: 14_000,
                        y: 10_000,
                    },
                    integrity: 180_000,
                }],
                exits: vec![
                    (from.min(to), Point { x: 0, y: 8_000 }),
                    (
                        from.max(to),
                        Point {
                            x: 24_000,
                            y: 8_000,
                        },
                    ),
                ],
            },
        );
        Ok(id)
    }
    fn synchronize_carried_fighters(&mut self, id: EncounterId) {
        let positions: Vec<_> = self.state.combat.as_ref().unwrap().encounters[&id]
            .fighters
            .keys()
            .filter_map(|actor| {
                let mut anchor = *actor;
                while let Location::Carried { carrier } =
                    self.state.people[self.person_index[&anchor]].location
                {
                    anchor = carrier;
                }
                (anchor != *actor).then(|| {
                    (
                        *actor,
                        self.state.combat.as_ref().unwrap().encounters[&id].fighters[&anchor]
                            .position,
                    )
                })
            })
            .collect();
        for (actor, position) in positions {
            self.state
                .combat
                .as_mut()
                .unwrap()
                .encounters
                .get_mut(&id)
                .unwrap()
                .fighters
                .get_mut(&actor)
                .unwrap()
                .position = position;
        }
    }
    fn begin_combat_round(&mut self, id: EncounterId) -> Result<(), Error> {
        let e = &self.state.combat.as_ref().unwrap().encounters[&id];
        let w = self.state.combat.as_ref().unwrap();
        let fatigue: Vec<_> = e
            .fighters
            .values()
            .map(|fighter| {
                (
                    fighter.actor,
                    self.state.physical.as_ref().unwrap().actors[&fighter.actor]
                        .vitals
                        .fatigue,
                )
            })
            .collect();
        let order = w.initiative_order(id, &fatigue)?;
        let e = self
            .state
            .combat
            .as_mut()
            .unwrap()
            .encounters
            .get_mut(&id)
            .unwrap();
        e.order = order;
        e.cursor = 0;
        for f in e.fighters.values_mut() {
            f.defending = false;
        }
        self.drive_combat_npcs(id)
    }
    fn observe_fighter(&mut self, encounter: EncounterId, actor: PersonId) {
        let w = self.state.combat.as_ref().unwrap();
        let e = &w.encounters[&encounter];
        let map = w.map(e.map).expect("encounter map");
        let f = &e.fighters[&actor];
        let seen: Vec<_> = e
            .fighters
            .values()
            .filter(|o| o.actor != actor && map.clear(f.position, o.position))
            .map(|o| {
                (
                    o.actor,
                    o.position,
                    !o.withdrawn
                        && self.state.physical.as_ref().unwrap().actors[&o.actor]
                            .vitals
                            .alive,
                )
            })
            .collect();
        // A remembered point is no longer a useful pursuit lead after the
        // actor can see that location and its opponent is absent there.
        let disproved: Vec<_> = f
            .known
            .iter()
            .filter(|(id, point)| {
                map.clear(f.position, **point) && !seen.iter().any(|(other, _, _)| other == *id)
            })
            .map(|(id, _)| *id)
            .collect();
        let known = &mut self
            .state
            .combat
            .as_mut()
            .unwrap()
            .encounters
            .get_mut(&encounter)
            .unwrap()
            .fighters
            .get_mut(&actor)
            .unwrap()
            .known;
        for id in disproved {
            known.remove(&id);
        }
        for (id, position, alive) in seen {
            if alive {
                known.insert(id, position);
            } else {
                known.remove(&id);
            }
        }
    }
    pub(crate) fn binding_stock(&self, actor: PersonId) -> Option<InventoryId> {
        self.inventory_index
            .get(&InventoryLocation::Carried(actor))
            .into_iter()
            .flatten()
            .copied()
            .find(|id| {
                self.state.inventories[id].good == Good::Binding
                    && self.state.inventories[id].quantity.0 >= 1_000
            })
    }
    fn execute_turn(
        &mut self,
        id: EncounterId,
        actor: PersonId,
        movement: Option<Point>,
        action: CombatAction,
    ) -> Result<(), Error> {
        let w = self.state.combat.as_ref().ok_or(Error::Unavailable)?;
        let e = w.encounters.get(&id).ok_or(Error::Unavailable)?;
        if !e.active
            || e.order.get(e.cursor) != Some(&actor)
            || !self.can_combat_act(actor)
            || !self.force_action_allowed(actor, movement, &action)
        {
            return Err(Error::Unavailable);
        }
        let f = &e.fighters[&actor];
        let map = w.map(e.map).expect("encounter map");
        let position = w.turn_position(id, actor, movement)?;
        if matches!(
            self.state.people[self.person_index[&actor]].location,
            Location::Carried { .. }
        ) && movement.is_some_and(|point| point != e.fighters[&actor].position)
        {
            return Err(Error::Unavailable);
        }
        let reachable = |target| w.reachable(id, position, target);
        match action {
            CombatAction::MissionObjective {
                mission,
                ref objective,
            } => {
                self.validate_mission_combat_order(actor, mission, objective)?;
            }
            CombatAction::Attack { target } => {
                if target == actor
                    || !reachable(target)
                    || !self.state.physical.as_ref().unwrap().actors[&target]
                        .vitals
                        .alive
                {
                    return Err(Error::Unavailable);
                }
            }
            CombatAction::Capture { target } => {
                let body = &self
                    .state
                    .physical
                    .as_ref()
                    .unwrap()
                    .actors
                    .get(&target)
                    .ok_or(Error::Unavailable)?
                    .vitals;
                if !w.capture_possible(
                    id,
                    actor,
                    position,
                    target,
                    game_conflict::TargetBody {
                        alive: body.alive,
                        health: body.health,
                    },
                    self.binding_stock(actor).is_some(),
                ) {
                    return Err(Error::Unavailable);
                }
            }
            CombatAction::Release { target } => {
                if !reachable(target) || !self.can_release(actor, target) {
                    return Err(Error::Unavailable);
                }
            }
            CombatAction::Retreat { destination } => {
                if matches!(
                    self.state.people[self.person_index[&actor]].location,
                    Location::Carried { .. }
                ) {
                    return Err(Error::Unavailable);
                }
                if !map.exits.iter().any(|(area, p)| {
                    *area == destination
                        && map.clear(position, *p)
                        && distance(position, *p) <= u128::from(w.rules.reach).pow(2)
                }) || (matches!(e.location, Location::AtArea(_))
                    && !self
                        .state
                        .areas
                        .iter()
                        .find(|a| a.id == e.area)
                        .unwrap()
                        .links
                        .iter()
                        .any(|l| l.destination == destination))
                {
                    return Err(Error::Unavailable);
                }
                if let Location::StoppedOnRoad {
                    from,
                    to,
                    progress,
                    duration,
                } = e.location
                {
                    let point = game_transport::RoadPosition {
                        from,
                        to,
                        progress,
                        duration,
                    };
                    self.road_retreat_plan(point, destination, self.state.time)?;
                } else {
                    self.path(e.area, destination)?;
                    self.departure(e.area, destination)?;
                }
            }
            CombatAction::Breach { obstacle } => {
                let o = map
                    .obstacles
                    .get(obstacle as usize)
                    .filter(|o| o.index == obstacle && o.integrity > 0)
                    .ok_or(Error::Unavailable)?;
                let edge = Point {
                    x: position.x.clamp(o.minimum.x, o.maximum.x),
                    y: position.y.clamp(o.minimum.y, o.maximum.y),
                };
                if distance(position, edge) > u128::from(w.rules.reach).pow(2) {
                    return Err(Error::Unavailable);
                }
            }
            _ => {}
        }
        self.effective_combat_damage(actor, w.rules.damage)?;
        self.ensure_event_capacity(8)?;
        if matches!(action, CombatAction::Capture { .. }) {
            self.state.next_id.checked_add(1).ok_or(Error::Overflow)?;
        }
        let old = f.position;
        let area = e.area;
        let encounter_map = e.map;
        self.state
            .combat
            .as_mut()
            .unwrap()
            .encounters
            .get_mut(&id)
            .unwrap()
            .fighters
            .get_mut(&actor)
            .unwrap()
            .position = position;
        if position != old {
            self.emit(EventKind::CombatMoved {
                encounter: id,
                actor,
                position,
            });
        }
        self.synchronize_carried_fighters(id);
        match action {
            CombatAction::MissionObjective { mission, objective } => {
                self.mission_combat_order(actor, mission, objective)?;
            }
            CombatAction::Attack { target } => {
                let w = self.state.combat.as_ref().unwrap();
                let defending = w.encounters[&id].fighters[&target].defending;
                let damage = self.invocation_combat_damage(
                    actor,
                    self.effective_combat_damage(actor, w.rules.damage)?,
                )?;
                let damage = w.rules.strike_damage(damage, defending);
                self.damage_from_combat(target, damage);
                self.emit(EventKind::Struck {
                    encounter: id,
                    actor,
                    target,
                    damage,
                });
            }
            CombatAction::Defend => {
                self.state
                    .combat
                    .as_mut()
                    .unwrap()
                    .encounters
                    .get_mut(&id)
                    .unwrap()
                    .fighters
                    .get_mut(&actor)
                    .unwrap()
                    .defending = true
            }
            CombatAction::Surrender => {
                self.state
                    .combat
                    .as_mut()
                    .unwrap()
                    .encounters
                    .get_mut(&id)
                    .unwrap()
                    .fighters
                    .get_mut(&actor)
                    .unwrap()
                    .surrendered = true;
                self.emit(EventKind::Surrendered {
                    encounter: id,
                    actor,
                });
                self.force_note_surrender(id, actor);
            }
            CombatAction::Capture { target } => {
                let source = self.binding_stock(actor).unwrap();
                self.state.inventories.get_mut(&source).unwrap().quantity.0 -= 1_000;
                let binding = self
                    .add_inventory(
                        Good::Binding,
                        InventoryLocation::Carried(target),
                        Quantity(0),
                        Quantity(1_000),
                        BTreeSet::from([actor]),
                    )
                    .expect("preflight binding inventory");
                self.state.inventories.get_mut(&binding).unwrap().quantity = Quantity(1_000);
                let target_position =
                    self.state.combat.as_ref().unwrap().encounters[&id].fighters[&target].position;
                let captured_map = self.state.combat.as_ref().unwrap().encounters[&id].map;
                self.state.combat.as_mut().unwrap().captivity.insert(
                    target,
                    Captivity {
                        actor: target,
                        captor: actor,
                        area,
                        position: target_position,
                        map: captured_map,
                        binding,
                    },
                );
                self.emit(EventKind::Transferred {
                    actor,
                    source,
                    destination: binding,
                    quantity: Quantity(1_000),
                });
                self.emit(EventKind::Captured {
                    encounter: id,
                    actor,
                    target,
                    binding,
                });
            }
            CombatAction::Release { target } => {
                self.release_captive(actor, target);
            }
            CombatAction::Retreat { destination } => {
                let f = self
                    .state
                    .combat
                    .as_mut()
                    .unwrap()
                    .encounters
                    .get_mut(&id)
                    .unwrap()
                    .fighters
                    .get_mut(&actor)
                    .unwrap();
                f.withdrawn = true;
                f.retreat = Some(destination);
                let passengers: Vec<_> = self.state.combat.as_ref().unwrap().encounters[&id]
                    .fighters
                    .keys()
                    .filter_map(|person| {
                        let mut current = *person;
                        while let Location::Carried { carrier } =
                            self.state.people[self.person_index[&current]].location
                        {
                            current = carrier;
                        }
                        (current == actor && *person != actor).then_some(*person)
                    })
                    .collect();
                for passenger in passengers {
                    let body = self
                        .state
                        .combat
                        .as_mut()
                        .unwrap()
                        .encounters
                        .get_mut(&id)
                        .unwrap()
                        .fighters
                        .get_mut(&passenger)
                        .unwrap();
                    body.withdrawn = true;
                    body.retreat = Some(destination);
                }
            }
            CombatAction::Breach { obstacle } => {
                let w = self.state.combat.as_mut().unwrap();
                let attack = w.rules.damage;
                let map = w.map_mut(encounter_map).unwrap();
                let o = &mut map.obstacles[obstacle as usize];
                let damage = o.integrity.min(attack);
                o.integrity -= damage;
                let map = map.id;
                self.emit(EventKind::ObstacleDamaged {
                    map,
                    obstacle,
                    damage,
                });
            }
            _ => {}
        }
        self.state
            .combat
            .as_mut()
            .unwrap()
            .encounters
            .get_mut(&id)
            .unwrap()
            .cursor += 1;
        self.revision += 1;
        Ok(())
    }
    fn damage_from_combat(&mut self, actor: PersonId, damage: u64) {
        let body = self
            .state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap();
        body.vitals.health = body.vitals.health.saturating_sub(damage);
        if body.vitals.health == 0 && !self.state.people[self.person_index[&actor]].hunger_exempt {
            self.end_life(actor);
        }
    }
    pub(crate) fn drive_combat_npcs(&mut self, id: EncounterId) -> Result<(), Error> {
        loop {
            let w = self.state.combat.as_ref().unwrap();
            let e = &w.encounters[&id];
            let Some(actor) = e.order.get(e.cursor).copied() else {
                break;
            };
            if !self.can_combat_act(actor)
                || e.fighters[&actor].withdrawn
                || e.fighters[&actor].surrendered
            {
                self.state
                    .combat
                    .as_mut()
                    .unwrap()
                    .encounters
                    .get_mut(&id)
                    .unwrap()
                    .cursor += 1;
                continue;
            }
            if actor == self.state.ruler && !self.past_life_controls_actor(actor) {
                break;
            }
            self.observe_fighter(id, actor);
            self.review_mission_combat(id, actor);
            let mission_action = self.mission_combat_action(id, actor);
            let w = self.state.combat.as_ref().unwrap();
            let e = &w.encounters[&id];
            let f = &e.fighters[&actor];
            let map = w.map(e.map).expect("encounter map");
            let rescue = self
                .force_rescue_target(actor)
                .filter(|target| f.known.contains_key(target));
            let target = rescue.or_else(|| {
                f.known
                    .keys()
                    .filter(|other| e.fighters.get(other).is_some_and(|o| o.team != f.team))
                    .min_by_key(|other| (distance(f.position, f.known[other]), **other))
                    .copied()
            });
            let (movement, action) = if let Some(pair) = mission_action {
                pair
            } else if let Some(action) = self.force_combat_action(actor, id) {
                (None, action)
            } else if let Some(target) = target {
                let destination = f.known[&target];
                let movement = map.navigation_step(
                    f.position,
                    destination,
                    w.rules.movement,
                    w.rules.reach.saturating_sub(2),
                );
                let visible = map.clear(f.position, e.fighters[&target].position);
                if !visible
                    && (movement.is_none()
                        || map.clear(f.position, destination)
                            && distance(f.position, destination)
                                <= u128::from(w.rules.reach).pow(2))
                {
                    (self.combat_search_step(id, actor), CombatAction::Defend)
                } else {
                    let from = movement.unwrap_or(f.position);
                    let reachable = visible
                        && map.clear(from, destination)
                        && distance(from, e.fighters[&target].position)
                            <= u128::from(w.rules.reach).pow(2);
                    let health = self.state.physical.as_ref().unwrap().actors[&actor]
                        .vitals
                        .health;
                    let borrowed = self.invocation_personality(actor);
                    let traits = borrowed.as_ref().unwrap_or(
                        &self.state.decisions.as_ref().unwrap().minds[&actor].personality,
                    );
                    let action = if rescue == Some(target)
                        && reachable
                        && self.can_release(actor, target)
                    {
                        CombatAction::Release { target }
                    } else if self.state.people[self.person_index[&actor]].role == Role::Guardian
                        && target == self.state.ruler
                    {
                        CombatAction::Defend
                    } else if traits.risk_tolerance < 500
                        && health < self.state.physical.as_ref().unwrap().rules.max_health / 2
                    {
                        CombatAction::Surrender
                    } else if reachable
                        && (e.fighters[&target].surrendered || !self.can_fight(target))
                        && self.binding_stock(actor).is_some()
                        && !w.captivity.contains_key(&target)
                    {
                        CombatAction::Capture { target }
                    } else if reachable
                        && !e.fighters[&target].surrendered
                        && (!w.captivity.contains_key(&target) || self.force_can_combat_act(target))
                        && rescue != Some(target)
                    {
                        CombatAction::Attack { target }
                    } else {
                        CombatAction::Defend
                    };
                    (movement, action)
                }
            } else {
                (self.combat_search_step(id, actor), CombatAction::Defend)
            };
            if self.execute_turn(id, actor, movement, action).is_err() {
                self.execute_turn(id, actor, None, CombatAction::Wait)?;
            }
        }
        let ends = self.state.combat.as_ref().unwrap().encounters[&id].ends;
        if !self.state.due.get(&ends).is_some_and(|due| {
            due.iter()
                .any(|d| matches!(d,Due::CombatRound(e) if *e == id))
        }) {
            self.state
                .due
                .entry(ends)
                .or_default()
                .push(Due::CombatRound(id));
        }
        Ok(())
    }
    pub(crate) fn combat_search_step(&mut self, id: EncounterId, actor: PersonId) -> Option<Point> {
        let world = self.state.combat.as_ref()?;
        let encounter = world.encounters.get(&id)?;
        let fighter = encounter.fighters.get(&actor)?;
        let mut search = fighter.search.clone();
        let movement = world
            .map(encounter.map)
            .expect("encounter map")
            .search_step(fighter.position, world.rules.movement, &mut search);
        self.state
            .combat
            .as_mut()?
            .encounters
            .get_mut(&id)?
            .fighters
            .get_mut(&actor)?
            .search = search;
        movement
    }
    pub(crate) fn preflight_combat(&self, due: &[Due], at: SimTime) -> Result<(), Error> {
        let Some(w) = &self.state.combat else {
            return Ok(());
        };
        for d in due {
            if let Due::CombatRound(id) = d {
                let e = &w.encounters[id];
                e.round.checked_add(1).ok_or(Error::Overflow)?;
                at.0.checked_add(w.rules.round.0).ok_or(Error::Overflow)?;
                for f in e.fighters.values().filter(|f| f.retreat.is_some()) {
                    if let Location::StoppedOnRoad {
                        from,
                        to,
                        progress,
                        duration,
                    } = e.location
                    {
                        self.road_retreat_plan(
                            game_transport::RoadPosition {
                                from,
                                to,
                                progress,
                                duration,
                            },
                            f.retreat.unwrap(),
                            at,
                        )?;
                        continue;
                    }
                    let link = self
                        .state
                        .areas
                        .iter()
                        .find(|a| a.id == e.area)
                        .and_then(|a| a.links.iter().find(|l| Some(l.destination) == f.retreat))
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
        let from = if destination == point.to {
            point.from
        } else if destination == point.from {
            point.to
        } else {
            return Err(Error::NoRoute);
        };
        let duration = self
            .state
            .areas
            .iter()
            .find(|a| a.id == from)
            .and_then(|a| a.links.iter().find(|link| link.destination == destination))
            .ok_or(Error::NoRoute)?
            .duration;
        game_transport::retreat(point, destination, duration, at)
    }
    pub(crate) fn finish_combat_round(&mut self, id: EncounterId) -> Result<(), Error> {
        let e = self.state.combat.as_ref().unwrap().encounters[&id].clone();
        for f in e.fighters.values().filter(|f| f.retreat.is_some()) {
            if matches!(
                self.state.people[self.person_index[&f.actor]].location,
                Location::Carried { .. }
            ) {
                continue;
            }
            self.state
                .physical
                .as_mut()
                .unwrap()
                .actors
                .get_mut(&f.actor)
                .unwrap()
                .activity = Activity::Idle;
            if matches!(e.location, Location::StoppedOnRoad { .. }) {
                self.transport_retreat(f.actor, f.retreat.unwrap())?;
            } else {
                self.start_movement(f.actor, f.retreat.unwrap())?;
            }
            self.state
                .combat
                .as_mut()
                .unwrap()
                .encounters
                .get_mut(&id)
                .unwrap()
                .fighters
                .get_mut(&f.actor)
                .unwrap()
                .retreat = None;
        }
        for f in e.fighters.values().filter(|f| {
            f.retreat.is_some()
                && matches!(
                    self.state.people[self.person_index[&f.actor]].location,
                    Location::Carried { .. }
                )
        }) {
            self.state
                .physical
                .as_mut()
                .unwrap()
                .actors
                .get_mut(&f.actor)
                .unwrap()
                .activity = Activity::Idle;
            self.state
                .combat
                .as_mut()
                .unwrap()
                .encounters
                .get_mut(&id)
                .unwrap()
                .fighters
                .get_mut(&f.actor)
                .unwrap()
                .retreat = None;
        }
        self.emit(EventKind::RoundEnded {
            encounter: id,
            round: e.round,
        });
        let teams: BTreeSet<_> = e
            .fighters
            .values()
            .filter(|f| {
                !f.withdrawn
                    && self.state.physical.as_ref().unwrap().actors[&f.actor]
                        .vitals
                        .alive
                    && (!self
                        .state
                        .combat
                        .as_ref()
                        .unwrap()
                        .captivity
                        .contains_key(&f.actor)
                        || self.force_can_combat_act(f.actor) && !f.surrendered)
            })
            .map(|f| f.team)
            .collect();
        if teams.len() <= 1 {
            self.state
                .combat
                .as_mut()
                .unwrap()
                .encounters
                .get_mut(&id)
                .unwrap()
                .active = false;
            for actor in e.fighters.keys() {
                if matches!(
                    self.state.physical.as_ref().unwrap().actors[actor].activity,
                    Activity::Combat { .. }
                ) {
                    self.state
                        .physical
                        .as_mut()
                        .unwrap()
                        .actors
                        .get_mut(actor)
                        .unwrap()
                        .activity = Activity::Idle;
                }
            }
            self.emit(EventKind::EncounterEnded { encounter: id });
            self.mission_react_to_encounter_end(id);
        } else {
            let ends = SimTime(
                self.state
                    .time
                    .0
                    .checked_add(self.state.combat.as_ref().unwrap().rules.round.0)
                    .ok_or(Error::Overflow)?,
            );
            let encounter = self
                .state
                .combat
                .as_mut()
                .unwrap()
                .encounters
                .get_mut(&id)
                .unwrap();
            encounter.started = self.state.time;
            encounter.ends = ends;
            encounter.round += 1;
            self.begin_combat_round(id)?;
        }
        Ok(())
    }
}

impl Simulation {
    pub(crate) fn can_release(&self, actor: PersonId, target: PersonId) -> bool {
        let Some(c) = self
            .state
            .combat
            .as_ref()
            .and_then(|w| w.captivity.get(&target))
        else {
            return false;
        };
        if !self.force_body_ready(actor)
            || (self
                .state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&actor)
                && actor != target)
        {
            return false;
        }
        self.transport_same_place(actor, target)
            && !self.transport_is_moving(actor)
            && (c.captor == actor
                || self
                    .inventory_index
                    .get(&InventoryLocation::Carried(actor))
                    .into_iter()
                    .flatten()
                    .any(|id| {
                        self.state.inventories[id].good == Good::Tool
                            && self.state.inventories[id].quantity.0 >= 1_000
                    }))
    }
    pub(crate) fn release_captive(&mut self, actor: PersonId, target: PersonId) {
        self.state
            .combat
            .as_mut()
            .unwrap()
            .captivity
            .remove(&target);
        self.transport_release_confinement(target);
        if let Some(force) = &mut self.state.force {
            force.escapes.remove(&target);
            force.yielded.remove(&target);
        }
        self.emit(EventKind::Released { actor, target });
    }
}

pub(crate) fn validate(s: &CanonicalState) -> Result<(), Error> {
    let Some(w) = &s.combat else {
        return Ok(());
    };
    let fail = || Error::InvalidSave("inconsistent combat state".into());
    let people: BTreeSet<_> = s.people.iter().map(|p| p.id).collect();
    let people_by_id: BTreeMap<_, _> = s.people.iter().map(|person| (person.id, person)).collect();
    let locations = transport::resolved_locations_for(s)?;
    if w.rules.round.0 == 0
        || w.rules.movement > 1_000_000
        || w.rules.reach > 1_000_000
        || w.rules.damage == 0
        || w.rules.incapacitation >= s.physical.as_ref().unwrap().rules.max_health
        || w.profiles.len() != people.len()
        || w.profiles
            .iter()
            .any(|(id, p)| !people.contains(id) || p.initiative > 1_000_000)
    {
        return Err(fail());
    }
    for (area, map) in &w.maps {
        if *area != map.area
            || map.location != Location::AtArea(*area)
            || map.version != 1
            || !(1..=1_000_000).contains(&map.width)
            || !(1..=1_000_000).contains(&map.height)
            || !s.areas.iter().any(|a| a.id == *area)
        {
            return Err(fail());
        }
        for (i, o) in map.obstacles.iter().enumerate() {
            if o.index != i as u32
                || !map.contains(o.minimum)
                || !map.contains(o.maximum)
                || o.minimum.x >= o.maximum.x
                || o.minimum.y >= o.maximum.y
            {
                return Err(fail());
            }
        }
        for (destination, point) in &map.exits {
            if !map.contains(*point)
                || !s
                    .areas
                    .iter()
                    .any(|a| a.id == *area && a.links.iter().any(|l| l.destination == *destination))
            {
                return Err(fail());
            }
        }
    }
    for (id, map) in &w.road_maps {
        let Location::StoppedOnRoad {
            from,
            to,
            progress,
            duration,
        } = map.location
        else {
            return Err(fail());
        };
        if *id != map.id
            || map.version != 1
            || map.area != from.min(to)
            || !(1..=1_000_000).contains(&map.width)
            || !(1..=1_000_000).contains(&map.height)
            || progress > duration
            || duration.0 == 0
            || !s.areas.iter().any(|a| {
                a.id == from
                    && a.links
                        .iter()
                        .any(|link| link.destination == to && link.duration == duration)
            })
        {
            return Err(fail());
        }
        for (index, obstacle) in map.obstacles.iter().enumerate() {
            if obstacle.index != index as u32
                || !map.contains(obstacle.minimum)
                || !map.contains(obstacle.maximum)
                || obstacle.minimum.x >= obstacle.maximum.x
                || obstacle.minimum.y >= obstacle.maximum.y
            {
                return Err(fail());
            }
        }
        if map.exits.len() != 2
            || map
                .exits
                .iter()
                .any(|(area, point)| (*area != from && *area != to) || !map.contains(*point))
            || map.exits[0].0 == map.exits[1].0
        {
            return Err(fail());
        }
    }
    let mut active = BTreeSet::new();
    let mut deadlines = BTreeSet::new();
    for (at, due) in &s.due {
        for d in due {
            if let Due::CombatRound(id) = d
                && (!w
                    .encounters
                    .get(id)
                    .is_some_and(|e| e.active && e.ends == *at)
                    || !deadlines.insert(*id))
            {
                return Err(fail());
            }
        }
    }
    for (id, e) in &w.encounters {
        let map = w.map(e.map).ok_or_else(fail)?;
        let place = transport::location_position(&e.location, s.time)?.key();
        if *id != e.id
            || e.area != map.area
            || !matches!(
                e.location,
                Location::AtArea(_) | Location::StoppedOnRoad { .. }
            )
            || transport::location_position(&map.location, s.time)?.key() != place
            || e.map != map.id
            || e.started > s.time
            || e.ends.0.checked_sub(e.started.0) != Some(w.rules.round.0)
            || e.cursor > e.order.len()
            || e.order.iter().collect::<BTreeSet<_>>().len() != e.order.len()
            || e.order.iter().any(|a| !e.fighters.contains_key(a))
            || e.active != deadlines.contains(id)
        {
            return Err(fail());
        }
        for (actor, f) in &e.fighters {
            if e.active && !f.withdrawn {
                let mut anchor = *actor;
                while let Location::Carried { carrier } = people_by_id[&anchor].location {
                    anchor = carrier;
                }
                if anchor != *actor
                    && !e
                        .fighters
                        .get(&anchor)
                        .is_some_and(|parent| parent.position == f.position)
                {
                    return Err(fail());
                }
            }
            if *actor != f.actor
                || f.joins_round > e.round.saturating_add(1)
                || (f.joins_round > e.round && e.order.contains(actor))
                || !people.contains(actor)
                || !map.contains(f.position)
                || !f.search.valid(map)
                || f.known
                    .iter()
                    .any(|(id, p)| !e.fighters.contains_key(id) || !map.contains(*p))
            {
                return Err(fail());
            }
            if e.active
                && (!f.withdrawn || f.retreat.is_some())
                && s.physical.as_ref().unwrap().actors[actor].vitals.alive
                && (!active.insert(*actor)
                    || s.physical.as_ref().unwrap().actors[actor].activity
                        != (Activity::Combat { encounter: *id })
                    || transport::location_position(&locations[actor], s.time)?.key() != place)
            {
                return Err(fail());
            }
        }
    }
    for p in &s.people {
        if matches!(
            s.physical.as_ref().unwrap().actors[&p.id].activity,
            Activity::Combat { .. }
        ) != active.contains(&p.id)
        {
            return Err(fail());
        }
    }
    for (actor, c) in &w.captivity {
        if *actor != c.actor
            || !people.contains(actor)
            || !people.contains(&c.captor)
            || !w
                .map(c.map)
                .is_some_and(|m| m.area == c.area && m.contains(c.position))
            || !s.inventories.get(&c.binding).is_some_and(|i| {
                i.good == Good::Binding
                    && i.quantity.0 == 1_000
                    && i.location == InventoryLocation::Carried(*actor)
            })
        {
            return Err(fail());
        }
    }
    Ok(())
}

#[cfg(test)]
mod reinforcement_tests {
    use super::*;

    #[test]
    fn a_real_arrival_joins_at_an_entrance_and_waits_for_the_next_initiative_round() {
        let mut sim = Simulation::combat_fixture(77, Execution::default()).unwrap();
        let ruler = sim.state.ruler;
        let opponent = sim.state.people[1].id;
        let reinforcement = sim.state.people[4].id;
        sim.state.combat.as_mut().unwrap().rules.damage = 1;
        sim.debug_set_personality(
            reinforcement,
            decisions::Personality::restful(),
            decisions::Dream::Comfort,
        )
        .unwrap();
        sim.debug_move_actor(reinforcement, AreaId(2)).unwrap();
        sim.advance(SimDuration(120_000)).unwrap();
        sim.debug_move_actor(reinforcement, AreaId(1)).unwrap();
        let arrival = match sim.state.people[sim.person_index[&reinforcement]].location {
            Location::InTransit { arrives, .. } => arrives,
            _ => panic!("reinforcements must travel"),
        };
        let encounter = sim
            .debug_start_encounter(vec![(ruler, 0), (opponent, 1)])
            .unwrap();
        let before = sim.state.clone();
        assert_eq!(
            sim.admit_mission_reinforcement(encounter, reinforcement, 0, AreaId(2)),
            Err(Error::Unavailable)
        );
        assert_eq!(sim.state, before);
        while sim.time() < arrival {
            if sim.awaiting_combat_input().is_some() {
                sim.execute_turn(encounter, ruler, None, CombatAction::Defend)
                    .unwrap();
            }
            sim.advance(SimDuration(6_000)).unwrap();
        }
        sim.admit_mission_reinforcement(encounter, reinforcement, 0, AreaId(2))
            .unwrap();
        let battle = &sim.state.combat.as_ref().unwrap().encounters[&encounter];
        assert!(!battle.order.contains(&reinforcement));
        assert_eq!(
            battle.fighters[&reinforcement].joins_round,
            battle.round + 1
        );
        let entrance = sim.state.combat.as_ref().unwrap().maps[&AreaId(1)]
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
            sim.state.combat.as_ref().unwrap().encounters[&encounter]
                .order
                .contains(&reinforcement)
        );
    }
}
