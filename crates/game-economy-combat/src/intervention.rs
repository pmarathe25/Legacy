//! Individual, voluntary or personally motivated reinforcement of a local fight.
use super::{Draft, Fighter, Order, Placement, Plan, Point, Runtime};
use game_economy_person::Person;
use game_lots::View;
use game_opinions::Opinions;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// A person's standing decision to intervene for a named person against another.
/// The observer witnesses a fight only when an open entry point has line of sight
/// to the beneficiary. Pressure is a directed feeling, never a command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    pub person: u64,
    pub beneficiary: u64,
    pub opponent: u64,
    #[serde(default)]
    pub willing: bool,
    #[serde(default)]
    pub pressure_from: Option<u64>,
    #[serde(default)]
    pub fear_at_least: Option<u64>,
    #[serde(default)]
    pub obligation_at_least: Option<u64>,
}

/// Outcome of a spent intervention attempt in the most recent boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub person: u64,
    pub encounter: u64,
    pub admitted: bool,
}

pub(crate) fn validate(rows: &[Preference], ids: &HashSet<u64>) -> Result<(), String> {
    let mut actors: Vec<_> = rows.par_iter().map(|row| row.person).collect();
    actors.par_sort_unstable();
    if actors.par_windows(2).any(|pair| pair[0] == pair[1])
        || rows.par_iter().any(|row| {
            !ids.contains(&row.person)
                || !ids.contains(&row.beneficiary)
                || !ids.contains(&row.opponent)
                || row.person == row.beneficiary
                || row.person == row.opponent
                || row.beneficiary == row.opponent
                || row.pressure_from.is_some_and(|id| !ids.contains(&id))
                || ((row.fear_at_least.is_some() || row.obligation_at_least.is_some())
                    && row.pressure_from.is_none())
        })
    {
        return Err("invalid personal intervention preference".into());
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Choice {
    actor: u64,
    beneficiary: u64,
    opponent: u64,
    encounter: u64,
    region: u64,
    side: u32,
}

pub struct Selection {
    choices: Vec<Choice>,
    pub actors: Vec<u64>,
}

/// Evaluate choices against the opening generation; the caller reserves actors.
pub fn select(
    runtime: Option<&Runtime>,
    opinions: Option<&Opinions>,
    people: &[Person],
    unavailable: &[u64],
) -> Option<Selection> {
    let runtime = runtime?;
    if runtime.state.interventions.is_empty() || runtime.state.conflict.encounters.is_empty() {
        return None;
    }
    let fighters: HashMap<_, _> = runtime
        .state
        .conflict
        .encounters
        .par_iter()
        .flat_map_iter(|fight| {
            fight
                .fighters
                .iter()
                .map(move |f| (f.person, (fight.id, fight.region, f.side)))
        })
        .collect();
    let feelings = opinions.map(Opinions::index);
    let immobilized: HashSet<_> = runtime
        .state
        .injuries
        .as_ref()
        .filter(|injuries| injuries.rules.immobilizes)
        .map_or_else(HashSet::new, |injuries| {
            injuries.cases.par_iter().map(|case| case.person).collect()
        });
    let mut choices: Vec<_> = runtime
        .state
        .interventions
        .par_iter()
        .filter_map(|row| {
            let person = &people[people.binary_search_by_key(&row.person, |p| p.id).ok()?];
            if person.controlled
                || person.health == 0
                || unavailable.binary_search(&row.person).is_ok()
                || runtime.actors().binary_search(&row.person).is_ok()
                || immobilized.contains(&row.person)
            {
                return None;
            }
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
            if !motivated {
                return None;
            }
            let &(encounter, region, side) = fighters.get(&row.beneficiary)?;
            let &(other_encounter, _, other_side) = fighters.get(&row.opponent)?;
            (person.region == Some(region) && encounter == other_encounter && side != other_side)
                .then_some(Choice {
                    actor: row.person,
                    beneficiary: row.beneficiary,
                    opponent: row.opponent,
                    encounter,
                    region,
                    side,
                })
        })
        .collect();
    if choices.is_empty() {
        return None;
    }
    choices.par_sort_unstable_by_key(|row| (row.encounter, row.actor));
    let mut actors: Vec<_> = choices.par_iter().map(|row| row.actor).collect();
    actors.par_sort_unstable();
    Some(Selection { choices, actors })
}

impl Runtime {
    /// Choose to join an already visible local fight; this creates no round.
    pub fn reinforce(
        &self,
        people: &[Person],
        view: &View,
        tick: u64,
        actor: u64,
        beneficiary: u64,
        opponent: u64,
    ) -> Result<Self, String> {
        if actor == beneficiary || actor == opponent || beneficiary == opponent {
            return Err("reinforcement requires three distinct people".into());
        }
        if self.actors().binary_search(&actor).is_ok() {
            return Err("person is already fighting".into());
        }
        if self
            .state
            .injuries
            .as_ref()
            .is_some_and(|injuries| injuries.immobilized(actor))
        {
            return Err("a wound prevents tactical entry".into());
        }
        let person = people
            .binary_search_by_key(&actor, |p| p.id)
            .ok()
            .map(|i| &people[i])
            .ok_or("unknown person")?;
        let region = person.region.ok_or("person is traveling")?;
        if person.health == 0 {
            return Err("person cannot fight while incapacitated".into());
        }
        let fight = self
            .state
            .conflict
            .encounters
            .iter()
            .find(|fight| {
                fight.region == region && fight.fighters.iter().any(|f| f.person == beneficiary)
            })
            .ok_or("no local fight involving that person")?;
        let side = fight
            .fighters
            .iter()
            .find(|f| f.person == beneficiary)
            .expect("selected fighter")
            .side;
        if fight
            .fighters
            .iter()
            .find(|f| f.person == opponent)
            .is_none_or(|f| f.side == side)
        {
            return Err("opponent is not on another side of this fight".into());
        }
        let battle = self
            .state
            .tactics
            .battles
            .iter()
            .find(|battle| battle.encounter == fight.id)
            .ok_or("missing tactical battle")?;
        let map = self
            .state
            .tactics
            .maps
            .iter()
            .find(|map| map.region == region)
            .ok_or("missing tactical map")?;
        let witness = battle
            .people
            .iter()
            .find(|p| p.person == beneficiary)
            .ok_or("missing fighter position")?
            .position;
        let occupied: HashSet<_> = battle
            .people
            .iter()
            .map(|p| (p.position.x, p.position.y))
            .collect();
        let point = (0..=map.rules.width.min(63))
            .flat_map(|x| (0..=map.rules.height.min(63)).map(move |y| Point { x, y }))
            .find(|p| !occupied.contains(&(p.x, p.y)) && map.visible(*p, witness))
            .ok_or("no visible open tactical entry")?;
        let encounter = fight.id;
        let mut state = self.state.clone();
        let fight = state
            .conflict
            .encounters
            .iter_mut()
            .find(|fight| fight.id == encounter)
            .expect("selected fight");
        fight.fighters.push(Fighter {
            person: actor,
            side,
            order: Order::Attack { target: opponent },
        });
        fight.fighters.sort_unstable_by_key(|f| f.person);
        let battle = state
            .tactics
            .battles
            .iter_mut()
            .find(|battle| battle.encounter == encounter)
            .expect("selected battle");
        battle.people.push(Placement {
            person: actor,
            position: point,
            last_move: None,
        });
        battle.people.sort_unstable_by_key(|p| p.person);
        Self::compile_known(state, people, view, tick, &self.goods)
    }
}

impl Draft {
    /// Admit surviving attempts atomically after the opening round. A failed
    /// attempt still consumed this boundary but does not join the encounter.
    pub fn add_interventions(
        &mut self,
        selection: &Selection,
        people: &[Person],
    ) -> Result<Vec<u64>, String> {
        let fights: HashMap<_, _> = self
            .state
            .conflict
            .encounters
            .par_iter()
            .map(|f| (f.id, f))
            .collect();
        let battles: HashMap<_, _> = self
            .state
            .tactics
            .battles
            .par_iter()
            .map(|b| (b.encounter, b))
            .collect();
        let maps: HashMap<_, _> = self
            .state
            .tactics
            .maps
            .par_iter()
            .map(|m| (m.region, m))
            .collect();
        let mut additions: Vec<_> = selection
            .choices
            .par_chunk_by(|a, b| a.encounter == b.encounter)
            .flat_map_iter(|group| {
                let Some(fight) = fights.get(&group[0].encounter) else {
                    return Vec::new();
                };
                let Some(battle) = battles.get(&group[0].encounter) else {
                    return Vec::new();
                };
                let Some(map) = maps.get(&group[0].region) else {
                    return Vec::new();
                };
                let mut occupied: HashSet<_> = battle
                    .people
                    .iter()
                    .map(|p| (p.position.x, p.position.y))
                    .collect();
                let fighters: HashMap<_, _> =
                    fight.fighters.iter().map(|f| (f.person, f.side)).collect();
                let positions: HashMap<_, _> = battle
                    .people
                    .iter()
                    .map(|p| (p.person, p.position))
                    .collect();
                let mut result = Vec::new();
                let height = map.rules.height.min(63) + 1;
                let capacity = (map.rules.width.min(63) + 1) * height;
                let mut cursor = 0;
                let mut deferred = Vec::<Point>::new();
                let mut exhausted = HashSet::new();
                for choice in group {
                    let live = |id| {
                        people
                            .binary_search_by_key(&id, |p| p.id)
                            .ok()
                            .is_some_and(|i| {
                                people[i].health > 0 && people[i].region == Some(choice.region)
                            })
                    };
                    if !live(choice.actor)
                        || !live(choice.beneficiary)
                        || !live(choice.opponent)
                        || fighters.get(&choice.beneficiary) != Some(&choice.side)
                        || fighters
                            .get(&choice.opponent)
                            .is_none_or(|side| *side == choice.side)
                    {
                        continue;
                    }
                    let Some(&witness) = positions.get(&choice.beneficiary) else {
                        continue;
                    };
                    if exhausted.contains(&choice.beneficiary) {
                        continue;
                    }
                    let point = deferred
                        .iter()
                        .position(|p| map.visible(*p, witness))
                        .map(|i| deferred.swap_remove(i))
                        .or_else(|| {
                            while cursor < capacity {
                                let p = Point {
                                    x: cursor / height,
                                    y: cursor % height,
                                };
                                cursor += 1;
                                if occupied.contains(&(p.x, p.y)) || !map.open(p) {
                                    continue;
                                }
                                if map.visible(p, witness) {
                                    return Some(p);
                                }
                                deferred.push(p);
                            }
                            None
                        });
                    if let Some(point) = point {
                        occupied.insert((point.x, point.y));
                        result.push((*choice, point));
                    } else {
                        exhausted.insert(choice.beneficiary);
                    }
                }
                result
            })
            .collect();
        let mut admitted: Vec<_> = additions.par_iter().map(|(row, _)| row.actor).collect();
        admitted.par_sort_unstable();
        self.state.intervention_events = selection
            .choices
            .par_iter()
            .map(|row| Event {
                person: row.actor,
                encounter: row.encounter,
                admitted: admitted.binary_search(&row.actor).is_ok(),
            })
            .collect();
        if additions.is_empty() {
            return Ok(admitted);
        }
        additions.par_sort_unstable_by_key(|(choice, _)| (choice.encounter, choice.actor));
        let grouped: HashMap<u64, Vec<(Choice, Point)>> = additions
            .par_chunk_by(|a, b| a.0.encounter == b.0.encounter)
            .map(|rows| (rows[0].0.encounter, rows.to_vec()))
            .collect();
        self.state
            .conflict
            .encounters
            .par_iter_mut()
            .for_each(|fight| {
                if let Some(rows) = grouped.get(&fight.id) {
                    fight.fighters.extend(rows.iter().map(|(row, _)| Fighter {
                        person: row.actor,
                        side: row.side,
                        order: Order::Attack {
                            target: row.opponent,
                        },
                    }));
                    fight.fighters.sort_unstable_by_key(|f| f.person);
                }
            });
        self.state
            .tactics
            .battles
            .par_iter_mut()
            .for_each(|battle| {
                if let Some(rows) = grouped.get(&battle.encounter) {
                    battle
                        .people
                        .extend(rows.iter().map(|(row, point)| Placement {
                            person: row.actor,
                            position: *point,
                            last_move: None,
                        }));
                    battle.people.sort_unstable_by_key(|p| p.person);
                }
            });
        self.replacement = Some((
            Some(Plan::compile(&self.state.conflict).map_err(str::to_owned)?),
            self.state.tactics.index().map_err(str::to_owned)?,
            Some(self.state.conflict.actors()),
        ));
        Ok(admitted)
    }
}
