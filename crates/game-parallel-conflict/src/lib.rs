//! Data-defined simultaneous conflict and attributed strike receipts.
use game_parallel_production_model::{PersonSnapshot, person_index};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conflict {
    pub damage: u32,
    pub next_id: u64,
    #[serde(default)]
    pub encounters: Vec<Encounter>,
    #[serde(default)]
    pub events: Vec<Event>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Encounter {
    pub id: u64,
    pub region: u64,
    pub fighters: Vec<Fighter>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fighter {
    pub person: u64,
    pub side: u32,
    pub order: Order,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Order {
    Attack { target: u64 },
    Defend,
    Retreat,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub encounter: u64,
    pub attacker: u64,
    pub target: u64,
    pub damage: u32,
}
#[derive(Clone, Debug)]
pub struct Command {
    pub encounter: u64,
    pub person: u64,
    pub order: Order,
}
impl Conflict {
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        regions: &HashSet<u64>,
        tick: u64,
    ) -> Result<(), String> {
        if self.damage == 0 || self.next_id == 0 {
            return Err("invalid conflict rules".into());
        }
        let mut ids = HashSet::new();
        let mut actors = HashSet::new();
        for fight in &self.encounters {
            if fight.id >= self.next_id
                || !ids.insert(fight.id)
                || !regions.contains(&fight.region)
                || fight.fighters.len() < 2
            {
                return Err("invalid encounter".into());
            }
            let sides_by_person: HashMap<_, _> = fight
                .fighters
                .par_iter()
                .map(|fighter| (fighter.person, fighter.side))
                .collect();
            let mut sides = HashSet::new();
            for fighter in &fight.fighters {
                if !people.contains(&fighter.person) || !actors.insert(fighter.person) {
                    return Err("invalid encounter actor".into());
                }
                sides.insert(fighter.side);
                if let Order::Attack { target } = fighter.order
                    && !sides_by_person
                        .get(&target)
                        .is_some_and(|&side| side != fighter.side)
                {
                    return Err("invalid attack target".into());
                }
            }
            if sides.len() < 2 {
                return Err("encounter needs opposing sides".into());
            }
        }
        if self.events.par_iter().any(|event| {
            event.tick > tick
                || !people.contains(&event.attacker)
                || !people.contains(&event.target)
                || event.encounter >= self.next_id
        }) {
            return Err("invalid conflict event".into());
        }
        Ok(())
    }
    pub fn actors(&self) -> Vec<u64> {
        let mut actors: Vec<_> = self
            .encounters
            .par_iter()
            .flat_map(|fight| fight.fighters.par_iter().map(|fighter| fighter.person))
            .collect();
        if actors.par_windows(2).any(|pair| pair[0] > pair[1]) {
            actors.par_sort_unstable();
        }
        actors
    }
    pub fn begin(
        &mut self,
        people: &[PersonSnapshot],
        busy: &[u64],
        mut fight: Encounter,
    ) -> Result<u64, String> {
        if fight.id != self.next_id || fight.fighters.len() < 2 {
            return Err("invalid encounter identity".into());
        }
        let next = self
            .next_id
            .checked_add(1)
            .ok_or("encounter identity overflow")?;
        let mut actors = self.actors();
        actors.extend_from_slice(busy);
        actors.par_sort_unstable();
        let mut local: Vec<_> = fight
            .fighters
            .iter()
            .map(|fighter| fighter.person)
            .collect();
        local.par_sort_unstable();
        if local.par_windows(2).any(|pair| pair[0] == pair[1])
            || fight.fighters.par_iter().any(|fighter| {
                actors.binary_search(&fighter.person).is_ok()
                    || !person_index(people, fighter.person).is_some_and(|index| {
                        people[index].1 == Some(fight.region) && people[index].2 > 0
                    })
            })
        {
            return Err("encounter participants must be living, local, and available".into());
        }
        let mut probe = self.clone();
        probe.next_id = next;
        probe.encounters.push(fight.clone());
        let identities: HashSet<_> = people.iter().map(|person| person.0).collect();
        let regions = [fight.region]
            .into_iter()
            .chain(self.encounters.iter().map(|fight| fight.region))
            .collect();
        probe.validate(&identities, &regions, u64::MAX)?;
        fight
            .fighters
            .sort_unstable_by_key(|fighter| fighter.person);
        let id = fight.id;
        self.encounters.push(fight);
        self.next_id = next;
        Ok(id)
    }
    /// Fast checked edits when every encounter's fighters are sorted by identity.
    pub fn order_sorted(&mut self, commands: &[Command]) -> Result<(), String> {
        let mut changes = Vec::with_capacity(commands.len());
        for command in commands {
            let fight_index = self
                .encounters
                .iter()
                .position(|fight| fight.id == command.encounter)
                .ok_or("unknown encounter")?;
            let fight = &self.encounters[fight_index];
            let index = fight
                .fighters
                .binary_search_by_key(&command.person, |fighter| fighter.person)
                .map_err(|_| "unknown fighter")?;
            if let Order::Attack { target } = command.order {
                let target = fight
                    .fighters
                    .binary_search_by_key(&target, |fighter| fighter.person)
                    .map_err(|_| "invalid attack target")?;
                if fight.fighters[target].side == fight.fighters[index].side {
                    return Err("invalid attack target".into());
                }
            }
            changes.push((fight_index, index, &command.order));
        }
        changes.sort_unstable_by_key(|row| (row.0, row.1));
        if changes
            .windows(2)
            .any(|pair| (pair[0].0, pair[0].1) == (pair[1].0, pair[1].1))
        {
            return Err("duplicate combat order".into());
        }
        for (fight, index, order) in changes {
            self.encounters[fight].fighters[index].order = order.clone();
        }
        Ok(())
    }
    pub fn order(&mut self, commands: &[Command]) -> Result<(), String> {
        let lookup: HashMap<_, _> = self
            .encounters
            .par_iter()
            .enumerate()
            .flat_map_iter(|(fight_index, fight)| {
                fight
                    .fighters
                    .iter()
                    .enumerate()
                    .map(move |(index, fighter)| {
                        (
                            (fight.id, fighter.person),
                            (fight_index, index, fighter.side),
                        )
                    })
            })
            .collect();
        let mut changes: Vec<_> = commands
            .par_iter()
            .map(|command| -> Result<_, String> {
                let &(fight, index, side) = lookup
                    .get(&(command.encounter, command.person))
                    .ok_or("unknown fighter")?;
                if let Order::Attack { target } = command.order
                    && !lookup
                        .get(&(command.encounter, target))
                        .is_some_and(|row| row.2 != side)
                {
                    return Err("invalid attack target".into());
                }
                Ok((fight, index, &command.order))
            })
            .collect::<Result<_, _>>()?;
        changes.par_sort_unstable_by_key(|row| (row.0, row.1));
        if changes
            .par_windows(2)
            .any(|pair| (pair[0].0, pair[0].1) == (pair[1].0, pair[1].1))
        {
            return Err("duplicate combat order".into());
        }
        self.encounters
            .par_iter_mut()
            .enumerate()
            .for_each(|(fight_index, fight)| {
                let start = changes.partition_point(|row| row.0 < fight_index);
                let end = changes.partition_point(|row| row.0 <= fight_index);
                let rows = &changes[start..end];
                fight
                    .fighters
                    .par_chunks_mut(4096)
                    .enumerate()
                    .for_each(|(chunk, fighters)| {
                        let base = chunk * 4096;
                        let first = rows.partition_point(|row| row.1 < base);
                        let last = rows.partition_point(|row| row.1 < base + fighters.len());
                        for &(_, index, order) in &rows[first..last] {
                            fighters[index - base].order = order.clone();
                        }
                    });
            });
        Ok(())
    }
}

/// A round is resolved from tick-start availability; strikes are simultaneous.
pub fn prepare(conflict: &Conflict, people: &[PersonSnapshot], tick: u64) -> Conflict {
    let encounters = conflict
        .encounters
        .par_iter()
        .map(|fight| Encounter {
            id: fight.id,
            region: fight.region,
            fighters: fight.fighters.par_iter().cloned().collect(),
        })
        .collect();
    let mut next = Conflict {
        damage: conflict.damage,
        next_id: conflict.next_id,
        encounters,
        events: Vec::new(),
    };
    let mut events: Vec<_> = conflict
        .encounters
        .par_iter()
        .flat_map(|fight| {
            let mut ordered = None;
            if fight
                .fighters
                .par_windows(2)
                .any(|pair| pair[0].person > pair[1].person)
            {
                let mut rows: Vec<_> = fight.fighters.par_iter().collect();
                rows.par_sort_unstable_by_key(|fighter| fighter.person);
                ordered = Some(rows);
            }
            let first = fight.fighters.first().map(|fighter| fighter.person);
            let dense = if ordered.is_none() {
                first.and_then(|first| {
                    fight.fighters.last().and_then(|last| {
                        (last.person.checked_sub(first) == Some((fight.fighters.len() - 1) as u64))
                            .then_some(first)
                    })
                })
            } else {
                None
            };
            fight.fighters.par_iter().filter_map(move |fighter| {
                let Order::Attack { target } = fighter.order else {
                    return None;
                };
                let target_fighter = if let Some(first) = dense {
                    let index = usize::try_from(target.checked_sub(first)?).ok()?;
                    fight.fighters.get(index)?
                } else if let Some(rows) = &ordered {
                    rows.get(
                        rows.binary_search_by_key(&target, |fighter| fighter.person)
                            .ok()?,
                    )
                    .copied()?
                } else {
                    fight.fighters.get(
                        fight
                            .fighters
                            .binary_search_by_key(&target, |fighter| fighter.person)
                            .ok()?,
                    )?
                };
                let available = |id| {
                    person_index(people, id)
                        .is_some_and(|i| people[i].1 == Some(fight.region) && people[i].2 > 0)
                };
                if !available(fighter.person) || !available(target) {
                    return None;
                }
                let damage = if matches!(target_fighter.order, Order::Defend) {
                    (conflict.damage / 2).max(1)
                } else {
                    conflict.damage
                };
                Some(Event {
                    tick,
                    encounter: fight.id,
                    attacker: fighter.person,
                    target,
                    damage,
                })
            })
        })
        .collect();
    if events.par_windows(2).any(|pair| {
        (pair[0].target, pair[0].encounter, pair[0].attacker)
            > (pair[1].target, pair[1].encounter, pair[1].attacker)
    }) {
        events.par_sort_unstable_by_key(|event| (event.target, event.encounter, event.attacker));
    }
    next.events = events;
    next
}
/// Resolve withdrawal and incapacitation after physical and lifecycle effects.
pub fn finish(conflict: &mut Conflict, alive: impl Fn(u64) -> bool + Sync) {
    conflict.encounters.par_iter_mut().for_each(|fight| {
        if fight
            .fighters
            .par_iter()
            .all(|fighter| !matches!(fighter.order, Order::Retreat) && alive(fighter.person))
        {
            return;
        }
        // The committed roster is immutable during this phase. Look up each
        // target there instead of rebuilding a shared hash set of survivors.
        let mut ordered = None;
        if fight
            .fighters
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            let mut rows: Vec<_> = fight.fighters.par_iter().collect();
            rows.par_sort_unstable_by_key(|fighter| fighter.person);
            ordered = Some(rows);
        }
        let dense = if ordered.is_none() {
            fight.fighters.first().and_then(|first| {
                fight.fighters.last().and_then(|last| {
                    (last.person.checked_sub(first.person)
                        == Some((fight.fighters.len() - 1) as u64))
                    .then_some(first.person)
                })
            })
        } else {
            None
        };
        let survivors: Vec<_> = fight
            .fighters
            .par_iter()
            .filter(|fighter| !matches!(fighter.order, Order::Retreat) && alive(fighter.person))
            .map(|fighter| {
                let mut survivor = fighter.clone();
                if let Order::Attack { target } = survivor.order {
                    let target_fighter = if let Some(first) = dense {
                        target
                            .checked_sub(first)
                            .and_then(|offset| usize::try_from(offset).ok())
                            .and_then(|index| fight.fighters.get(index))
                    } else if let Some(rows) = &ordered {
                        rows.binary_search_by_key(&target, |fighter| fighter.person)
                            .ok()
                            .map(|index| rows[index])
                    } else {
                        fight
                            .fighters
                            .binary_search_by_key(&target, |fighter| fighter.person)
                            .ok()
                            .map(|index| &fight.fighters[index])
                    };
                    if target_fighter.is_none_or(|fighter| {
                        matches!(fighter.order, Order::Retreat) || !alive(target)
                    }) {
                        survivor.order = Order::Defend;
                    }
                }
                survivor
            })
            .collect();
        fight.fighters = survivors;
    });
    let keep: Vec<_> = conflict
        .encounters
        .par_iter()
        .map(|fight| {
            fight.fighters.first().is_some_and(|first| {
                fight
                    .fighters
                    .par_iter()
                    .any(|other| other.side != first.side)
            })
        })
        .collect();
    let mut index = 0;
    conflict.encounters.retain(|_| {
        let result = keep[index];
        index += 1;
        result
    });
}

/// Validate active combatants are living, local, and free of unfinished work.
pub fn validate_physical(
    conflict: &Conflict,
    regions: &[game_parallel_people::Region],
    jobs: &[game_parallel_production_model::Job],
) -> Result<(), String> {
    let actors: HashSet<_> = conflict.actors().into_iter().collect();
    let local: HashMap<_, _> = regions
        .par_iter()
        .flat_map_iter(|region| {
            region
                .people
                .iter()
                .filter(|person| actors.contains(&person.id))
                .map(move |person| (person.id, (region.id, person.health)))
        })
        .collect();
    let busy: HashSet<_> = jobs
        .par_iter()
        .filter(|job| job.phase == game_parallel_production_model::JobPhase::Working)
        .map(|job| job.worker)
        .collect();
    if conflict.encounters.par_iter().any(|fight| {
        fight.fighters.par_iter().any(|fighter| {
            busy.contains(&fighter.person)
                || !local
                    .get(&fighter.person)
                    .is_some_and(|&(region, health)| region == fight.region && health > 0)
        })
    }) {
        return Err("combat participant must be living, local, and free of work".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
