//! Deterministic, independently staged quasi-static apparatus experiments.
pub mod inquiry;
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
    /// The input comes from this connected part, or from the experiment's source.
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_millis: Option<u128>,
    pub actor: u64,
    pub apparatus: u64,
    pub output_part: u64,
    /// The experimenter's supplied conditions, retained with the result.
    #[serde(default)]
    pub input_force: u64,
    #[serde(default)]
    pub input_distance: u64,
    #[serde(default)]
    pub load_force: u64,
    pub outcome: Outcome,
    pub available_force: u64,
    pub travel: u64,
    pub delivered_work: u128,
    pub broken_parts: Vec<u64>,
}

pub struct World {
    state: Snapshot,
    topology: Vec<Topology>,
    instance: u64,
}

pub struct Draft {
    instance: u64,
    revision: u64,
    tick: u64,
    changes: Vec<(usize, Apparatus)>,
    observations: Vec<Observation>,
}

/// A proposed apparatus assembled by an individual from identified parts.
/// The caller must verify physical custody and the actor's decision to assemble.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assembly {
    pub actor: u64,
    pub apparatus: Apparatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyOutcome {
    pub actor: u64,
    pub apparatus: u64,
    pub accepted: bool,
}

/// Prepared assembly results. Publication fails if the world has changed.
pub struct AssemblyDraft {
    instance: u64,
    revision: u64,
    tick: u64,
    apparatus: Vec<Apparatus>,
    outcomes: Vec<AssemblyOutcome>,
    topology: Vec<Topology>,
}

