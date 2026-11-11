//! Deterministic, independently staged quasi-static apparatus experiments.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_WORLD: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub id: u32,
    pub name: String,
    pub force_ratio_numerator: u32,
    pub force_ratio_denominator: u32,
    pub efficiency_per_mille: u16,
    pub max_input_force: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub components: Vec<Component>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Part {
    pub id: u64,
    pub component: u32,
    /// The input comes from this earlier part, or from the experiment's source.
    pub parent: Option<u64>,
    /// Fraction of the parent's output force directed to this branch.
    pub share_per_mille: u16,
    #[serde(default)]
    pub broken: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Apparatus {
    pub id: u64,
    pub parts: Arc<Vec<Part>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub catalog: Catalog,
    pub apparatus: Vec<Apparatus>,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub last_tick: Option<u64>,
}

/// A person chooses an experiment; this request alone does not grant custody,
/// access, effort, or another person's cooperation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Experiment {
    pub actor: u64,
    pub apparatus: u64,
    pub output_part: u64,
    pub input_force: u64,
    pub input_distance: u64,
    pub load_force: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Moved,
    Stalled,
    Broken,
    Contended,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub tick: u64,
    pub actor: u64,
    pub apparatus: u64,
    pub output_part: u64,
    pub outcome: Outcome,
    pub available_force: u64,
    pub travel: u64,
    pub delivered_work: u128,
    pub broken_parts: Vec<u64>,
}

pub struct World {
    state: Snapshot,
    instance: u64,
}

pub struct Draft {
    instance: u64,
    revision: u64,
    tick: u64,
    apparatus: Vec<Apparatus>,
    observations: Vec<Observation>,
}

impl Draft {
    pub fn observations(&self) -> &[Observation] {
        &self.observations
    }
}

