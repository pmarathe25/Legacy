//! Initiative turns refine existing people and areas. One shared round deadline
//! advances the world once, including when several encounters run concurrently.
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    pub round: SimDuration,
    pub movement: u64,
    pub reach: u64,
    pub damage: u64,
    pub defence: u64,
    pub incapacitation: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub initiative: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Map {
    pub id: CombatMapId,
    pub version: u32,
    pub area: AreaId,
    pub width: i64,
    pub height: i64,
    pub obstacles: Vec<ObstacleView>,
    pub exits: Vec<(AreaId, Point)>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fighter {
    pub actor: PersonId,
    pub team: u32,
    pub position: Point,
    pub surrendered: bool,
    pub withdrawn: bool,
    pub defending: bool,
    pub retreat: Option<AreaId>,
    pub known: BTreeMap<PersonId, Point>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Encounter {
    pub id: EncounterId,
    pub area: AreaId,
    pub map: CombatMapId,
    pub round: u64,
    pub started: SimTime,
    pub ends: SimTime,
    pub fighters: BTreeMap<PersonId, Fighter>,
    pub order: Vec<PersonId>,
    pub cursor: usize,
    pub active: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Captivity {
    pub actor: PersonId,
    pub captor: PersonId,
    pub area: AreaId,
    pub position: Point,
    pub binding: InventoryId,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub rules: Rules,
    pub profiles: BTreeMap<PersonId, Profile>,
    pub maps: BTreeMap<AreaId, Map>,
    pub encounters: BTreeMap<EncounterId, Encounter>,
    pub captivity: BTreeMap<PersonId, Captivity>,
}

impl Map {
    fn contains(&self, p: Point) -> bool {
        p.x >= 0 && p.y >= 0 && p.x <= self.width && p.y <= self.height
    }
    fn clear(&self, a: Point, b: Point) -> bool {
        self.contains(a)
            && self.contains(b)
            && self
                .obstacles
                .iter()
                .filter(|o| o.integrity > 0)
                .all(|o| !crosses(a, b, o.minimum, o.maximum))
    }
}
fn distance(a: Point, b: Point) -> u128 {
    let x = i128::from(a.x) - i128::from(b.x);
    let y = i128::from(a.y) - i128::from(b.y);
    (x * x + y * y) as u128
}
fn crosses(a: Point, b: Point, min: Point, max: Point) -> bool {
    let inside = |p: Point| p.x >= min.x && p.x <= max.x && p.y >= min.y && p.y <= max.y;
    if inside(a) || inside(b) {
        return true;
    }
    let orient = |a: Point, b: Point, c: Point| {
        (i128::from(b.x) - i128::from(a.x)) * (i128::from(c.y) - i128::from(a.y))
            - (i128::from(b.y) - i128::from(a.y)) * (i128::from(c.x) - i128::from(a.x))
    };
    let segment = |c: Point, d: Point| {
        if a.x.max(b.x) < c.x.min(d.x)
            || c.x.max(d.x) < a.x.min(b.x)
            || a.y.max(b.y) < c.y.min(d.y)
            || c.y.max(d.y) < a.y.min(b.y)
        {
            return false;
        }
        let (x, y, u, v) = (
            orient(a, b, c),
            orient(a, b, d),
            orient(c, d, a),
            orient(c, d, b),
        );
        (x == 0 || y == 0 || x.signum() != y.signum())
            && (u == 0 || v == 0 || u.signum() != v.signum())
    };
    let p = Point { x: min.x, y: max.y };
    let q = Point { x: max.x, y: min.y };
    segment(min, p) || segment(p, max) || segment(max, q) || segment(q, min)
}

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
    fn can_fight(&self, actor: PersonId) -> bool {
        let w = self.state.combat.as_ref().unwrap();
        let v = &self.state.physical.as_ref().unwrap().actors[&actor].vitals;
        v.alive && v.health > w.rules.incapacitation && !w.captivity.contains_key(&actor)
    }
    pub(crate) fn awaiting_combat_input(&self) -> Option<EncounterId> {
        self.state
            .combat
            .as_ref()?
            .encounters
            .values()
            .find(|e| e.active && e.order.get(e.cursor) == Some(&self.state.ruler))
            .map(|e| e.id)
    }
    pub(crate) fn tactical_view(&self) -> Option<TacticalView> {
        let w = self.state.combat.as_ref()?;
        let e = w
            .encounters
            .values()
            .rev()
            .find(|e| e.fighters.contains_key(&self.state.ruler))?;
        if self.state.people[self.person_index[&self.state.ruler]].location
            != Location::AtArea(e.area)
            || e.fighters[&self.state.ruler].withdrawn
            || !self.state.physical.as_ref().unwrap().actors[&self.state.ruler]
                .vitals
                .alive
        {
            return None;
        }
        let map = &w.maps[&e.area];
        let mine = e.fighters[&self.state.ruler].position;
        Some(TacticalView {
            encounter: e.id,
            active: e.active,
            map: e.map,
            area: e.area,
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
                        || self.state.people[self.person_index[&f.actor]].location
                            == Location::AtArea(e.area)
                            && map.clear(mine, f.position)
                })
                .map(|f| FighterView {
                    actor: f.actor,
                    position: f.position,
                    team: f.team,
                    incapacitated: !self.can_fight(f.actor),
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
        let mut area = None;
        for (actor, _) in &participants {
            let p = self
                .person_index
                .get(actor)
                .map(|i| &self.state.people[*i])
                .ok_or(Error::Unavailable)?;
            let Location::AtArea(at) = p.location else {
                return Err(Error::Unavailable);
            };
            if area.is_some_and(|a| a != at)
                || !ids.insert(*actor)
                || !self.can_fight(*actor)
                || self.in_combat(*actor)
                || !self.independent(*actor)
            {
                return Err(Error::Unavailable);
            }
            area = Some(at);
        }
        let area = area.unwrap();
        let ends = SimTime(
            self.state
                .time
                .0
                .checked_add(w.rules.round.0)
                .ok_or(Error::Overflow)?,
        );
        self.state
            .next_id
            .checked_add(if w.maps.contains_key(&area) { 1 } else { 2 })
            .ok_or(Error::Overflow)?;
        self.ensure_event_capacity(participants.len() * 8 + 2)?;
        if !self.state.combat.as_ref().unwrap().maps.contains_key(&area) {
            let id = CombatMapId(allocate(&mut self.state)?);
            let offset =
                ((self.state.seed ^ area.0).wrapping_mul(0x9e3779b97f4a7c15) % 2_000) as i64;
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
        }
        let id = EncounterId(allocate(&mut self.state)?);
        let map = self.state.combat.as_ref().unwrap().maps[&area].id;
        let mut fighters = BTreeMap::new();
        for (i, (actor, team)) in participants.into_iter().enumerate() {
            self.interrupt_physical(actor);
            self.state
                .physical
                .as_mut()
                .unwrap()
                .actors
                .get_mut(&actor)
                .unwrap()
                .activity = Activity::Combat { encounter: id };
            fighters.insert(
                actor,
                Fighter {
                    actor,
                    team,
                    position: Point {
                        x: 4_000 + if team == 0 { 0 } else { 2_000 },
                        y: 8_000 + (i as i64 / 2) * 300,
                    },
                    surrendered: false,
                    withdrawn: false,
                    defending: false,
                    retreat: None,
                    known: BTreeMap::new(),
                },
            );
        }
        self.state.combat.as_mut().unwrap().encounters.insert(
            id,
            Encounter {
                id,
                area,
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
        self.emit(EventKind::EncounterStarted {
            encounter: id,
            area,
        });
        self.begin_combat_round(id)?;
        self.revision += 1;
        Ok(id)
    }
    fn begin_combat_round(&mut self, id: EncounterId) -> Result<(), Error> {
        let e = &self.state.combat.as_ref().unwrap().encounters[&id];
        let w = self.state.combat.as_ref().unwrap();
        let mut order: Vec<_> = e
            .fighters
            .values()
            .filter(|f| !f.withdrawn)
            .map(|f| f.actor)
            .collect();
        order.sort_by_key(|actor| {
            (
                std::cmp::Reverse(
                    w.profiles[actor].initiative.saturating_sub(
                        self.state.physical.as_ref().unwrap().actors[actor]
                            .vitals
                            .fatigue
                            / 1_000,
                    ),
                ),
                *actor,
            )
        });
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
        let map = &w.maps[&e.area];
        let f = &e.fighters[&actor];
        let seen: Vec<_> = e
            .fighters
            .values()
            .filter(|o| o.team != f.team && map.clear(f.position, o.position))
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
        for (id, position, alive) in seen {
            if alive {
                known.insert(id, position);
            } else {
                known.remove(&id);
            }
        }
    }
    fn binding_stock(&self, actor: PersonId) -> Option<InventoryId> {
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
        if !e.active || e.order.get(e.cursor) != Some(&actor) || !self.can_fight(actor) {
            return Err(Error::Unavailable);
        }
        let f = &e.fighters[&actor];
        let map = &w.maps[&e.area];
        let position = movement.unwrap_or(f.position);
        if !map.contains(position)
            || distance(position, f.position) > u128::from(w.rules.movement).pow(2)
            || !map.clear(f.position, position)
        {
            return Err(Error::Unavailable);
        }
        let reachable = |target: PersonId| {
            e.fighters.get(&target).is_some_and(|t| {
                !t.withdrawn
                    && distance(position, t.position) <= u128::from(w.rules.reach).pow(2)
                    && map.clear(position, t.position)
            })
        };
        match action {
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
                if target == actor
                    || !reachable(target)
                    || w.captivity.contains_key(&target)
                    || !self.state.physical.as_ref().unwrap().actors[&target]
                        .vitals
                        .alive
                    || !(e.fighters[&target].surrendered || !self.can_fight(target))
                    || self.binding_stock(actor).is_none()
                {
                    return Err(Error::Unavailable);
                }
            }
            CombatAction::Release { target } => {
                if !reachable(target)
                    || !w.captivity.get(&target).is_some_and(|c| c.captor == actor)
                {
                    return Err(Error::Unavailable);
                }
            }
            CombatAction::Retreat { destination } => {
                if !map.exits.iter().any(|(area, p)| {
                    *area == destination
                        && distance(position, *p) <= u128::from(w.rules.reach).pow(2)
                }) || !self
                    .state
                    .areas
                    .iter()
                    .find(|a| a.id == e.area)
                    .unwrap()
                    .links
                    .iter()
                    .any(|l| l.destination == destination)
                {
                    return Err(Error::Unavailable);
                }
                self.path(e.area, destination)?;
                self.departure(e.area, destination)?;
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
        match action {
            CombatAction::Attack { target } => {
                let w = self.state.combat.as_ref().unwrap();
                let defending = w.encounters[&id].fighters[&target].defending;
                let damage = self
                    .effective_combat_damage(actor, w.rules.damage)?
                    .saturating_sub(if defending { w.rules.defence } else { 0 });
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
            }
            CombatAction::Capture { target } => {
                let source = self.binding_stock(actor).unwrap();
                self.state.inventories.get_mut(&source).unwrap().quantity.0 -= 1_000;
                let binding = self
                    .add_inventory(
                        Good::Binding,
                        InventoryLocation::Area(area),
                        Quantity(1_000),
                        Quantity(1_000),
                        BTreeSet::from([actor]),
                    )
                    .expect("preflight binding inventory");
                let target_position =
                    self.state.combat.as_ref().unwrap().encounters[&id].fighters[&target].position;
                self.state.combat.as_mut().unwrap().captivity.insert(
                    target,
                    Captivity {
                        actor: target,
                        captor: actor,
                        area,
                        position: target_position,
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
                self.state
                    .combat
                    .as_mut()
                    .unwrap()
                    .captivity
                    .remove(&target);
                self.emit(EventKind::Released { actor, target });
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
            }
            CombatAction::Breach { obstacle } => {
                let w = self.state.combat.as_mut().unwrap();
                let map = w.maps.get_mut(&area).unwrap();
                let o = &mut map.obstacles[obstacle as usize];
                let damage = o.integrity.min(w.rules.damage);
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
    fn drive_combat_npcs(&mut self, id: EncounterId) -> Result<(), Error> {
        loop {
            let w = self.state.combat.as_ref().unwrap();
            let e = &w.encounters[&id];
            let Some(actor) = e.order.get(e.cursor).copied() else {
                break;
            };
            if !self.can_fight(actor)
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
            if actor == self.state.ruler {
                break;
            }
            self.observe_fighter(id, actor);
            let w = self.state.combat.as_ref().unwrap();
            let e = &w.encounters[&id];
            let f = &e.fighters[&actor];
            let map = &w.maps[&e.area];
            let target = f
                .known
                .keys()
                .filter(|other| e.fighters.get(other).is_some_and(|o| o.team != f.team))
                .min_by_key(|other| (distance(f.position, f.known[other]), **other))
                .copied();
            let (movement, action) = if let Some(target) = target {
                let destination = f.known[&target];
                let d = distance(f.position, destination).isqrt() as i64;
                let step = d
                    .saturating_sub(w.rules.reach as i64)
                    .min(w.rules.movement as i64);
                let p = if d > 0 {
                    Point {
                        x: f.position.x + (destination.x - f.position.x) * step / d,
                        y: f.position.y + (destination.y - f.position.y) * step / d,
                    }
                } else {
                    f.position
                };
                let movement = if p != f.position && map.clear(f.position, p) {
                    Some(p)
                } else {
                    None
                };
                let from = movement.unwrap_or(f.position);
                let reachable = map.clear(from, e.fighters[&target].position)
                    && distance(from, e.fighters[&target].position)
                        <= u128::from(w.rules.reach).pow(2);
                let health = self.state.physical.as_ref().unwrap().actors[&actor]
                    .vitals
                    .health;
                let traits = &self.state.decisions.as_ref().unwrap().minds[&actor].personality;
                let action = if self.state.people[self.person_index[&actor]].role == Role::Guardian
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
                    && !w.captivity.contains_key(&target)
                {
                    CombatAction::Attack { target }
                } else {
                    CombatAction::Defend
                };
                (movement, action)
            } else {
                (None, CombatAction::Defend)
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

    pub(crate) fn finish_combat_round(&mut self, id: EncounterId) -> Result<(), Error> {
        let e = self.state.combat.as_ref().unwrap().encounters[&id].clone();
        for f in e.fighters.values().filter(|f| f.retreat.is_some()) {
            self.state
                .physical
                .as_mut()
                .unwrap()
                .actors
                .get_mut(&f.actor)
                .unwrap()
                .activity = Activity::Idle;
            self.start_movement(f.actor, f.retreat.unwrap())?;
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
                    && !self
                        .state
                        .combat
                        .as_ref()
                        .unwrap()
                        .captivity
                        .contains_key(&f.actor)
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
        self.person_index
            .get(&actor)
            .is_some_and(|i| self.state.people[*i].location == Location::AtArea(c.area))
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
        self.emit(EventKind::Released { actor, target });
    }
}

pub(crate) fn validate(s: &CanonicalState) -> Result<(), Error> {
    let Some(w) = &s.combat else {
        return Ok(());
    };
    let fail = || Error::InvalidSave("inconsistent combat state".into());
    let people: BTreeSet<_> = s.people.iter().map(|p| p.id).collect();
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
        let map = w.maps.get(&e.area).ok_or_else(fail)?;
        if *id != e.id
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
            if *actor != f.actor
                || !people.contains(actor)
                || !map.contains(f.position)
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
                    || !s
                        .people
                        .iter()
                        .any(|p| p.id == *actor && p.location == Location::AtArea(e.area)))
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
            || !s
                .people
                .iter()
                .any(|p| p.id == *actor && p.location == Location::AtArea(c.area))
            || !w.maps.get(&c.area).is_some_and(|m| m.contains(c.position))
            || !s.inventories.get(&c.binding).is_some_and(|i| {
                i.good == Good::Binding
                    && i.quantity.0 == 1_000
                    && i.location == InventoryLocation::Area(c.area)
            })
        {
            return Err(fail());
        }
    }
    Ok(())
}