impl AssemblyDraft {
    pub fn outcomes(&self) -> &[AssemblyOutcome] {
        &self.outcomes
    }
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

#[derive(Clone, Copy)]
struct SolveNode {
    part: usize,
    parent: Option<usize>,
    component: usize,
}

type Topology = Arc<[SolveNode]>;

fn prepare_topology(machine: &Apparatus, catalog: &Catalog) -> Result<Topology, String> {
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
    let mut root = None;
    let mut shares = SmallVec::<[u16; 8]>::from_elem(0, machine.parts.len());
    let mut nodes = SmallVec::<[SolveNode; 8]>::new();
    for (index, part) in machine.parts.iter().enumerate() {
        if part.id == 0 || !(1..=1000).contains(&part.share_per_mille) {
            return Err("invalid physical part".into());
        }
        let component = catalog
            .components
            .binary_search_by_key(&part.component, |row| row.id)
            .map_err(|_| "unknown physical component")?;
        let parent = if let Some(parent) = part.parent {
            let parent = machine
                .parts
                .binary_search_by_key(&parent, |row| row.id)
                .map_err(|_| "apparatus must be acyclic and connected")?;
            shares[parent] += part.share_per_mille;
            if shares[parent] > 1000 {
                return Err("branches exceed their input force".into());
            }
            Some(parent)
        } else {
            if part.share_per_mille != 1000 {
                return Err("root must receive the full source".into());
            }
            if root.replace(index).is_some() {
                return Err("apparatus requires one source".into());
            }
            None
        };
        nodes.push(SolveNode {
            part: index,
            parent,
            component,
        });
    }
    let root = root.ok_or("apparatus requires one source")?;
    if nodes
        .iter()
        .all(|node| node.parent.is_none_or(|parent| parent < node.part))
    {
        return Ok(Arc::from(nodes.as_slice()));
    }
    let mut children = SmallVec::<[SmallVec<[usize; 2]>; 8]>::with_capacity(nodes.len());
    children.resize_with(nodes.len(), SmallVec::new);
    for node in &nodes {
        if let Some(parent) = node.parent {
            children[parent].push(node.part);
        }
    }
    let mut ordered = SmallVec::<[SolveNode; 8]>::new();
    ordered.push(nodes[root]);
    let mut cursor = 0;
    while let Some(node) = ordered.get(cursor).copied() {
        ordered.extend(children[node.part].iter().map(|&child| nodes[child]));
        cursor += 1;
    }
    if ordered.len() != nodes.len() {
        return Err("apparatus must be acyclic and connected".into());
    }
    Ok(Arc::from(ordered.as_slice()))
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
        let topology: Vec<_> = state
            .apparatus
            .par_iter()
            .map(|machine| prepare_topology(machine, &state.catalog))
            .collect::<Result<_, _>>()?;
        let mut part_ids: Vec<_> = state
            .apparatus
            .par_iter()
            .flat_map_iter(|machine| machine.parts.iter().map(|part| part.id))
            .collect();
        part_ids.par_sort_unstable();
        if part_ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("duplicated physical part".into());
        }
        let instance = NEXT_WORLD
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| "mechanics instance overflow")?;
        Ok(Self {
            state,
            topology,
            instance,
        })
    }

    pub fn snapshot(&self) -> Snapshot {
        self.state.clone()
    }

    pub fn apparatus(&self) -> &[Apparatus] {
        &self.state.apparatus
    }

    /// A stable terminal component for a default experiment.
    pub fn terminal_part(&self, apparatus: u64) -> Option<u64> {
        let index = self
            .state
            .apparatus
            .binary_search_by_key(&apparatus, |row| row.id)
            .ok()?;
        let part = self.topology[index].last()?.part;
        Some(self.state.apparatus[index].parts[part].id)
    }

    /// Stage chosen designs independently. Competing proposals must win every
    /// part and apparatus identifier they name; losing one grants no parts.
    /// The caller verifies physical custody before publishing the draft.
    pub fn prepare_assemblies(
        &self,
        tick: u64,
        seed: u64,
        requests: &[Assembly],
    ) -> Result<AssemblyDraft, String> {
        self.prepare_assemblies_at(tick, tick, seed, requests)
    }

    /// Stage assemblies with a tie key derived from simulated time.
    pub fn prepare_assemblies_at(
        &self,
        tick: u64,
        round: u64,
        seed: u64,
        requests: &[Assembly],
    ) -> Result<AssemblyDraft, String> {
        if self.state.last_tick.is_some_and(|last| tick < last)
            || self.state.revision.checked_add(1).is_none()
        {
            return Err("invalid mechanics time or revision".into());
        }
        let mut actors: Vec<_> = requests.par_iter().map(|row| row.actor).collect();
        actors.par_sort_unstable();
        if actors.first() == Some(&0) || actors.par_windows(2).any(|row| row[0] == row[1]) {
            return Err("invalid or overlapping assembly actors".into());
        }
        let prepared: Vec<_> = requests
            .par_iter()
            .map(|request| prepare_topology(&request.apparatus, &self.state.catalog))
            .collect::<Result<_, _>>()?;
        let used: HashSet<_> = self
            .state
            .apparatus
            .par_iter()
            .flat_map_iter(|machine| machine.parts.iter().map(|part| part.id))
            .collect();
        let eligible: Vec<_> = requests
            .par_iter()
            .map(|request| {
                self.state
                    .apparatus
                    .binary_search_by_key(&request.apparatus.id, |row| row.id)
                    .is_err()
                    && request
                        .apparatus
                        .parts
                        .iter()
                        .all(|part| !used.contains(&part.id))
            })
            .collect();
        let mut bids: Vec<_> = requests
            .par_iter()
            .zip(eligible.par_iter())
            .filter(|(_, valid)| **valid)
            .flat_map_iter(|(request, _)| {
                std::iter::once((0u8, request.apparatus.id, request.actor)).chain(
                    request
                        .apparatus
                        .parts
                        .iter()
                        .map(|part| (1u8, part.id, request.actor)),
                )
            })
            .collect();
        bids.par_sort_unstable();
        let winners: HashMap<_, _> = bids
            .par_chunk_by(|a, b| (a.0, a.1) == (b.0, b.1))
            .map(|group| {
                let priority = |row: &&(u8, u64, u64)| {
                    (
                        random(seed ^ round.rotate_left(17) ^ row.2.rotate_left(29)),
                        row.2,
                    )
                };
                let winner = if group.len() >= 1024 {
                    group.par_iter().min_by_key(priority)
                } else {
                    group.iter().min_by_key(priority)
                }
                .expect("nonempty assembly bids");
                ((winner.0, winner.1), winner.2)
            })
            .collect();
        let mut outcomes: Vec<_> = requests
            .par_iter()
            .zip(eligible.par_iter())
            .map(|(request, valid)| {
                let accepted = *valid
                    && winners.get(&(0, request.apparatus.id)) == Some(&request.actor)
                    && request
                        .apparatus
                        .parts
                        .iter()
                        .all(|part| winners.get(&(1, part.id)) == Some(&request.actor));
                AssemblyOutcome {
                    actor: request.actor,
                    apparatus: request.apparatus.id,
                    accepted,
                }
            })
            .collect();
        let accepted: HashSet<_> = outcomes
            .par_iter()
            .filter(|row| row.accepted)
            .map(|row| row.actor)
            .collect();
        let mut machines: Vec<_> = self
            .state
            .apparatus
            .par_iter()
            .cloned()
            .zip(self.topology.par_iter().cloned())
            .collect();
        machines.par_extend(
            requests
                .par_iter()
                .zip(prepared.into_par_iter())
                .filter(|(request, _)| accepted.contains(&request.actor))
                .map(|(request, topology)| (request.apparatus.clone(), topology)),
        );
        machines.par_sort_unstable_by_key(|(machine, _)| machine.id);
        let (apparatus, topology) = machines.into_par_iter().unzip();
        outcomes.par_sort_unstable_by_key(|row| row.actor);
        Ok(AssemblyDraft {
            instance: self.instance,
            revision: self.state.revision,
            tick,
            apparatus,
            outcomes,
            topology,
        })
    }

    pub fn commit_assemblies(
        &mut self,
        draft: AssemblyDraft,
    ) -> Result<Vec<AssemblyOutcome>, String> {
        if draft.instance != self.instance
            || draft.revision != self.state.revision
            || self.state.last_tick.is_some_and(|last| draft.tick < last)
        {
            return Err("stale mechanical assembly draft".into());
        }
        if draft.outcomes.iter().all(|outcome| !outcome.accepted) {
            return Ok(draft.outcomes);
        }
        let revision = self
            .state
            .revision
            .checked_add(1)
            .ok_or("mechanics revision overflow")?;
        let prior = std::mem::replace(&mut self.state.apparatus, draft.apparatus);
        self.topology = draft.topology;
        self.state.last_tick = Some(draft.tick);
        self.state.revision = revision;
        prior.into_par_iter().for_each(drop);
        Ok(draft.outcomes)
    }

    /// Prepare without changing the world. Callers provide chosen actions and
    /// validate personal access and physical effort before invoking this owner.
    pub fn prepare(&self, tick: u64, seed: u64, requests: &[Experiment]) -> Result<Draft, String> {
        self.prepare_at(tick, None, seed, requests)
    }

    /// Keep the physical generation for evidence and use absolute time for competing trials.
    pub fn prepare_at(
        &self,
        tick: u64,
        at_millis: Option<u128>,
        seed: u64,
        requests: &[Experiment],
    ) -> Result<Draft, String> {
        let round = at_millis.map_or(tick, |now| {
            (now as u64) ^ ((now >> 64) as u64).rotate_left(29)
        });
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
                                seed ^ round.rotate_left(17)
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
        let staged: Vec<_> = winners
            .par_iter()
            .enumerate()
            .map(|(index, &request)| {
                let slot = if aligned {
                    index
                } else {
                    self.state
                        .apparatus
                        .binary_search_by_key(&request.apparatus, |row| row.id)
                        .expect("validated apparatus")
                };
                let mut machine = self.state.apparatus[slot].clone();
                let observation = evaluate(
                    &mut machine,
                    &self.topology[slot],
                    &self.state.catalog,
                    tick,
                    at_millis,
                    request,
                )?;
                Ok((slot, machine, observation))
            })
            .collect::<Result<_, String>>()?;
        let mut observations: Vec<_> = if winners.len() == requests.len() {
            staged
                .par_iter()
                .map(|(_, _, event)| event.clone())
                .collect()
        } else {
            requests
                .par_iter()
                .map(|row| {
                    let winner = winners
                        .binary_search_by_key(&row.apparatus, |winner| winner.apparatus)
                        .expect("validated apparatus has a winner");
                    if winners[winner].actor == row.actor {
                        staged[winner].2.clone()
                    } else {
                        Observation {
                            tick,
                            at_millis,
                            actor: row.actor,
                            apparatus: row.apparatus,
                            output_part: row.output_part,
                            input_force: row.input_force,
                            input_distance: row.input_distance,
                            load_force: row.load_force,
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
            changes: staged
                .into_par_iter()
                .filter_map(|(slot, machine, event)| {
                    (!event.broken_parts.is_empty()).then_some((slot, machine))
                })
                .collect(),
            observations,
        })
    }

    /// Validate before another owner commits an independently staged draft.
    pub fn validate_draft(&self, draft: &Draft) -> Result<(), String> {
        if draft.instance != self.instance
            || draft.revision != self.state.revision
            || self.state.last_tick.is_some_and(|last| draft.tick < last)
        {
            return Err("stale mechanical experiment draft".into());
        }
        self.state
            .revision
            .checked_add(1)
            .ok_or("mechanics revision overflow")?;
        Ok(())
    }

    pub fn commit(&mut self, draft: Draft) -> Result<Vec<Observation>, String> {
        self.validate_draft(&draft)?;
        let revision = self.state.revision + 1;
        publish_changes(&mut self.state.apparatus, &draft.changes, 0);
        self.state.last_tick = Some(draft.tick);
        self.state.revision = revision;
        Ok(draft.observations)
    }
}

fn publish_changes(apparatus: &mut [Apparatus], changes: &[(usize, Apparatus)], origin: usize) {
    if changes.len() <= 64 {
        for (slot, machine) in changes {
            apparatus[*slot - origin] = machine.clone();
        }
        return;
    }
    let middle = changes.len() / 2;
    let right_origin = changes[middle].0;
    let (left, right) = apparatus.split_at_mut(right_origin - origin);
    rayon::join(
        || publish_changes(left, &changes[..middle], origin),
        || publish_changes(right, &changes[middle..], right_origin),
    );
}

#[derive(Clone, Copy)]
struct Reading {
    force: u64,
    distance: u64,
    blocked: bool,
}

fn evaluate(
    machine: &mut Apparatus,
    topology: &[SolveNode],
    catalog: &Catalog,
    tick: u64,
    at_millis: Option<u128>,
    request: Experiment,
) -> Result<Observation, String> {
    let blocked = Reading {
        force: 0,
        distance: 0,
        blocked: true,
    };
    let mut readings = SmallVec::<[Reading; 8]>::from_elem(blocked, machine.parts.len());
    let mut broken_parts = Vec::new();
    for node in topology {
        let index = node.part;
        let part = &machine.parts[index];
        let input = if let Some(parent_index) = node.parent {
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
        let component = &catalog.components[node.component];
        let failed = input.blocked || part.broken || input.force > component.max_input_force;
        if !input.blocked && !part.broken && input.force > component.max_input_force {
            let id = part.id;
            Arc::make_mut(&mut machine.parts)[index].broken = true;
            broken_parts.push(id);
        }
        if failed {
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
        readings[index] = Reading {
            force: force.min(u128::from(u64::MAX)) as u64,
            distance,
            blocked: false,
        };
    }
    broken_parts.sort_unstable();
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
        at_millis,
        actor: request.actor,
        apparatus: request.apparatus,
        output_part: request.output_part,
        input_force: request.input_force,
        input_distance: request.input_distance,
        load_force: request.load_force,
        outcome,
        available_force: target.force,
        travel,
        delivered_work: u128::from(request.load_force) * u128::from(travel),
        broken_parts,
    })
}

#[cfg(test)]
mod sparse_trials {
    use super::*;

    #[test]
    fn only_changed_apparatus_is_published_and_ties_follow_absolute_time() {
        let run = |workers, tick| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut world = World::compile(Snapshot {
                        catalog: Catalog {
                            components: vec![Component {
                                id: 1,
                                name: "Lever".into(),
                                force_ratio_numerator: 1,
                                force_ratio_denominator: 1,
                                efficiency_per_mille: 1000,
                                max_input_force: 100,
                            }],
                        },
                        apparatus: (1..=512)
                            .map(|id| Apparatus {
                                id,
                                parts: Arc::new(vec![Part {
                                    id,
                                    component: 1,
                                    parent: None,
                                    share_per_mille: 1000,
                                    broken: false,
                                }]),
                            })
                            .collect(),
                        revision: 0,
                        last_tick: None,
                    })
                    .unwrap();
                    let mut trials: Vec<_> = (1..=512)
                        .step_by(4)
                        .map(|id| Experiment {
                            actor: id,
                            apparatus: id,
                            output_part: id,
                            input_force: 200,
                            input_distance: 1,
                            load_force: 1,
                        })
                        .collect();
                    trials.push(Experiment {
                        actor: 999,
                        ..trials[0]
                    });
                    let draft = world.prepare_at(tick, Some(3415), 7, &trials).unwrap();
                    assert_eq!(draft.changes.len(), 128);
                    assert_eq!(draft.observations().len(), 129);
                    let mut observations = world.commit(draft).unwrap();
                    assert!(
                        world
                            .apparatus()
                            .iter()
                            .all(|machine| { machine.parts[0].broken == (machine.id % 4 == 1) })
                    );
                    for observation in &mut observations {
                        assert_eq!(observation.at_millis, Some(3415));
                        assert_eq!(observation.tick, tick);
                        observation.tick = 0;
                    }
                    (observations, world.apparatus().to_vec())
                })
        };
        assert_eq!(run(1, 2), run(6, 20));
    }
}
