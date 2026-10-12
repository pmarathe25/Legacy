//! Initiative turns refine existing people and areas. One shared round deadline
//! advances the world once, including when several encounters run concurrently.
use crate::*;

use game_conflict::distance;
pub use game_conflict::{Captivity, Encounter, Fighter, Map, Profile, Rules, World};

struct EncounterOpening {
    location: Location,
    area: AreaId,
    ends: SimTime,
}

pub(crate) enum PreparedCombatMap {
    Existing(Map),
    New(Map),
}
impl PreparedCombatMap {
    pub(crate) fn map(&self) -> &Map {
        match self {
            Self::Existing(map) | Self::New(map) => map,
        }
    }
    pub(crate) fn is_new(&self) -> bool {
        matches!(self, Self::New(_))
    }
}

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
        let profiles = sim
            .host
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
        sim.host.state.combat = Some(World {
            rules,
            profiles,
            maps: BTreeMap::new(),
            road_maps: BTreeMap::new(),
            encounters: BTreeMap::new(),
            captivity: BTreeMap::new(),
        });
        for actor in [sim.host.state.ruler, sim.host.state.people[3].id] {
            sim.debug_add_inventory(
                Good::Binding,
                InventoryLocation::Carried(actor),
                Quantity(1_000),
                Quantity(1_000),
                BTreeSet::from([actor]),
            )?;
        }
        let target = sim.host.state.people[3].id;
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
        self.in_combat(actor)
            || self.host.state.combat.as_ref().is_some_and(|w| {
                w.captivity.contains_key(&actor)
                    || self.host.state.physical.as_ref().is_some_and(|_| {
                        game_physiology::body(&self.host.storage, actor)
                            .is_some_and(|a| a.vitals.health <= w.rules.incapacitation)
                    })
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
        let world = self.host.state.combat.as_ref().ok_or(Error::Unavailable)?;
        let encounter = world.encounters.get(&id).ok_or(Error::Unavailable)?;
        if !encounter.active
            || encounter.fighters.len() >= world.rules.encounter_limit
            || encounter.fighters.contains_key(&actor)
            || !encounter
                .fighters
                .values()
                .any(|fighter| fighter.team == team)
            || !self.can_fight(actor)
            || self.in_combat(actor)
            || self.transport_position(actor).map(|p| p.key())
                != transport::location_position(&encounter.location, self.host.clock().time)
                    .ok()
                    .map(|p| p.key())
        {
            return Err(Error::Unavailable);
        }
        let mut anchor = actor;
        while let Location::Carried { carrier } = *game_spatial::location(
            &self.host.storage,
            game_ecs::StableId((self.host.state.people[self.person_index[&anchor]].id).0),
        )
        .expect("location exists")
        {
            anchor = carrier;
        }
        if anchor != actor && !encounter.fighters.contains_key(&anchor) {
            return Err(Error::Unavailable);
        }
        let area = encounter.area;
        let arrived = self.host.events().iter().rev().take_while(|e| e.at == self.host.clock().time)
            .any(|event| matches!(event.event, EventKind::Arrived { actor: person, area: destination } if person == actor && destination == area));
        let departed = self.host.events().iter().rev().any(|event| matches!(event.event,
            EventKind::Departed { actor: person, from: origin, to, arrives } if person == actor && origin == from && to == area && arrives == self.host.clock().time));
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
        self.admit_combat_fighter(id, actor, team, position, joins_round)
    }
    pub(crate) fn admit_local_army_approach(
        &mut self,
        approach: &armies::Approach,
    ) -> Result<(), Error> {
        let (team, position) = self.validate_army_approach(approach, true)?;
        let w = self.host.state.combat.as_ref().unwrap();
        let e = &w.encounters[&approach.encounter];
        if e.fighters.len() >= w.rules.encounter_limit
            || self
                .host
                .state
                .armies
                .as_ref()
                .unwrap()
                .approaches
                .get(&approach.actor)
                != Some(approach)
            || *self
                .host
                .storage
                .activity(approach.actor)
                .expect("body exists")
                != (Activity::ArmyApproach {
                    mission: approach.mission,
                    encounter: approach.encounter,
                    completes: approach.completes,
                })
        {
            return Err(Error::Unavailable);
        }
        let joins_round = e.round.checked_add(1).ok_or(Error::Overflow)?;
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
        self.ensure_event_capacity(1)?;
        self.interrupt_actor_activity(actor);
        *self.host.storage.activity_mut(actor).unwrap() = Activity::Combat { encounter: id };
        self.host
            .state
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
        self.host
            .state
            .combat
            .as_ref()?
            .encounters
            .values()
            .find(|e| {
                e.active
                    && !self.past_life_controls_actor(self.host.state.ruler)
                    && e.order.get(e.cursor) == Some(&self.host.state.ruler)
            })
            .map(|e| e.id)
    }
    pub(crate) fn tactical_view(&self) -> Option<TacticalView> {
        let w = self.host.state.combat.as_ref()?;
        let e = w
            .encounters
            .values()
            .rev()
            .find(|e| e.fighters.contains_key(&self.host.state.ruler))?;
        if self
            .transport_position(self.host.state.ruler)
            .map(|p| p.key())
            != transport::location_position(&e.location, self.host.clock().time)
                .ok()
                .map(|p| p.key())
            || e.fighters[&self.host.state.ruler].withdrawn
            || !game_physiology::body(&self.host.storage, self.host.state.ruler)
                .expect("body exists")
                .vitals
                .alive
        {
            return None;
        }
        let map = w.map(e.map)?;
        let mine = e.fighters[&self.host.state.ruler].position;
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
                    f.actor == self.host.state.ruler
                        || self.transport_position(f.actor).map(|p| p.key())
                            == transport::location_position(&e.location, self.host.clock().time)
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
        self.host.preflight_records(0, 0, 1)?;
        let next = self
            .host
            .state
            .next_command
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        self.preflight_agreements()?;
        match &submission.command {
            Command::Engage { target } => {
                self.debug_start_encounter(vec![(self.host.state.ruler, 0), (*target, 1)])?;
            }
            Command::CombatTurn {
                encounter,
                movement,
                action,
            } => {
                self.execute_turn(*encounter, self.host.state.ruler, *movement, action.clone())?;
                self.drive_combat_npcs(*encounter)?;
            }
            _ => return Err(Error::Unavailable),
        }
        self.update_property_events()?;
        self.reconcile_agreements()?;
        self.observe_enforcement();
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
        self.host.bump_revision(1)?;
        Ok(receipt)
    }
    pub(crate) fn preflight_start_encounter(
        &self,
        participants: &[(PersonId, u32)],
    ) -> Result<(), Error> {
        self.prepare_encounter_opening(participants).map(|_| ())
    }

    fn prepare_encounter_opening(
        &self,
        participants: &[(PersonId, u32)],
    ) -> Result<EncounterOpening, Error> {
        let w = self.host.state.combat.as_ref().ok_or(Error::Unavailable)?;
        if !w.rules.capacity_valid()
            || !(2..=w.rules.encounter_limit).contains(&participants.len())
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
        for (actor, _) in participants {
            self.person_index.get(actor).ok_or(Error::Unavailable)?;
            let at = self.resolved_location(*actor);
            if !matches!(at, Location::AtArea(_) | Location::StoppedOnRoad { .. }) {
                return Err(Error::Unavailable);
            }
            let key = transport::location_position(&at, self.host.clock().time)?.key();
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
            while let Location::Carried { carrier } = *game_spatial::location(
                &self.host.storage,
                game_ecs::StableId((self.host.state.people[self.person_index[&anchor]].id).0),
            )
            .expect("location exists")
            {
                anchor = carrier;
            }
            if !ids.contains(&anchor) {
                return Err(Error::Unavailable);
            }
        }
        let ends = SimTime(
            self.host
                .clock()
                .time
                .0
                .checked_add(w.rules.round.0)
                .ok_or(Error::Overflow)?,
        );
        self.host.next_id().checked_add(2).ok_or(Error::Overflow)?;
        self.ensure_event_capacity(participants.len() * 8 + 2)?;
        // An existing map can be fully obstructed; reject before allocating or
        // interrupting anybody. Generated maps have clear deployment space.
        let existing_map = w.maps.values().chain(w.road_maps.values()).find(|map| {
            transport::location_position(&map.location, self.host.clock().time)
                .is_ok_and(|p| Some(p.key()) == place)
        });
        if existing_map.is_some_and(|map| {
            participants
                .iter()
                .enumerate()
                .any(|(i, (_, team))| map.deployment(i, *team).is_none())
        }) {
            return Err(Error::Unavailable);
        }
        Ok(EncounterOpening {
            location,
            area,
            ends,
        })
    }

    pub fn debug_start_encounter(
        &mut self,
        participants: Vec<(PersonId, u32)>,
    ) -> Result<EncounterId, Error> {
        let owner_now = self.host.clock().time;
        self.host.preflight_records(0, 0, 1)?;
        let EncounterOpening {
            location,
            area,
            ends,
        } = self.prepare_encounter_opening(&participants)?;
        let map = self.ensure_combat_location_map(&location)?;
        let positions: Vec<_> = participants
            .iter()
            .enumerate()
            .map(|(i, (actor, team))| {
                self.host
                    .state
                    .combat
                    .as_ref()
                    .unwrap()
                    .captivity
                    .get(actor)
                    .filter(|c| c.map == map)
                    .map(|c| c.position)
                    .or_else(|| {
                        self.host
                            .state
                            .combat
                            .as_ref()
                            .unwrap()
                            .map(map)
                            .unwrap()
                            .deployment(i, *team)
                    })
                    .ok_or(Error::Unavailable)
            })
            .collect::<Result<_, _>>()?;
        let id = EncounterId(self.host.allocate_id()?.0);
        let mut fighters = BTreeMap::new();
        for (i, (actor, team)) in participants.into_iter().enumerate() {
            self.interrupt_actor_activity(actor);
            *self.host.storage.activity_mut(actor).unwrap() = Activity::Combat { encounter: id };
            let position = positions[i];
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
        self.host.state.combat.as_mut().unwrap().encounters.insert(
            id,
            Encounter {
                id,
                area,
                location,
                map,
                round: 0,
                started: owner_now,
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
        self.host.bump_revision(1)?;
        Ok(id)
    }
    pub(crate) fn prepare_combat_location_map(
        &self,
        location: &Location,
    ) -> Result<PreparedCombatMap, Error> {
        let combat = self.host.state.combat.as_ref().ok_or(Error::Unavailable)?;
        if let Location::AtArea(area) = *location {
            if let Some(map) = combat.maps.get(&area) {
                return Ok(PreparedCombatMap::Existing(map.clone()));
            }
            if !self.host.state.areas.iter().any(|node| node.id == area) {
                return Err(Error::UnknownArea);
            }
            self.host.next_id().checked_add(1).ok_or(Error::Overflow)?;
            let id = CombatMapId(self.host.next_id());
            let offset =
                ((self.host.seed() ^ area.0).wrapping_mul(0x9e3779b97f4a7c15) % 2_000) as i64;
            let exits = self
                .host
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
            return Ok(PreparedCombatMap::New(map));
        }
        let key = transport::location_position(location, self.host.clock().time)?.key();
        if let Some(map) = combat.road_maps.values().find(|map| {
            transport::location_position(&map.location, self.host.clock().time)
                .is_ok_and(|point| point.key() == key)
        }) {
            return Ok(PreparedCombatMap::Existing(map.clone()));
        }
        let Location::StoppedOnRoad { from, to, .. } = *location else {
            return Err(Error::Unavailable);
        };
        if !self.host.state.areas.iter().any(|area| area.id == from)
            || !self.host.state.areas.iter().any(|area| area.id == to)
        {
            return Err(Error::UnknownArea);
        }
        self.host.next_id().checked_add(1).ok_or(Error::Overflow)?;
        let id = CombatMapId(self.host.next_id());
        let area = from.min(to);
        Ok(PreparedCombatMap::New(Map {
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
        }))
    }
    /// Install only a preflighted map at its reserved global ID. Geometry and
    /// location construction do not run after a confinement begins mutation.
    pub(crate) fn install_combat_location_map(&mut self, plan: PreparedCombatMap) -> CombatMapId {
        match plan {
            PreparedCombatMap::Existing(map) => map.id,
            PreparedCombatMap::New(map) => {
                let id = CombatMapId(self.host.allocate_id().expect("reserved scene ID").0);
                debug_assert_eq!(id, map.id);
                let combat = self.host.state.combat.as_mut().unwrap();
                match map.location {
                    Location::AtArea(area) => {
                        combat.maps.insert(area, map);
                    }
                    Location::StoppedOnRoad { .. } => {
                        combat.road_maps.insert(id, map);
                    }
                    _ => unreachable!("prepared stationary scene"),
                }
                id
            }
        }
    }
    pub(crate) fn ensure_combat_location_map(
        &mut self,
        location: &Location,
    ) -> Result<CombatMapId, Error> {
        let plan = self.prepare_combat_location_map(location)?;
        Ok(self.install_combat_location_map(plan))
    }
    fn synchronize_carried_fighters(&mut self, id: EncounterId) {
        let positions: Vec<_> = self.host.state.combat.as_ref().unwrap().encounters[&id]
            .fighters
            .keys()
            .filter_map(|actor| {
                let mut anchor = *actor;
                while let Location::Carried { carrier } = *game_spatial::location(
                    &self.host.storage,
                    game_ecs::StableId((self.host.state.people[self.person_index[&anchor]].id).0),
                )
                .expect("location exists")
                {
                    anchor = carrier;
                }
                (anchor != *actor).then(|| {
                    (
                        *actor,
                        self.host.state.combat.as_ref().unwrap().encounters[&id].fighters[&anchor]
                            .position,
                    )
                })
            })
            .collect();
        for (actor, position) in positions {
            self.host
                .state
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
        let e = &self.host.state.combat.as_ref().unwrap().encounters[&id];
        let w = self.host.state.combat.as_ref().unwrap();
        let fatigue: Vec<_> = e
            .fighters
            .values()
            .map(|fighter| {
                (
                    fighter.actor,
                    game_physiology::body(&self.host.storage, fighter.actor)
                        .expect("body exists")
                        .vitals
                        .fatigue,
                )
            })
            .collect();
        let order = w.initiative_order(id, &fatigue)?;
        let e = self
            .host
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
        let w = self.host.state.combat.as_ref().unwrap();
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
                        && game_physiology::body(&self.host.storage, o.actor)
                            .expect("body exists")
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
            .host
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
    fn execute_turn(
        &mut self,
        id: EncounterId,
        actor: PersonId,
        movement: Option<Point>,
        action: CombatAction,
    ) -> Result<(), Error> {
        self.host.preflight_records(0, 0, 1)?;
        let w = self.host.state.combat.as_ref().ok_or(Error::Unavailable)?;
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
            *game_spatial::location(
                &self.host.storage,
                game_ecs::StableId((self.host.state.people[self.person_index[&actor]].id).0)
            )
            .expect("location exists"),
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
                let leader = self.host.state.missions.as_ref().unwrap().missions[&mission]
                    .proposal
                    .leader;
                if leader != actor && !reachable(leader) {
                    return Err(Error::Unavailable);
                }
            }
            CombatAction::Attack { target } => {
                if target == actor
                    || !reachable(target)
                    || !game_physiology::body(&self.host.storage, target)
                        .expect("body exists")
                        .vitals
                        .alive
                {
                    return Err(Error::Unavailable);
                }
            }
            CombatAction::Capture { target } => {
                let body = &game_physiology::body(&self.host.storage, target)
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
                    *game_spatial::location(
                        &self.host.storage,
                        game_ecs::StableId(
                            (self.host.state.people[self.person_index[&actor]].id).0
                        )
                    )
                    .expect("location exists"),
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
                        .host
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
                    self.road_retreat_plan(point, destination, self.host.clock().time)?;
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
            self.host.next_id().checked_add(1).ok_or(Error::Overflow)?;
        }
        let confinement = if let CombatAction::Capture { target } = action {
            let source = self.binding_stock(actor).ok_or(Error::Unavailable)?;
            let facts = self.confinement_facts(
                actor,
                target,
                e.fighters[&target].surrendered,
                reachable(target),
                None,
            )?;
            let mut anchor = target;
            while let Location::Carried { carrier } = *game_spatial::location(
                &self.host.storage,
                game_ecs::StableId((self.host.state.people[self.person_index[&anchor]].id).0),
            )
            .expect("location exists")
            {
                anchor = carrier;
            }
            let target_position = if anchor == actor {
                position
            } else {
                e.fighters[&anchor].position
            };
            let context = game_conflict::ConfinementContext {
                map: e.map,
                area: e.area,
                location: e.location.clone(),
                target_position,
            };
            let plan = game_conflict::prepare_confinement(
                &w.rules,
                facts.clone(),
                game_cargo::inventory(&self.host.storage, source).expect("inventory exists"),
                InventoryId(self.host.next_id()),
                context,
                game_conflict::ConfinementMode::Tactical { encounter: id },
                self.host.clock().time,
            )?;
            Some(plan.begin(&self.host.storage, &w.captivity, &facts)?)
        } else {
            None
        };
        let old = f.position;
        let encounter_map = e.map;
        self.host
            .state
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
                let w = self.host.state.combat.as_ref().unwrap();
                let defending = w.encounters[&id].fighters[&target].defending;
                let damage = self.invocation_combat_damage(
                    actor,
                    self.effective_combat_damage(actor, w.rules.damage)?,
                )?;
                let strike = w.rules.strike(
                    actor,
                    damage,
                    defending,
                    self.host
                        .state
                        .skills
                        .as_ref()
                        .map_or(0, |world| world.rules.action_practice_ms),
                );
                self.damage_from_combat(target, strike.damage);
                self.emit(EventKind::Struck {
                    encounter: id,
                    actor,
                    target,
                    damage: strike.damage,
                });
                self.commit_skills(vec![strike.practice]);
            }
            CombatAction::Defend => {
                self.host
                    .state
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
                self.host
                    .state
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
            CombatAction::Capture { .. } => {
                let plan = confinement.expect("capture was prepared before movement");
                self.apply_confinement(plan, None)?;
            }
            CombatAction::Release { target } => {
                self.release_captive(actor, target);
            }
            CombatAction::Retreat { destination } => {
                let f = self
                    .host
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
                let passengers: Vec<_> = self.host.state.combat.as_ref().unwrap().encounters[&id]
                    .fighters
                    .keys()
                    .filter_map(|person| {
                        let mut current = *person;
                        while let Location::Carried { carrier } = *game_spatial::location(
                            &self.host.storage,
                            game_ecs::StableId(
                                (self.host.state.people[self.person_index[&current]].id).0,
                            ),
                        )
                        .expect("location exists")
                        {
                            current = carrier;
                        }
                        (current == actor && *person != actor).then_some(*person)
                    })
                    .collect();
                for passenger in passengers {
                    let body = self
                        .host
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
                let w = self.host.state.combat.as_mut().unwrap();
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
        self.host
            .state
            .combat
            .as_mut()
            .unwrap()
            .encounters
            .get_mut(&id)
            .unwrap()
            .cursor += 1;
        self.host.bump_revision(1)?;
        Ok(())
    }
    fn damage_from_combat(&mut self, actor: PersonId, damage: u64) {
        let body = game_physiology::body_mut(&mut self.host.storage, actor).unwrap();
        body.vitals.health = body.vitals.health.saturating_sub(damage);
        if body.vitals.health == 0
            && !self.host.state.people[self.person_index[&actor]].hunger_exempt
        {
            self.end_life(actor);
        }
    }
    pub(crate) fn drive_combat_npcs(&mut self, id: EncounterId) -> Result<(), Error> {
        loop {
            let w = self.host.state.combat.as_ref().unwrap();
            let e = &w.encounters[&id];
            let Some(actor) = e.order.get(e.cursor).copied() else {
                break;
            };
            if !self.can_combat_act(actor)
                || e.fighters[&actor].withdrawn
                || e.fighters[&actor].surrendered
            {
                self.host
                    .state
                    .combat
                    .as_mut()
                    .unwrap()
                    .encounters
                    .get_mut(&id)
                    .unwrap()
                    .cursor += 1;
                continue;
            }
            if actor == self.host.state.ruler && !self.past_life_controls_actor(actor) {
                break;
            }
            self.observe_fighter(id, actor);
            self.review_mission_combat(id, actor);
            let mission_action = self.mission_combat_action(id, actor);
            let w = self.host.state.combat.as_ref().unwrap();
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
                    let health = game_physiology::body(&self.host.storage, actor)
                        .expect("body exists")
                        .vitals
                        .health;
                    let borrowed = self.invocation_personality(actor);
                    let traits = borrowed.as_ref().unwrap_or(
                        &game_characters::decisions::mind::<game_api::Command>(
                            &self.host.storage,
                            actor,
                        )
                        .unwrap()
                        .personality,
                    );
                    let action = if rescue == Some(target)
                        && reachable
                        && self.can_release(actor, target)
                    {
                        CombatAction::Release { target }
                    } else if self.host.state.people[self.person_index[&actor]].role
                        == Role::Guardian
                        && target == self.host.state.ruler
                    {
                        CombatAction::Defend
                    } else if traits.risk_tolerance < 500
                        && health < self.host.state.physical.as_ref().unwrap().rules.max_health / 2
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
        let ends = self.host.state.combat.as_ref().unwrap().encounters[&id].ends;
        if !self.host.state.due.get(&ends).is_some_and(|due| {
            due.iter()
                .any(|d| matches!(d,Due::CombatRound(e) if *e == id))
        }) {
            self.host
                .state
                .due
                .entry(ends)
                .or_default()
                .push(Due::CombatRound(id));
        }
        Ok(())
    }
    pub(crate) fn combat_search_step(&mut self, id: EncounterId, actor: PersonId) -> Option<Point> {
        let world = self.host.state.combat.as_ref()?;
        let encounter = world.encounters.get(&id)?;
        let fighter = encounter.fighters.get(&actor)?;
        let mut search = fighter.search.clone();
        let movement = world
            .map(encounter.map)
            .expect("encounter map")
            .search_step(fighter.position, world.rules.movement, &mut search);
        self.host
            .state
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
        let Some(w) = &self.host.state.combat else {
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
                        .host
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
            .host
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
        let owner_now = self.host.clock().time;
        let e = self.host.state.combat.as_ref().unwrap().encounters[&id].clone();
        for f in e.fighters.values().filter(|f| f.retreat.is_some()) {
            if matches!(
                *game_spatial::location(
                    &self.host.storage,
                    game_ecs::StableId((self.host.state.people[self.person_index[&f.actor]].id).0)
                )
                .expect("location exists"),
                Location::Carried { .. }
            ) {
                continue;
            }
            *self.host.storage.activity_mut(f.actor).unwrap() = Activity::Idle;
            if matches!(e.location, Location::StoppedOnRoad { .. }) {
                self.transport_retreat(f.actor, f.retreat.unwrap())?;
            } else {
                self.start_movement(f.actor, f.retreat.unwrap())?;
            }
            self.host
                .state
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
        let carried_retreats: Vec<_> = e
            .fighters
            .values()
            .filter(|f| {
                f.retreat.is_some()
                    && matches!(
                        *game_spatial::location(
                            &self.host.storage,
                            game_ecs::StableId(
                                (self.host.state.people[self.person_index[&f.actor]].id).0
                            )
                        )
                        .expect("location exists"),
                        Location::Carried { .. }
                    )
            })
            .map(|fighter| fighter.actor)
            .collect();
        for actor in carried_retreats {
            *self.host.storage.activity_mut(actor).unwrap() = Activity::Idle;
            self.host
                .state
                .combat
                .as_mut()
                .unwrap()
                .encounters
                .get_mut(&id)
                .unwrap()
                .fighters
                .get_mut(&actor)
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
                    && game_physiology::body(&self.host.storage, f.actor)
                        .expect("body exists")
                        .vitals
                        .alive
                    && (!self
                        .host
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
            self.host
                .state
                .combat
                .as_mut()
                .unwrap()
                .encounters
                .get_mut(&id)
                .unwrap()
                .active = false;
            for actor in e.fighters.keys() {
                if matches!(
                    *self.host.storage.activity(*actor).expect("body exists"),
                    Activity::Combat { .. }
                ) {
                    *self.host.storage.activity_mut(*actor).unwrap() = Activity::Idle;
                }
            }
            self.emit(EventKind::EncounterEnded { encounter: id });
            self.mission_react_to_encounter_end(id);
        } else {
            let ends = SimTime(
                owner_now
                    .0
                    .checked_add(self.host.state.combat.as_ref().unwrap().rules.round.0)
                    .ok_or(Error::Overflow)?,
            );
            let encounter = self
                .host
                .state
                .combat
                .as_mut()
                .unwrap()
                .encounters
                .get_mut(&id)
                .unwrap();
            encounter.started = owner_now;
            encounter.ends = ends;
            encounter.round += 1;
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
                actor_captive: world.captivity.contains_key(&actor),
                contact: self.transport_same_place(actor, target),
                moving: self.transport_is_moving(actor),
            },
            world.captivity.get(&target),
            self.inventory_index
                .get(&InventoryLocation::Carried(actor))
                .into_iter()
                .flatten()
                .filter_map(|id| game_cargo::inventory(&self.host.storage, *id)),
        )
    }
    pub(crate) fn release_captive(&mut self, actor: PersonId, target: PersonId) {
        self.host
            .state
            .combat
            .as_mut()
            .unwrap()
            .captivity
            .remove(&target);
        self.transport_release_confinement(target);
        if let Some(force) = &mut self.host.state.force {
            force.escapes.remove(&target);
            force.yielded.remove(&target);
        }
        self.emit(EventKind::Released { actor, target });
    }
}

pub(crate) fn validate(s: &CanonicalState) -> Result<(), Error> {
    let Some(w) = &s.state.combat else {
        return Ok(());
    };
    let fail = || Error::InvalidSave("inconsistent combat state".into());
    let people: BTreeSet<_> = s.state.people.iter().map(|p| p.id).collect();
    let locations = transport::resolved_locations_for(s)?;
    if w.rules.round.0 == 0
        || !w.rules.capacity_valid()
        || w.rules.movement > 1_000_000
        || w.rules.reach > 1_000_000
        || w.rules.damage == 0
        || w.rules.incapacitation >= s.state.physical.as_ref().unwrap().rules.max_health
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
            || !s.state.areas.iter().any(|a| a.id == *area)
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
                    .state
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
            || !s.state.areas.iter().any(|a| {
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
    for (at, due) in &s.state.due {
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
            || !(2..=w.rules.encounter_limit).contains(&e.fighters.len())
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
                while let Location::Carried { carrier } =
                    *game_spatial::location(s.components, game_ecs::StableId((anchor).0))
                        .ok_or_else(fail)?
                {
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
                && game_physiology::body(s.components, *actor)
                    .expect("body exists")
                    .vitals
                    .alive
                && (!active.insert(*actor)
                    || *s.components.activity(*actor).expect("body exists")
                        != (Activity::Combat { encounter: *id })
                    || transport::location_position(&locations[actor], s.time)?.key() != place)
            {
                return Err(fail());
            }
        }
    }
    for p in &s.state.people {
        if matches!(
            *s.components.activity(p.id).expect("body exists"),
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
            || !game_cargo::inventory(s.components, c.binding).is_some_and(|i| {
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
        let ruler = sim.host.state.ruler;
        let opponent = sim.host.state.people[1].id;
        let reinforcement = sim.host.state.people[4].id;
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
            game_ecs::StableId((sim.host.state.people[sim.person_index[&reinforcement]].id).0),
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
        let battle = &sim.host.state.combat.as_ref().unwrap().encounters[&encounter];
        assert!(!battle.order.contains(&reinforcement));
        assert_eq!(
            battle.fighters[&reinforcement].joins_round,
            battle.round + 1
        );
        let entrance = sim.host.state.combat.as_ref().unwrap().maps[&AreaId(1)]
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
            sim.host.state.combat.as_ref().unwrap().encounters[&encounter]
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
        for person in sim.host.state.people.clone() {
            sim.debug_actor_command(person.id, Command::Cancel).unwrap();
            sim.debug_set_personality(
                person.id,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )
            .unwrap();
        }
        sim.host.state.combat.as_mut().unwrap().maps.clear();
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
        sim.host
            .state
            .combat
            .as_mut()
            .unwrap()
            .maps
            .get_mut(&AreaId(1))
            .unwrap()
            .obstacles
            .clear();
        let mut cautious = decisions::Personality::restful();
        cautious.risk_tolerance = 0;
        sim.debug_set_personality(target, cautious, decisions::Aspiration::Comfort)
            .unwrap();
        let maximum = sim.host.state.physical.as_ref().unwrap().rules.max_health;
        sim.debug_injure(target, maximum * 3 / 5).unwrap();
        let world = sim.host.state.combat.as_mut().unwrap();
        world.profiles.get_mut(&target).unwrap().initiative = 1_000_000;
        world.profiles.get_mut(&actor).unwrap().initiative = 900_000;
        // Starting the encounter really gives the faster NPC its own turn. It
        // moves and chooses surrender from its actual injury and personality.
        let encounter = sim
            .debug_start_encounter(vec![(actor, 0), (target, 1)])
            .unwrap();
        let world = sim.host.state.combat.as_ref().unwrap();
        let battle = &world.encounters[&encounter];
        assert_eq!(battle.order.get(battle.cursor), Some(&actor));
        assert!(battle.fighters[&target].surrendered);
        assert!(sim.host.events().iter().any(|event| matches!(event.event,
            EventKind::Surrendered { encounter: id, actor: who } if id == encounter && who == target)));
        let position = battle.fighters[&target].position;
        let movement = world.map(battle.map).unwrap().navigation_step(
            battle.fighters[&actor].position,
            position,
            world.rules.movement,
            world.rules.reach,
        );
        assert!(world.reachable(
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
            let target = sim.host.state.people[3].id;
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
            let captive = sim.host.state.combat.as_ref().unwrap().captivity[&target].clone();
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
                    .map(captive.map)
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
        let target = sim.host.state.people[3].id;
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
        let target = sim.host.state.people[3].id;
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