fn random(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

impl World {
    pub fn compile(state: Snapshot) -> Result<Self, String> {
        let catalog = &state.catalog.components;
        if catalog.is_empty()
            || catalog.windows(2).any(|rows| rows[0].id >= rows[1].id)
            || catalog.iter().any(|part| {
                part.id == 0
                    || part.name.is_empty()
                    || part.force_ratio_numerator == 0
                    || part.force_ratio_denominator == 0
                    || part.max_input_force == 0
                    || !(1..=1000).contains(&part.efficiency_per_mille)
            })
            || state
                .apparatus
                .windows(2)
                .any(|rows| rows[0].id >= rows[1].id)
        {
            return Err("invalid mechanics catalog or apparatus order".into());
        }
        let mut part_ids = HashSet::new();
        for machine in &state.apparatus {
            if machine.id == 0
                || machine.parts.is_empty()
                || machine.parts.len() > 1024
                || machine
                    .parts
                    .windows(2)
                    .any(|rows| rows[0].id >= rows[1].id)
            {
                return Err("invalid apparatus parts".into());
            }
            let mut roots = 0;
            let mut shares = HashMap::<u64, u32>::new();
            for part in machine.parts.iter() {
                if part.id == 0
                    || !part_ids.insert(part.id)
                    || catalog
                        .binary_search_by_key(&part.component, |row| row.id)
                        .is_err()
                    || !(1..=1000).contains(&part.share_per_mille)
                {
                    return Err("invalid or duplicated physical part".into());
                }
                if let Some(parent) = part.parent {
                    if parent >= part.id
                        || machine
                            .parts
                            .binary_search_by_key(&parent, |row| row.id)
                            .is_err()
                    {
                        return Err("apparatus must be acyclic and connected".into());
                    }
                    let total = shares.entry(parent).or_default();
                    *total += u32::from(part.share_per_mille);
                    if *total > 1000 {
                        return Err("branches exceed their input force".into());
                    }
                } else {
                    roots += 1;
                    if part.share_per_mille != 1000 {
                        return Err("root must receive the full source".into());
                    }
                }
            }
            if roots != 1 {
                return Err("apparatus requires one source".into());
            }
        }
        let instance = NEXT_WORLD
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| "mechanics instance overflow")?;
        Ok(Self { state, instance })
    }

    pub fn snapshot(&self) -> Snapshot {
        self.state.clone()
    }

    pub fn apparatus(&self) -> &[Apparatus] {
        &self.state.apparatus
    }

    /// Prepare without changing the world. Callers provide chosen actions and
    /// validate personal access and physical effort before invoking this owner.
    pub fn prepare(&self, tick: u64, seed: u64, requests: &[Experiment]) -> Result<Draft, String> {
        if self.state.last_tick.is_some_and(|last| tick < last)
            || self.state.revision.checked_add(1).is_none()
        {
            return Err("invalid mechanics time or revision".into());
        }
        let unordered_actors = requests
            .par_windows(2)
            .any(|rows| rows[0].actor > rows[1].actor);
        let duplicate_actors = if unordered_actors {
            let mut actors: Vec<_> = requests.par_iter().map(|row| row.actor).collect();
            actors.par_sort_unstable();
            actors.par_windows(2).any(|rows| rows[0] == rows[1])
        } else {
            requests
                .par_windows(2)
                .any(|rows| rows[0].actor == rows[1].actor)
        };
        let aligned = requests.len() == self.state.apparatus.len()
            && requests
                .par_iter()
                .zip(self.state.apparatus.par_iter())
                .all(|(request, machine)| request.apparatus == machine.id);
        if duplicate_actors
            || requests.par_iter().enumerate().any(|(position, request)| {
                request.actor == 0
                    || request.input_force == 0
                    || request.input_distance == 0
                    || (if aligned {
                        Some(position)
                    } else {
                        self.state
                            .apparatus
                            .binary_search_by_key(&request.apparatus, |row| row.id)
                            .ok()
                    })
                    .is_none_or(|index| {
                        self.state.apparatus[index]
                            .parts
                            .binary_search_by_key(&request.output_part, |part| part.id)
                            .is_err()
                    })
            })
        {
            return Err("invalid or overlapping mechanical experiments".into());
        }
        let selected;
        let winners: &[Experiment] = if aligned {
            requests
        } else {
            let mut ranked: Vec<_> = requests.par_iter().copied().collect();
            if ranked
                .par_windows(2)
                .any(|rows| rows[0].apparatus > rows[1].apparatus)
            {
                ranked.par_sort_unstable_by_key(|row| row.apparatus);
            }
            selected = ranked
                .par_chunk_by(|a, b| a.apparatus == b.apparatus)
                .map(|rows| {
                    let priority = |row: &&Experiment| {
                        (
                            random(
                                seed ^ tick.rotate_left(17)
                                    ^ row.apparatus
                                    ^ row.actor.rotate_left(29),
                            ),
                            row.actor,
                        )
                    };
                    *if rows.len() >= 1024 {
                        rows.par_iter().min_by_key(priority)
                    } else {
                        rows.iter().min_by_key(priority)
                    }
                    .expect("nonempty group")
                })
                .collect::<Vec<_>>();
            &selected
        };
        let staged: Vec<_> = self
            .state
            .apparatus
            .par_iter()
            .enumerate()
            .map(|(position, opening)| {
                let mut machine = opening.clone();
                let winner = if aligned {
                    Some(position)
                } else {
                    winners
                        .binary_search_by_key(&machine.id, |row| row.apparatus)
                        .ok()
                };
                let observation = winner
                    .map(|index| evaluate(&mut machine, &self.state.catalog, tick, winners[index]))
                    .transpose()?;
                Ok((machine, observation))
            })
            .collect::<Result<_, String>>()?;
        let mut observations: Vec<_> = if winners.len() == requests.len() {
            staged
                .par_iter()
                .filter_map(|(_, event)| event.clone())
                .collect()
        } else {
            requests
                .par_iter()
                .map(|row| {
                    let winner = winners
                        .binary_search_by_key(&row.apparatus, |winner| winner.apparatus)
                        .expect("validated apparatus has a winner");
                    if winners[winner].actor == row.actor {
                        let index = staged
                            .binary_search_by_key(&row.apparatus, |(machine, _)| machine.id)
                            .expect("validated apparatus is staged");
                        staged[index].1.clone().expect("winner was evaluated")
                    } else {
                        Observation {
                            tick,
                            actor: row.actor,
                            apparatus: row.apparatus,
                            output_part: row.output_part,
                            outcome: Outcome::Contended,
                            available_force: 0,
                            travel: 0,
                            delivered_work: 0,
                            broken_parts: Vec::new(),
                        }
                    }
                })
                .collect()
        };
        if observations
            .par_windows(2)
            .any(|rows| (rows[0].actor, rows[0].apparatus) > (rows[1].actor, rows[1].apparatus))
        {
            observations.par_sort_unstable_by_key(|row| (row.actor, row.apparatus));
        }
        Ok(Draft {
            instance: self.instance,
            revision: self.state.revision,
            tick,
            apparatus: staged.into_par_iter().map(|(machine, _)| machine).collect(),
            observations,
        })
    }

    pub fn commit(&mut self, draft: Draft) -> Result<Vec<Observation>, String> {
        if draft.instance != self.instance
            || draft.revision != self.state.revision
            || self.state.last_tick.is_some_and(|last| draft.tick < last)
        {
            return Err("stale mechanical experiment draft".into());
        }
        let revision = self
            .state
            .revision
            .checked_add(1)
            .ok_or("mechanics revision overflow")?;
        let prior = std::mem::replace(&mut self.state.apparatus, draft.apparatus);
        self.state.last_tick = Some(draft.tick);
        self.state.revision = revision;
        prior.into_par_iter().for_each(drop);
        Ok(draft.observations)
    }
}

