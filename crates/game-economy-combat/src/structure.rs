//! Tactical strikes against explicitly mapped physical work sites.
use super::{Combat, Draft, Obstacle, Order, Plan, Point, Runtime};
use game_economy_person::Person;
use game_opinions::Opinions;
use game_production_model::{PersonSnapshot, person_index};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

fn barrier_key(region: u64, wall: Obstacle) -> (u64, u32, u32, u32, u32) {
    (region, wall.min.x, wall.min.y, wall.max.x, wall.max.y)
}

/// A site's attackable point on its persistent local tactical map.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub site: u64,
    pub region: u64,
    pub position: Point,
    /// If enabled, combat damage at the wear limit destroys the site.
    #[serde(default)]
    pub collapses_at_limit: bool,
    /// An obstacle physically belonging to this site, removed with it.
    #[serde(default)]
    pub barrier: Option<Obstacle>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub encounter: u64,
    pub attacker: u64,
    pub site: u64,
    pub damage: u32,
    #[serde(default)]
    pub collapsed: bool,
}

/// A person's choice to damage a particular site while fighting.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    pub person: u64,
    pub site: u64,
    #[serde(default)]
    pub willing: bool,
    #[serde(default)]
    pub pressure_from: Option<u64>,
    #[serde(default)]
    pub fear_at_least: Option<u64>,
    #[serde(default)]
    pub obligation_at_least: Option<u64>,
}

#[derive(Clone, Copy)]
pub(crate) struct Intent {
    encounter: u64,
    region: u64,
    pub(crate) attacker: u64,
    site: u64,
}

#[derive(Clone, Copy)]
pub(crate) struct Candidate {
    intent: Intent,
    preference: usize,
    position: Point,
}

pub(crate) fn validate_preferences(
    rows: &[Preference],
    people: &std::collections::HashSet<u64>,
) -> Result<(), String> {
    if rows
        .par_windows(2)
        .any(|pair| pair[0].person >= pair[1].person)
        || rows.par_iter().any(|row| {
            !people.contains(&row.person)
                || row.site == 0
                || row.pressure_from.is_some_and(|id| !people.contains(&id))
                || ((row.fear_at_least.is_some() || row.obligation_at_least.is_some())
                    && row.pressure_from.is_none())
        })
    {
        return Err("invalid personal site attack preference".into());
    }
    Ok(())
}

pub(crate) fn candidates(state: &Combat) -> Vec<Candidate> {
    if state.structure_preferences.is_empty() || state.structures.is_empty() {
        return Vec::new();
    }
    state
        .conflict
        .encounters
        .par_iter()
        .flat_map(|fight| {
            fight.fighters.par_iter().filter_map(move |fighter| {
                if !matches!(fighter.order, Order::Attack { .. }) {
                    return None;
                }
                let preference = state
                    .structure_preferences
                    .binary_search_by_key(&fighter.person, |row| row.person)
                    .ok()?;
                let site = state.structure_preferences[preference].site;
                let target = state
                    .structures
                    .binary_search_by_key(&site, |row| row.site)
                    .ok()?;
                (state.structures[target].region == fight.region).then_some(Candidate {
                    intent: Intent {
                        encounter: fight.id,
                        region: fight.region,
                        attacker: fighter.person,
                        site,
                    },
                    preference,
                    position: state.structures[target].position,
                })
            })
        })
        .collect()
}

