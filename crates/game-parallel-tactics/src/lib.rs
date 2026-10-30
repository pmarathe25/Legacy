//! Persistent tactical encounters, compact geometry indexes and atomic movement.
pub use game_parallel_tactical_map::{Map, Obstacle, Point, Rules};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
mod geometry;
mod membership;
mod movement;
pub struct Index {
    battles: std::collections::HashMap<u64, IndexedBattle>,
    actors: Vec<Geometry>,
    maps: Vec<Map>,
}
struct Geometry {
    id: u64,
    encounter: u64,
    map: usize,
    point: Point,
    active: bool,
}
struct IndexedBattle {
    map: usize,
    slot: usize,
    slots: Option<std::collections::HashMap<u64, usize>>,
    occupied: std::collections::HashMap<(u32, u32), u64>,
}
/// A persistent local site and per-encounter positions within it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tactics {
    pub maps: Vec<Map>,
    #[serde(default)]
    pub battles: Vec<Battle>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Battle {
    pub encounter: u64,
    pub region: u64,
    pub people: Vec<Placement>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    pub person: u64,
    pub position: Point,
    #[serde(default)]
    pub last_move: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Move {
    pub encounter: u64,
    pub person: u64,
    pub destination: Point,
}
impl Tactics {
    pub fn validate(&self, now: u64) -> Result<(), &'static str> {
        use std::collections::HashSet;
        let mut regions = HashSet::new();
        for map in &self.maps {
            map.validate()?;
            if !regions.insert(map.region) {
                return Err("duplicate tactical map");
            }
        }
        let mut battles = HashSet::new();
        let mut participants = HashSet::new();
        for battle in &self.battles {
            let map = self
                .maps
                .iter()
                .find(|map| map.region == battle.region)
                .ok_or("missing tactical map")?;
            let mut locations = HashSet::new();
            if !battles.insert(battle.encounter)
                || battle.people.iter().any(|person| {
                    !participants.insert(person.person)
                        || !map.open(person.position)
                        || !locations.insert((person.position.x, person.position.y))
                        || person.last_move.is_some_and(|tick| tick > now)
                })
            {
                return Err("invalid tactical placement");
            }
        }
        Ok(())
    }
    /// All movement commands commit together. A fighter may move at most once
    /// per tick, and simultaneous swaps are allowed when final positions differ.
    pub fn move_batch(&self, moves: &[Move], tick: u64) -> Result<Self, &'static str> {
        let mut next = self.clone();
        self.index()?.move_batch(&mut next, moves, tick)?;
        Ok(next)
    }
    pub fn can_attack(&self, encounter: u64, attacker: u64, target: u64) -> bool {
        let Some(battle) = self
            .battles
            .iter()
            .find(|battle| battle.encounter == encounter)
        else {
            return false;
        };
        let Some(map) = self.maps.iter().find(|map| map.region == battle.region) else {
            return false;
        };
        let Some(from) = battle
            .people
            .iter()
            .find(|person| person.person == attacker)
        else {
            return false;
        };
        let Some(to) = battle.people.iter().find(|person| person.person == target) else {
            return false;
        };
        map.can_attack(from.position, to.position)
    }
}

#[cfg(test)]
#[path = "../tests/support/arena.rs"]
mod arena_tests;

impl Tactics {
    /// Check maps and participant placement against authoritative encounters.
    pub fn validate_world(
        &self,
        conflict: &game_parallel_conflict::Conflict,
        regions: &std::collections::HashSet<u64>,
        now: u64,
    ) -> Result<(), String> {
        self.validate(now).map_err(str::to_owned)?;
        if self.maps.iter().any(|map| !regions.contains(&map.region))
            || self.battles.len() != conflict.encounters.len()
        {
            return Err("invalid tactical encounter set".into());
        }
        for fight in &conflict.encounters {
            let battle = self
                .battles
                .iter()
                .find(|battle| battle.encounter == fight.id)
                .ok_or("missing tactical encounter")?;
            if battle.region != fight.region || battle.people.len() != fight.fighters.len() {
                return Err("invalid tactical encounter participants".into());
            }
            let people: std::collections::HashSet<_> =
                battle.people.iter().map(|person| person.person).collect();
            if fight
                .fighters
                .iter()
                .any(|fighter| !people.contains(&fighter.person))
            {
                return Err("invalid tactical encounter participants".into());
            }
        }
        Ok(())
    }
    /// Retain local sites but remove participants and battles resolved at the boundary.
    pub fn sync(&mut self, conflict: &game_parallel_conflict::Conflict) {
        let active: std::collections::HashMap<_, std::collections::HashSet<_>> = conflict
            .encounters
            .par_iter()
            .map(|fight| {
                (
                    fight.id,
                    fight
                        .fighters
                        .par_iter()
                        .map(|fighter| fighter.person)
                        .collect(),
                )
            })
            .collect();
        self.battles.par_iter_mut().for_each(|battle| {
            if let Some(people) = active.get(&battle.encounter) {
                battle
                    .people
                    .retain(|person| people.contains(&person.person));
            }
        });
        self.battles
            .retain(|battle| active.contains_key(&battle.encounter));
    }
}