#[derive(Clone, Copy)]
struct Reading {
    force: u64,
    distance: u64,
    blocked: bool,
}

fn evaluate(
    machine: &mut Apparatus,
    catalog: &Catalog,
    tick: u64,
    request: Experiment,
) -> Result<Observation, String> {
    let mut readings = SmallVec::<[Reading; 8]>::with_capacity(machine.parts.len());
    let mut broken_parts = Vec::new();
    for index in 0..machine.parts.len() {
        let part = &machine.parts[index];
        let input = if let Some(parent) = part.parent {
            let parent_index = machine
                .parts
                .binary_search_by_key(&parent, |row| row.id)
                .expect("validated parent");
            let parent = readings[parent_index];
            Reading {
                force: ((u128::from(parent.force) * u128::from(part.share_per_mille)) / 1000)
                    as u64,
                distance: parent.distance,
                blocked: parent.blocked,
            }
        } else {
            Reading {
                force: request.input_force,
                distance: request.input_distance,
                blocked: false,
            }
        };
        let component = &catalog.components[catalog
            .components
            .binary_search_by_key(&part.component, |row| row.id)
            .expect("validated component")];
        let failed = input.blocked || part.broken || input.force > component.max_input_force;
        if !input.blocked && !part.broken && input.force > component.max_input_force {
            let id = part.id;
            Arc::make_mut(&mut machine.parts)[index].broken = true;
            broken_parts.push(id);
        }
        if failed {
            readings.push(Reading {
                force: 0,
                distance: 0,
                blocked: true,
            });
            continue;
        }
        let distance = u64::try_from(
            u128::from(input.distance) * u128::from(component.force_ratio_denominator)
                / u128::from(component.force_ratio_numerator),
        )
        .map_err(|_| "mechanical travel overflow")?;
        let raw_force = u128::from(input.force)
            * u128::from(component.force_ratio_numerator)
            * u128::from(component.efficiency_per_mille)
            / u128::from(component.force_ratio_denominator)
            / 1000;
        let input_work = u128::from(input.force) * u128::from(input.distance);
        let efficiency = u128::from(component.efficiency_per_mille);
        let work = (input_work / 1000) * efficiency + (input_work % 1000) * efficiency / 1000;
        let force = if distance == 0 {
            raw_force
        } else {
            raw_force.min(work / u128::from(distance))
        };
        readings.push(Reading {
            force: force.min(u128::from(u64::MAX)) as u64,
            distance,
            blocked: false,
        });
    }
    let target = readings[machine
        .parts
        .binary_search_by_key(&request.output_part, |row| row.id)
        .expect("validated output")];
    let outcome = if target.blocked {
        Outcome::Broken
    } else if target.distance == 0 || target.force < request.load_force {
        Outcome::Stalled
    } else {
        Outcome::Moved
    };
    let travel = if outcome == Outcome::Moved {
        target.distance
    } else {
        0
    };
    Ok(Observation {
        tick,
        actor: request.actor,
        apparatus: request.apparatus,
        output_part: request.output_part,
        outcome,
        available_force: target.force,
        travel,
        delivered_work: u128::from(request.load_force) * u128::from(travel),
        broken_parts,
    })
}