pub(crate) fn choose(
    runtime: &Runtime,
    people: &[Person],
    opinions: Option<&Opinions>,
    retreating: &[u64],
) -> Vec<Intent> {
    if runtime.structure_candidates.is_empty() {
        return Vec::new();
    }
    let needs_feelings = runtime.structure_candidates.par_iter().any(|candidate| {
        let row = &runtime.state.structure_preferences[candidate.preference];
        !row.willing && row.pressure_from.is_some()
    });
    let feelings = opinions.filter(|_| needs_feelings).map(Opinions::index);
    let dense = people.first().zip(people.last()).and_then(|(first, last)| {
        (last.id.checked_sub(first.id) == Some((people.len() - 1) as u64)).then_some(first.id)
    });
    runtime
        .structure_candidates
        .par_iter()
        .filter_map(|candidate| {
            let intent = candidate.intent;
            if retreating.binary_search(&intent.attacker).is_ok() {
                return None;
            }
            if !runtime.index.can_attack_point(
                intent.encounter,
                intent.attacker,
                candidate.position,
            ) {
                return None;
            }
            let index = dense
                .and_then(|first| intent.attacker.checked_sub(first))
                .and_then(|offset| usize::try_from(offset).ok())
                .filter(|&index| index < people.len())
                .or_else(|| {
                    dense
                        .is_none()
                        .then(|| {
                            people
                                .binary_search_by_key(&intent.attacker, |person| person.id)
                                .ok()
                        })
                        .flatten()
                })?;
            let person = &people[index];
            if person.controlled || person.health == 0 {
                return None;
            }
            let row = &runtime.state.structure_preferences[candidate.preference];
            let motivated = row.willing
                || row
                    .pressure_from
                    .zip(feelings.as_ref())
                    .is_some_and(|(source, index)| {
                        row.fear_at_least
                            .is_some_and(|n| index.fear(row.person, source) >= n)
                            || row
                                .obligation_at_least
                                .is_some_and(|n| index.obligation(row.person, source) >= n)
                    });
            motivated.then_some(intent)
        })
        .collect()
}

pub(crate) fn validate(state: &Combat) -> Result<(), String> {
    if state.structures.is_empty() {
        return if state.conflict.encounters.par_iter().any(|fight| {
            if fight.fighters.len() >= 4096 {
                fight
                    .fighters
                    .par_iter()
                    .any(|fighter| matches!(fighter.order, Order::AttackSite { .. }))
            } else {
                fight
                    .fighters
                    .iter()
                    .any(|fighter| matches!(fighter.order, Order::AttackSite { .. }))
            }
        }) {
            Err("invalid tactical site target".into())
        } else {
            Ok(())
        };
    }
    let maps: HashMap<_, _> = state
        .tactics
        .maps
        .par_iter()
        .map(|map| (map.region, map))
        .collect();
    let mut barriers: Vec<_> = state
        .structures
        .par_iter()
        .filter_map(|target| target.barrier.map(|wall| barrier_key(target.region, wall)))
        .collect();
    barriers.par_sort_unstable();
    let mut obstacles: Vec<_> = if barriers.is_empty() {
        Vec::new()
    } else {
        state
            .tactics
            .maps
            .par_iter()
            .flat_map(|map| {
                map.obstacles
                    .par_iter()
                    .map(move |&wall| barrier_key(map.region, wall))
            })
            .collect()
    };
    obstacles.par_sort_unstable();
    if state
        .structures
        .par_windows(2)
        .any(|pair| pair[0].site >= pair[1].site)
        || barriers.par_windows(2).any(|pair| pair[0] == pair[1])
        || state.structures.par_iter().any(|target| {
            target.site == 0
                || maps.get(&target.region).is_none_or(|map| {
                    !map.open(target.position)
                        || target.barrier.is_some_and(|wall| {
                            let key = barrier_key(target.region, wall);
                            obstacles.partition_point(|item| *item <= key)
                                - obstacles.partition_point(|item| *item < key)
                                != 1
                        })
                })
        })
        || state.conflict.encounters.par_iter().any(|fight| {
            fight
                .fighters
                .par_iter()
                .any(|fighter| match fighter.order {
                    Order::AttackSite { site } => state
                        .structures
                        .binary_search_by_key(&site, |target| target.site)
                        .ok()
                        .is_none_or(|i| state.structures[i].region != fight.region),
                    _ => false,
                })
        })
    {
        return Err("invalid tactical site target".into());
    }
    Ok(())
}

