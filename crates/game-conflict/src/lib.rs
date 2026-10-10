//! Conflict-owned maps, encounters, and deterministic initiative rules.
use game_core::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
mod confinement;
pub use confinement::*;
mod navigation;
pub use navigation::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObstacleView {
    pub index: u32,
    pub minimum: Point,
    pub maximum: Point,
    pub integrity: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    pub encounter_limit: usize,
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
    pub location: Location,
    pub width: i64,
    pub height: i64,
    pub obstacles: Vec<ObstacleView>,
    pub exits: Vec<(AreaId, Point)>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fighter {
    pub actor: PersonId,
    pub joins_round: u64,
    pub team: u32,
    pub position: Point,
    pub surrendered: bool,
    pub withdrawn: bool,
    pub defending: bool,
    pub retreat: Option<AreaId>,
    pub known: BTreeMap<PersonId, Point>,
    pub search: SearchState,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Encounter {
    pub id: EncounterId,
    pub area: AreaId,
    pub location: Location,
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
    pub map: CombatMapId,
    pub binding: InventoryId,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub rules: Rules,
    pub profiles: BTreeMap<PersonId, Profile>,
    pub maps: BTreeMap<AreaId, Map>,
    pub road_maps: BTreeMap<CombatMapId, Map>,
    pub encounters: BTreeMap<EncounterId, Encounter>,
    pub captivity: BTreeMap<PersonId, Captivity>,
}

/// Physical facts supplied by the coordinator; social authority is evaluated
/// separately and never makes an active opponent physically capturable.
#[derive(Clone, Copy, Debug)]
pub struct TargetBody {
    pub alive: bool,
    pub health: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Strike {
    pub damage: u64,
    pub practice: game_skills::PracticeDelta,
}

impl Rules {
    pub fn capacity_valid(&self) -> bool {
        (2..=4096).contains(&self.encounter_limit)
    }
    pub fn can_fight(&self, body: TargetBody, captive: bool) -> bool {
        body.alive && body.health > self.incapacitation && !captive
    }
    pub fn strike_damage(&self, attack: u64, defending: bool) -> u64 {
        attack.saturating_sub(if defending { self.defence } else { 0 })
    }

    /// A resolved strike earns experience even when defence absorbs its damage.
    /// The caller applies both effects once after the actual initiative action.
    pub fn strike(
        &self,
        actor: PersonId,
        attack: u64,
        defending: bool,
        practice_ms: u64,
    ) -> Strike {
        Strike {
            damage: self.strike_damage(attack, defending),
            practice: game_skills::PracticeDelta {
                actor,
                skill: Skill::Combat,
                milliseconds: practice_ms,
                mastery_cap: None,
            },
        }
    }
}

impl Map {
    /// Stable initial placement inside actual observed geometry. Existing small
    /// encounter positions remain unchanged; larger groups wrap within bounds.
    pub fn deployment(&self, index: usize, team: u32) -> Option<Point> {
        if !(1..=1_000_000).contains(&self.width) || !(1..=1_000_000).contains(&self.height) {
            return None;
        }
        let x = (4_000 + if team == 0 { 0 } else { 2_000 }).min(self.width);
        let row = i64::try_from(index / 2).ok()?;
        let y = 8_000i64.checked_add(row.checked_mul(300)?)?;
        let preferred = Point { x, y };
        if self.clear(preferred, preferred) {
            return Some(preferred);
        }
        let wrapped = Point {
            x,
            y: y % (self.height + 1),
        };
        if self.clear(wrapped, wrapped) {
            return Some(wrapped);
        }
        for offset in 0..64 {
            let cell = (index % 64 + offset) % 64;
            let p = Point {
                x: self.width * (cell % 8 + 1) as i64 / 9,
                y: self.height * (cell / 8 + 1) as i64 / 9,
            };
            if self.clear(p, p) {
                return Some(p);
            }
        }
        [
            Point { x: 0, y: 0 },
            Point {
                x: self.width,
                y: 0,
            },
            Point {
                x: 0,
                y: self.height,
            },
            Point {
                x: self.width,
                y: self.height,
            },
        ]
        .into_iter()
        .find(|p| self.clear(*p, *p))
    }
    pub fn contains(&self, p: Point) -> bool {
        p.x >= 0 && p.y >= 0 && p.x <= self.width && p.y <= self.height
    }
    pub fn clear(&self, a: Point, b: Point) -> bool {
        self.contains(a)
            && self.contains(b)
            && self
                .obstacles
                .iter()
                .filter(|o| o.integrity > 0)
                .all(|o| !crosses(a, b, o.minimum, o.maximum))
    }
}
pub fn distance(a: Point, b: Point) -> u128 {
    let x = (i128::from(a.x) - i128::from(b.x)).unsigned_abs();
    let y = (i128::from(a.y) - i128::from(b.y)).unsigned_abs();
    x.saturating_mul(x).saturating_add(y.saturating_mul(y))
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

impl World {
    pub fn map(&self, id: CombatMapId) -> Option<&Map> {
        self.road_maps
            .get(&id)
            .or_else(|| self.maps.values().find(|map| map.id == id))
    }
    pub fn map_mut(&mut self, id: CombatMapId) -> Option<&mut Map> {
        if self.road_maps.contains_key(&id) {
            self.road_maps.get_mut(&id)
        } else {
            self.maps.values_mut().find(|map| map.id == id)
        }
    }
    pub fn turn_position(
        &self,
        encounter: EncounterId,
        actor: PersonId,
        movement: Option<Point>,
    ) -> Result<Point, Error> {
        let encounter = self.encounters.get(&encounter).ok_or(Error::Unavailable)?;
        if !encounter.active || encounter.order.get(encounter.cursor) != Some(&actor) {
            return Err(Error::Unavailable);
        }
        let fighter = encounter.fighters.get(&actor).ok_or(Error::Unavailable)?;
        let map = self.map(encounter.map).ok_or(Error::Unavailable)?;
        let position = movement.unwrap_or(fighter.position);
        if distance(position, fighter.position) > u128::from(self.rules.movement).pow(2)
            || !map.clear(fighter.position, position)
        {
            return Err(Error::Unavailable);
        }
        Ok(position)
    }
    pub fn reachable(&self, encounter: EncounterId, position: Point, target: PersonId) -> bool {
        self.encounters.get(&encounter).is_some_and(|encounter| {
            encounter.fighters.get(&target).is_some_and(|fighter| {
                !fighter.withdrawn
                    && distance(position, fighter.position) <= u128::from(self.rules.reach).pow(2)
                    && self
                        .map(encounter.map)
                        .is_some_and(|map| map.clear(position, fighter.position))
            })
        })
    }
    pub fn capture_possible(
        &self,
        encounter: EncounterId,
        actor: PersonId,
        position: Point,
        target: PersonId,
        body: TargetBody,
        has_binding: bool,
    ) -> bool {
        actor != target
            && has_binding
            && body.alive
            && !self.captivity.contains_key(&target)
            && self.reachable(encounter, position, target)
            && self.encounters[&encounter]
                .fighters
                .get(&target)
                .is_some_and(|fighter| fighter.surrendered || !self.rules.can_fight(body, false))
    }
    pub fn initiative_order(
        &self,
        encounter: EncounterId,
        fatigue: &[(PersonId, u64)],
    ) -> Result<Vec<PersonId>, Error> {
        let e = self.encounters.get(&encounter).ok_or(Error::Unavailable)?;
        let fatigue: BTreeMap<_, _> = fatigue.iter().copied().collect();
        let mut ranked = Vec::new();
        for f in e
            .fighters
            .values()
            .filter(|f| !f.withdrawn && f.joins_round <= e.round)
        {
            let profile = self.profiles.get(&f.actor).ok_or(Error::Unavailable)?;
            let fatigue = fatigue.get(&f.actor).ok_or(Error::Unavailable)?;
            ranked.push((
                std::cmp::Reverse(profile.initiative.saturating_sub(fatigue / 1_000)),
                f.actor,
            ));
        }
        ranked.sort_unstable();
        Ok(ranked.into_iter().map(|(_, actor)| actor).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_actual_defended_strike_supplies_damage_and_independent_combat_practice() {
        let rules = Rules {
            encounter_limit: 128,
            round: SimDuration(6_000),
            movement: 5_000,
            reach: 2_000,
            damage: 50,
            defence: 30,
            incapacitation: 20,
        };
        let strike = rules.strike(PersonId(1), rules.defence, true, 6_000);
        assert_eq!(strike.damage, 0);
        assert_eq!(strike.practice.skill, Skill::Combat);
        let mut skills = game_skills::World::default();
        skills.apply_practice(strike.practice);
        assert_eq!(skills.practice(PersonId(1), Skill::Combat), 6_000);
        assert_eq!(skills.practice(PersonId(2), Skill::Combat), 0);
    }
    #[test]
    fn large_deployments_remain_inside_clear_geometry_and_blocked_maps_have_no_spawn() {
        let mut scene = Map {
            id: CombatMapId(1),
            version: 1,
            area: AreaId(1),
            location: Location::AtArea(AreaId(1)),
            width: 24_000,
            height: 16_000,
            obstacles: vec![],
            exits: vec![],
        };
        for index in 0..4096 {
            let p = scene.deployment(index, (index % 2) as u32).unwrap();
            assert!(scene.clear(p, p));
            assert_eq!(scene.deployment(index, (index % 2) as u32), Some(p));
            if index < 32 {
                assert_eq!(
                    p,
                    Point {
                        x: 4_000 + if index % 2 == 0 { 0 } else { 2_000 },
                        y: 8_000 + (index as i64 / 2) * 300
                    }
                );
            }
        }
        scene.obstacles = vec![ObstacleView {
            index: 0,
            minimum: Point { x: 0, y: 0 },
            maximum: Point {
                x: 24_000,
                y: 16_000,
            },
            integrity: 1,
        }];
        assert_eq!(scene.deployment(0, 0), None);
        scene.obstacles[0].integrity = 0;
        assert!(scene.deployment(65, 1).is_some());
        scene.height = i64::MAX;
        assert_eq!(scene.deployment(65, 1), None);
    }
    fn map() -> Map {
        Map {
            id: CombatMapId(1),
            version: 1,
            area: AreaId(2),
            location: Location::AtArea(AreaId(2)),
            width: 20_000,
            height: 20_000,
            obstacles: vec![ObstacleView {
                index: 0,
                minimum: Point { x: 5_000, y: 4_000 },
                maximum: Point { x: 6_000, y: 8_000 },
                integrity: 10,
            }],
            exits: vec![],
        }
    }
    #[test]
    fn obstacles_stop_reach_until_the_owned_map_records_actual_damage() {
        let mut map = map();
        let (from, to) = (Point { x: 3_000, y: 5_000 }, Point { x: 8_000, y: 5_000 });
        assert!(!map.clear(from, to));
        map.obstacles[0].integrity = 0;
        assert!(map.clear(from, to));
    }
    #[test]
    fn continuous_geometry_handles_boundary_touch_and_rejects_outside_points() {
        let map = map();
        assert!(!map.clear(
            Point { x: 0, y: 4_000 },
            Point {
                x: 10_000,
                y: 4_000
            }
        ));
        assert!(map.clear(
            Point { x: 0, y: 3_999 },
            Point {
                x: 10_000,
                y: 3_999
            }
        ));
        assert!(!map.clear(Point { x: -1, y: 0 }, Point { x: 1, y: 0 }));
        assert_eq!(distance(Point { x: 0, y: 0 }, Point { x: 3, y: 4 }), 25);
    }
    #[test]
    fn initiative_is_stable_and_a_late_arrival_waits_for_its_eligible_round() {
        let mut fighters = BTreeMap::new();
        for (actor, joins_round) in [(PersonId(5), 0), (PersonId(3), 0), (PersonId(4), 2)] {
            fighters.insert(
                actor,
                Fighter {
                    actor,
                    joins_round,
                    team: 0,
                    position: Point { x: 0, y: 0 },
                    surrendered: false,
                    withdrawn: false,
                    defending: false,
                    retreat: None,
                    known: BTreeMap::new(),
                    search: SearchState::default(),
                },
            );
        }
        let id = EncounterId(10);
        let mut world = World {
            rules: Rules {
                encounter_limit: 128,
                round: SimDuration(6_000),
                movement: 5_000,
                reach: 2_000,
                damage: 1,
                defence: 1,
                incapacitation: 1,
            },
            profiles: fighters
                .keys()
                .map(|p| (*p, Profile { initiative: 100 }))
                .collect(),
            maps: BTreeMap::new(),
            road_maps: BTreeMap::new(),
            captivity: BTreeMap::new(),
            encounters: BTreeMap::from([(
                id,
                Encounter {
                    id,
                    area: AreaId(2),
                    location: Location::AtArea(AreaId(2)),
                    map: CombatMapId(1),
                    round: 1,
                    started: SimTime(0),
                    ends: SimTime(6_000),
                    fighters,
                    order: vec![],
                    cursor: 0,
                    active: true,
                },
            )]),
        };
        let fatigue = [(PersonId(5), 0), (PersonId(3), 0), (PersonId(4), 0)];
        assert_eq!(
            world.initiative_order(id, &fatigue).unwrap(),
            vec![PersonId(3), PersonId(5)]
        );
        world.encounters.get_mut(&id).unwrap().round = 2;
        assert_eq!(
            world.initiative_order(id, &fatigue).unwrap(),
            vec![PersonId(3), PersonId(4), PersonId(5)]
        );
        assert_eq!(
            world.initiative_order(id, &[(PersonId(5), 0)]),
            Err(Error::Unavailable)
        );
    }
    #[test]
    fn capture_requires_physical_surrender_or_incapacitation_and_a_real_binding() {
        let actor = PersonId(3);
        let target = PersonId(5);
        let id = EncounterId(10);
        let fighters = [actor, target]
            .into_iter()
            .map(|actor| {
                (
                    actor,
                    Fighter {
                        actor,
                        joins_round: 0,
                        team: 0,
                        position: Point { x: 100, y: 100 },
                        surrendered: false,
                        withdrawn: false,
                        defending: false,
                        retreat: None,
                        known: BTreeMap::new(),
                        search: SearchState::default(),
                    },
                )
            })
            .collect();
        let mut world = World {
            rules: Rules {
                encounter_limit: 128,
                round: SimDuration(6_000),
                movement: 5_000,
                reach: 2_000,
                damage: 50,
                defence: 30,
                incapacitation: 20,
            },
            profiles: BTreeMap::new(),
            maps: BTreeMap::from([(AreaId(2), map())]),
            road_maps: BTreeMap::new(),
            captivity: BTreeMap::new(),
            encounters: BTreeMap::from([(
                id,
                Encounter {
                    id,
                    area: AreaId(2),
                    location: Location::AtArea(AreaId(2)),
                    map: CombatMapId(1),
                    round: 1,
                    started: SimTime(0),
                    ends: SimTime(6_000),
                    fighters,
                    order: vec![actor],
                    cursor: 0,
                    active: true,
                },
            )]),
        };
        let position = world
            .turn_position(id, actor, Some(Point { x: 200, y: 100 }))
            .unwrap();
        let body = TargetBody {
            alive: true,
            health: 100,
        };
        assert!(!world.capture_possible(id, actor, position, target, body, true));
        world
            .encounters
            .get_mut(&id)
            .unwrap()
            .fighters
            .get_mut(&target)
            .unwrap()
            .surrendered = true;
        assert!(world.capture_possible(id, actor, position, target, body, true));
        assert!(!world.capture_possible(id, actor, position, target, body, false));
        assert!(!world.capture_possible(
            id,
            actor,
            position,
            target,
            TargetBody {
                alive: false,
                health: 0
            },
            true
        ));
        world
            .encounters
            .get_mut(&id)
            .unwrap()
            .fighters
            .get_mut(&target)
            .unwrap()
            .surrendered = false;
        assert!(world.capture_possible(
            id,
            actor,
            position,
            target,
            TargetBody {
                alive: true,
                health: 20
            },
            true
        ));
        assert_eq!(world.rules.strike_damage(50, true), 20);
        assert_eq!(world.rules.strike_damage(10, true), 0);
        assert_eq!(
            world.turn_position(id, actor, Some(Point { x: 8_000, y: 100 })),
            Err(Error::Unavailable)
        );
        assert_eq!(
            world.turn_position(id, target, None),
            Err(Error::Unavailable)
        );
    }
}
