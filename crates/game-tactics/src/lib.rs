//! Persistent tactical encounters, compact geometry indexes and atomic movement.
pub use game_tactical_map::{Map, Obstacle, Point, Rules};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
mod geometry;
mod membership;
mod movement;
pub struct Index {
    battles: Vec<(u64, IndexedBattle)>,
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
        use std::collections::{HashMap, HashSet};
        let mut regions = HashMap::new();
        for map in &self.maps {
            map.validate()?;
            if regions.insert(map.region, map).is_some() {
                return Err("duplicate tactical map");
            }
        }
        if self
            .battles
            .par_iter()
            .any(|battle| !regions.contains_key(&battle.region))
        {
            return Err("missing tactical map");
        }
        let mut battles: Vec<_> = self
            .battles
            .par_iter()
            .map(|battle| battle.encounter)
            .collect();
        if battles.par_windows(2).any(|p| p[0] > p[1]) {
            battles.par_sort_unstable();
        }
        if battles.par_windows(2).any(|p| p[0] == p[1]) {
            return Err("invalid tactical placement");
        }
        let mut participants: Vec<_> = if self
            .battles
            .par_iter()
            .any(|battle| battle.people.len() >= 4096)
        {
            self.battles
                .par_iter()
                .flat_map(|battle| battle.people.par_iter().map(|p| p.person))
                .collect()
        } else {
            self.battles
                .par_iter()
                .flat_map_iter(|battle| battle.people.iter().map(|p| p.person))
                .collect()
        };
        if participants.par_windows(2).any(|p| p[0] > p[1]) {
            participants.par_sort_unstable();
        }
        if participants.par_windows(2).any(|p| p[0] == p[1]) {
            return Err("invalid tactical placement");
        }
        let invalid = self.battles.par_iter().any(|battle| {
            let map = regions[&battle.region];
            let bad = |person: &Placement| {
                !map.open(person.position) || person.last_move.is_some_and(|tick| tick > now)
            };
            if battle.people.len() <= 32 {
                battle.people.iter().enumerate().any(|(i, person)| {
                    bad(person)
                        || battle.people[..i]
                            .iter()
                            .any(|other| other.position == person.position)
                })
            } else if battle.people.len() >= 4096 {
                let mut positions: Vec<_> = battle
                    .people
                    .par_iter()
                    .map(|p| (p.position.x, p.position.y))
                    .collect();
                if positions.par_windows(2).any(|pair| pair[0] > pair[1]) {
                    positions.par_sort_unstable();
                }
                battle.people.par_iter().any(bad) || positions.par_windows(2).any(|p| p[0] == p[1])
            } else {
                let mut positions = HashSet::new();
                battle.people.iter().any(|person| {
                    bad(person) || !positions.insert((person.position.x, person.position.y))
                })
            }
        });
        if invalid {
            return Err("invalid tactical placement");
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
        conflict: &game_conflict::Conflict,
        regions: &std::collections::HashSet<u64>,
        now: u64,
    ) -> Result<(), String> {
        self.validate(now).map_err(str::to_owned)?;
        if self.maps.iter().any(|map| !regions.contains(&map.region))
            || self.battles.len() != conflict.encounters.len()
        {
            return Err("invalid tactical encounter set".into());
        }
        let mut battles: Vec<_> = self.battles.par_iter().collect();
        if battles
            .par_windows(2)
            .any(|pair| pair[0].encounter > pair[1].encounter)
        {
            battles.par_sort_unstable_by_key(|battle| battle.encounter);
        }
        let error = conflict
            .encounters
            .par_iter()
            .map(|fight| {
                let Some(battle) = battles
                    .binary_search_by_key(&fight.id, |battle| battle.encounter)
                    .ok()
                    .map(|index| battles[index])
                else {
                    return Some("missing tactical encounter");
                };
                if battle.region != fight.region || battle.people.len() != fight.fighters.len() {
                    return Some("invalid tactical encounter participants");
                }
                let missing = if battle.people.len() <= 32 {
                    fight
                        .fighters
                        .iter()
                        .any(|fighter| !battle.people.iter().any(|p| p.person == fighter.person))
                } else {
                    let people: std::collections::HashSet<_> = if battle.people.len() >= 4096 {
                        battle.people.par_iter().map(|p| p.person).collect()
                    } else {
                        battle.people.iter().map(|p| p.person).collect()
                    };
                    if fight.fighters.len() >= 4096 {
                        fight
                            .fighters
                            .par_iter()
                            .any(|fighter| !people.contains(&fighter.person))
                    } else {
                        fight
                            .fighters
                            .iter()
                            .any(|fighter| !people.contains(&fighter.person))
                    }
                };
                missing.then_some("invalid tactical encounter participants")
            })
            .find_first(Option::is_some)
            .flatten();
        if let Some(error) = error {
            return Err(error.into());
        }
        Ok(())
    }
    /// Retain local sites but remove participants and battles resolved at the boundary.
    pub fn sync(&mut self, conflict: &game_conflict::Conflict) {
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