pub(crate) fn orders(state: &Combat) -> Vec<Intent> {
    if state.structures.is_empty() {
        return Vec::new();
    }
    state
        .conflict
        .encounters
        .par_iter()
        .flat_map(|fight| {
            fight.fighters.par_iter().filter_map(move |fighter| {
                let Order::AttackSite { site } = fighter.order else {
                    return None;
                };
                Some(Intent {
                    encounter: fight.id,
                    region: fight.region,
                    attacker: fighter.person,
                    site,
                })
            })
        })
        .collect()
}

pub(crate) fn strikes(
    runtime: &Runtime,
    people: &[PersonSnapshot],
    retreating: &[u64],
    autonomous: &[Intent],
    tick: u64,
) -> Vec<Event> {
    if runtime.structure_orders.is_empty() && autonomous.is_empty() {
        return Vec::new();
    }
    let mut events: Vec<_> = runtime
        .structure_orders
        .par_iter()
        .chain(autonomous.par_iter())
        .filter_map(|intent| {
            if retreating.binary_search(&intent.attacker).is_ok() {
                return None;
            }
            let person = people[person_index(people, intent.attacker)?];
            if person.1 != Some(intent.region) || person.2 == 0 {
                return None;
            }
            let targets = &runtime.state.structures;
            let first = targets.first()?.site;
            let index = targets
                .last()
                .and_then(|last| {
                    (last.site.checked_sub(first) == Some((targets.len() - 1) as u64))
                        .then(|| {
                            intent
                                .site
                                .checked_sub(first)
                                .and_then(|offset| usize::try_from(offset).ok())
                        })
                        .flatten()
                        .filter(|&index| index < targets.len())
                })
                .or_else(|| {
                    targets
                        .binary_search_by_key(&intent.site, |target| target.site)
                        .ok()
                })?;
            let target = &targets[index];
            runtime
                .index
                .can_attack_point(intent.encounter, intent.attacker, target.position)
                .then_some(Event {
                    tick,
                    encounter: intent.encounter,
                    attacker: intent.attacker,
                    site: intent.site,
                    damage: runtime.state.conflict.damage,
                    collapsed: false,
                })
        })
        .collect();
    if events.par_windows(2).any(|pair| {
        (pair[0].site, pair[0].encounter, pair[0].attacker)
            > (pair[1].site, pair[1].encounter, pair[1].attacker)
    }) {
        events.par_sort_unstable_by_key(|event| (event.site, event.encounter, event.attacker));
    }
    events
}

impl Draft {
    /// Remove tactical references to sites that no longer exist after the
    /// physical boundary. Fighters with a vanished target revert to defense.
    pub fn retain_structures(&mut self, sites: &[u64]) -> Result<(), String> {
        if self
            .state
            .structures
            .par_iter()
            .all(|target| sites.binary_search(&target.site).is_ok())
        {
            return Ok(());
        }
        let removed: HashSet<_> = self
            .state
            .structures
            .par_iter()
            .filter(|target| sites.binary_search(&target.site).is_err())
            .filter_map(|target| target.barrier.map(|wall| barrier_key(target.region, wall)))
            .collect();
        if !removed.is_empty() {
            self.state.tactics.maps.par_iter_mut().for_each(|map| {
                map.obstacles = std::mem::take(&mut map.obstacles)
                    .into_par_iter()
                    .filter(|wall| !removed.contains(&barrier_key(map.region, *wall)))
                    .collect();
            });
        }
        self.state.structures = self
            .state
            .structures
            .par_iter()
            .filter(|target| sites.binary_search(&target.site).is_ok())
            .cloned()
            .collect();
        self.state
            .conflict
            .encounters
            .par_iter_mut()
            .for_each(|fight| {
                fight.fighters.par_iter_mut().for_each(|fighter| {
                    if let Order::AttackSite { site } = fighter.order
                        && sites.binary_search(&site).is_err()
                    {
                        fighter.order = Order::Defend;
                    }
                });
            });
        self.replacement = Some((
            Some(Plan::compile(&self.state.conflict).map_err(str::to_owned)?),
            self.state.tactics.index().map_err(str::to_owned)?,
            Some(self.state.conflict.actors()),
        ));
        Ok(())
    }
}
