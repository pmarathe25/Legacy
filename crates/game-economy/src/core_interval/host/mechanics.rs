//! Mechanical apparatus tied to real identified personal possessions.
use super::*;
use std::collections::HashMap;

fn indexed<T>(rows: &[T], id: u64, key: impl Fn(&T) -> u64) -> Option<&T> {
    rows.first()
        .and_then(|first| id.checked_sub(key(first)))
        .and_then(|offset| usize::try_from(offset).ok())
        .filter(|&index| rows.get(index).is_some_and(|row| key(row) == id))
        .or_else(|| rows.binary_search_by_key(&id, key).ok())
        .map(|index| &rows[index])
}

fn carries_parts(
    container: Option<&game_lots::Container>,
    apparatus: &game_mechanics::Apparatus,
    materials: &[MechanicalMaterial],
    tick: u64,
) -> bool {
    let Some(container) = container else {
        return false;
    };
    let expected = |part: &game_mechanics::Part| {
        materials
            .binary_search_by_key(&part.component, |row| row.component)
            .ok()
            .map(|index| materials[index].good)
    };
    if apparatus.parts.len() > 16 && container.lots.len() > 32 {
        let items: std::collections::HashMap<_, _> = container
            .lots
            .iter()
            .filter(|lot| lot.expires_at.is_none_or(|expires| expires > tick))
            .filter_map(|lot| lot.item.map(|item| (item, lot.good)))
            .collect();
        apparatus
            .parts
            .iter()
            .all(|part| expected(part).is_some_and(|good| items.get(&part.id) == Some(&good)))
    } else {
        apparatus.parts.iter().all(|part| {
            container.lots.iter().any(|lot| {
                lot.item == Some(part.id)
                    && Some(lot.good) == expected(part)
                    && lot.expires_at.is_none_or(|expires| expires > tick)
            })
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanicalMaterial {
    pub component: u32,
    pub good: u32,
}

/// Physical resistance per unit of an identified good.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanicalLoadMaterial {
    pub good: u32,
    pub force_per_unit: u64,
}

/// A person's exact attempt to move a local physical load with an apparatus.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanicalLoadRequest {
    pub actor: u64,
    pub apparatus: u64,
    pub output_part: u64,
    pub input_force: u64,
    pub input_distance: u64,
    pub required_travel: u64,
    pub movement: game_lots::Movement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MechanicalLoadOutcome {
    Unavailable,
    Contended,
    Stalled,
    Broken,
    InsufficientTravel,
    Moved,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanicalLoadResult {
    pub actor: u64,
    pub outcome: MechanicalLoadOutcome,
    pub observation: Option<game_mechanics::Observation>,
}

/// One person's optional inclination to attempt an exact physical load action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanicalLoadPolicy {
    pub id: u64,
    pub request: MechanicalLoadRequest,
    #[serde(default = "default_experiment_value")]
    pub value: i64,
    #[serde(default)]
    pub willing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanicalCapability {
    pub person: u64,
    pub max_force: u64,
    pub max_distance: u64,
}

/// A person's own standing inclination to try a particular apparatus.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanicalPolicy {
    pub id: u64,
    pub experiment: game_mechanics::Experiment,
    /// This person's own signed assessment, not an objective utility.
    #[serde(default = "default_experiment_value")]
    pub value: i64,
    #[serde(default)]
    pub willing: bool,
}

fn default_experiment_value() -> i64 {
    1
}

/// A person's signed reaction to personally remembered trial outcomes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanicalReactions {
    #[serde(default)]
    pub moved: i64,
    #[serde(default)]
    pub stalled: i64,
    #[serde(default)]
    pub broken: i64,
    #[serde(default)]
    pub contended: i64,
}

/// An individual's choices about sharing and accepting mechanical testimony.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanicalCommunication {
    #[serde(default)]
    pub share: bool,
    #[serde(default)]
    pub receive: bool,
    #[serde(default)]
    pub use_testimony: bool,
    /// If set, deliberately report this result instead of the observed one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub misrepresent: Option<game_mechanics::Outcome>,
}

/// A received claim, kept distinct from the recipient's own observations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanicalTestimony {
    pub speaker: u64,
    pub tick: u64,
    pub apparatus: u64,
    pub claimed_outcome: game_mechanics::Outcome,
}

/// A person's bounded recollection of their own actual experiments.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanicalMemory {
    pub person: u64,
    pub capacity: usize,
    #[serde(default)]
    pub retain: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reactions: Option<MechanicalReactions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub communication: Option<MechanicalCommunication>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub testimony: Vec<MechanicalTestimony>,
    #[serde(default)]
    pub observations: Vec<game_mechanics::Observation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreMechanics {
    pub world: game_mechanics::Snapshot,
    pub materials: Vec<MechanicalMaterial>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub load_materials: Vec<MechanicalLoadMaterial>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub load_policies: Vec<MechanicalLoadPolicy>,
    pub capabilities: Vec<MechanicalCapability>,
    #[serde(default)]
    pub policies: Vec<MechanicalPolicy>,
    #[serde(default)]
    pub memories: Vec<MechanicalMemory>,
    #[serde(default)]
    pub observations: Vec<game_mechanics::Observation>,
}

pub(super) struct Runtime {
    world: game_mechanics::World,
    part_ids: Vec<u64>,
    materials: Vec<MechanicalMaterial>,
    load_materials: Vec<MechanicalLoadMaterial>,
    load_policies: Vec<MechanicalLoadPolicy>,
    capabilities: Vec<MechanicalCapability>,
    policies: Vec<MechanicalPolicy>,
    memories: Vec<MechanicalMemory>,
    observations: Vec<game_mechanics::Observation>,
}

impl CoreMechanics {
    pub(super) fn compile(self, core: &CoreInterval) -> Result<Runtime, String> {
        if self
            .world
            .last_tick
            .is_some_and(|tick| tick > core.physical.tick())
            || self
                .materials
                .par_windows(2)
                .any(|rows| rows[0].component >= rows[1].component)
            || self
                .load_materials
                .par_windows(2)
                .any(|rows| rows[0].good >= rows[1].good)
            || self.load_policies.par_windows(2).any(|rows| {
                (rows[0].request.actor, rows[0].id) >= (rows[1].request.actor, rows[1].id)
            })
            || self.load_policies.par_iter().any(|policy| {
                let request = policy.request;
                policy.id == 0
                    || request.actor == 0
                    || request.apparatus == 0
                    || request.output_part == 0
                    || request.input_force == 0
                    || request.input_distance == 0
                    || request.required_travel == 0
                    || request.movement.id != 0
                    || request.movement.quantity == 0
                    || request.movement.to.is_none()
                    || request.movement.to == Some(request.movement.from)
                    || !request.movement.from.accepts(request.movement.good)
                    || request
                        .movement
                        .to
                        .is_some_and(|to| !to.accepts(request.movement.good))
                    || request.movement.item == Some(0)
                    || request.movement.batch == Some(0)
                    || (request.movement.item.is_some() && request.movement.quantity != 1)
                    || core
                        .people
                        .binary_search_by_key(&request.actor, |person| person.id)
                        .is_err()
                    || self
                        .load_materials
                        .binary_search_by_key(&request.movement.good, |row| row.good)
                        .is_err()
                    || self
                        .world
                        .apparatus
                        .binary_search_by_key(&request.apparatus, |row| row.id)
                        .ok()
                        .is_none_or(|index| {
                            self.world.apparatus[index]
                                .parts
                                .binary_search_by_key(&request.output_part, |row| row.id)
                                .is_err()
                        })
            })
            || self.load_materials.par_iter().any(|row| {
                row.force_per_unit == 0
                    || core
                        .physical
                        .goods()
                        .binary_search_by_key(&row.good, |good| good.id)
                        .is_err()
            })
            || self
                .capabilities
                .par_windows(2)
                .any(|rows| rows[0].person >= rows[1].person)
            || self.policies.par_windows(2).any(|rows| {
                (rows[0].experiment.actor, rows[0].id) >= (rows[1].experiment.actor, rows[1].id)
            })
            || self
                .memories
                .par_windows(2)
                .any(|rows| rows[0].person >= rows[1].person)
            || self.materials.par_iter().any(|row| {
                self.world
                    .catalog
                    .components
                    .binary_search_by_key(&row.component, |component| component.id)
                    .is_err()
                    || core
                        .physical
                        .goods()
                        .binary_search_by_key(&row.good, |good| good.id)
                        .is_err()
            })
            || self.capabilities.par_iter().any(|row| {
                row.person == 0
                    || row.max_force == 0
                    || row.max_distance == 0
                    || core
                        .people
                        .binary_search_by_key(&row.person, |person| person.id)
                        .is_err()
            })
            || self.policies.par_iter().any(|policy| {
                let request = policy.experiment;
                policy.id == 0
                    || request.actor == 0
                    || request.input_force == 0
                    || request.input_distance == 0
                    || core
                        .people
                        .binary_search_by_key(&request.actor, |person| person.id)
                        .is_err()
                    || self
                        .world
                        .apparatus
                        .binary_search_by_key(&request.apparatus, |machine| machine.id)
                        .ok()
                        .is_none_or(|index| {
                            self.world.apparatus[index]
                                .parts
                                .binary_search_by_key(&request.output_part, |part| part.id)
                                .is_err()
                        })
            })
            || self.memories.par_iter().any(|memory| {
                memory.person == 0
                    || !(1..=64).contains(&memory.capacity)
                    || memory.observations.len() > memory.capacity
                    || memory.testimony.len() > memory.capacity
                    || core
                        .people
                        .binary_search_by_key(&memory.person, |person| person.id)
                        .is_err()
                    || memory
                        .observations
                        .iter()
                        .any(|row| row.actor != memory.person || row.tick > core.physical.tick())
                    || memory
                        .observations
                        .windows(2)
                        .any(|rows| rows[0].tick > rows[1].tick)
                    || memory
                        .testimony
                        .windows(2)
                        .any(|rows| rows[0].tick > rows[1].tick)
                    || memory.testimony.iter().any(|report| {
                        report.speaker == memory.person
                            || report.tick > core.physical.tick()
                            || core
                                .people
                                .binary_search_by_key(&report.speaker, |person| person.id)
                                .is_err()
                            || self
                                .world
                                .apparatus
                                .binary_search_by_key(&report.apparatus, |machine| machine.id)
                                .is_err()
                    })
            })
            || self.world.apparatus.par_iter().any(|apparatus| {
                apparatus.parts.iter().any(|part| {
                    self.materials
                        .binary_search_by_key(&part.component, |row| row.component)
                        .is_err()
                })
            })
            || self
                .observations
                .par_windows(2)
                .any(|rows| rows[0].actor >= rows[1].actor)
            || self.observations.par_iter().any(|row| {
                let apparatus = self
                    .world
                    .apparatus
                    .binary_search_by_key(&row.apparatus, |machine| machine.id)
                    .ok()
                    .map(|index| &self.world.apparatus[index]);
                row.tick > core.physical.tick()
                    || ((row.input_force == 0) != (row.input_distance == 0))
                    || core
                        .people
                        .binary_search_by_key(&row.actor, |person| person.id)
                        .is_err()
                    || apparatus.is_none_or(|machine| {
                        machine
                            .parts
                            .binary_search_by_key(&row.output_part, |part| part.id)
                            .is_err()
                            || row.broken_parts.iter().any(|id| {
                                machine
                                    .parts
                                    .binary_search_by_key(id, |part| part.id)
                                    .ok()
                                    .is_none_or(|index| !machine.parts[index].broken)
                            })
                    })
                    || match row.outcome {
                        game_mechanics::Outcome::Moved => {
                            row.travel == 0
                                || row.delivered_work
                                    > u128::from(row.available_force) * u128::from(row.travel)
                                || (row.input_force != 0
                                    && (row.available_force < row.load_force
                                        || row.delivered_work
                                            != u128::from(row.load_force) * u128::from(row.travel)
                                        || u128::from(row.available_force)
                                            * u128::from(row.travel)
                                            > u128::from(row.input_force)
                                                * u128::from(row.input_distance)))
                        }
                        game_mechanics::Outcome::Stalled | game_mechanics::Outcome::Broken => {
                            row.travel != 0 || row.delivered_work != 0
                        }
                        game_mechanics::Outcome::Contended => {
                            row.travel != 0
                                || row.delivered_work != 0
                                || row.available_force != 0
                                || !row.broken_parts.is_empty()
                        }
                    }
            })
        {
            return Err("invalid physical mechanics state".into());
        }
        let world = game_mechanics::World::compile(self.world)?;
        let mut part_ids: Vec<_> = world
            .apparatus()
            .par_iter()
            .flat_map_iter(|apparatus| apparatus.parts.iter().map(|part| part.id))
            .collect();
        if part_ids.par_windows(2).any(|rows| rows[0] > rows[1]) {
            part_ids.par_sort_unstable();
        }
        Ok(Runtime {
            world,
            part_ids,
            materials: self.materials,
            load_materials: self.load_materials,
            load_policies: self.load_policies,
            capabilities: self.capabilities,
            policies: self.policies,
            memories: self.memories,
            observations: self.observations,
        })
    }
}

impl Runtime {
    pub(super) fn owns_part(&self, id: u64) -> bool {
        indexed(&self.part_ids, id, |row| *row).is_some()
    }
    pub(super) fn snapshot(&self) -> CoreMechanics {
        CoreMechanics {
            world: self.world.snapshot(),
            materials: self.materials.clone(),
            load_materials: self.load_materials.clone(),
            load_policies: self.load_policies.clone(),
            capabilities: self.capabilities.clone(),
            policies: self.policies.clone(),
            memories: self.memories.clone(),
            observations: self.observations.clone(),
        }
    }

    fn eligible(
        &self,
        core: &CoreInterval,
        request: &game_mechanics::Experiment,
        occupied: &[u64],
        active: &HashSet<u64>,
    ) -> bool {
        let actor = indexed(&core.people, request.actor, |person| person.id);
        let capability = indexed(&self.capabilities, request.actor, |row| row.person);
        let apparatus = indexed(self.world.apparatus(), request.apparatus, |row| row.id);
        actor.is_some_and(|person| person.health > 0 && person.region.is_some())
            && occupied.binary_search(&request.actor).is_err()
            && !active.contains(&request.actor)
            && capability.is_some_and(|capability| {
                request.input_force <= capability.max_force
                    && request.input_distance <= capability.max_distance
            })
            && apparatus.is_some_and(|apparatus| {
                carries_parts(
                    core.physical
                        .custody(game_lots::Place::Person(request.actor)),
                    apparatus,
                    &self.materials,
                    core.physical.tick(),
                )
            })
    }
}

impl CoreHost {
    pub(super) fn stage_mechanical_testimony(
        &self,
        meetings: &[game_social::Encounter],
        tick: u64,
    ) -> Option<Vec<MechanicalMemory>> {
        let owner = self.mechanics.as_ref()?;
        if meetings.is_empty()
            || !owner
                .memories
                .par_iter()
                .any(|memory| memory.communication.is_some_and(|policy| policy.share))
        {
            return None;
        }
        let mut reports: Vec<_> = meetings
            .par_iter()
            .flat_map_iter(|meeting| [(meeting.a, meeting.b), (meeting.b, meeting.a)])
            .filter_map(|(speaker, recipient)| {
                let speaker_person = indexed(&self.core.people, speaker, |row| row.id)?;
                let recipient_person = indexed(&self.core.people, recipient, |row| row.id)?;
                if speaker_person.controlled
                    || recipient_person.controlled
                    || speaker_person.health == 0
                    || recipient_person.health == 0
                {
                    return None;
                }
                let source = indexed(&owner.memories, speaker, |memory| memory.person)?;
                let destination = indexed(&owner.memories, recipient, |memory| memory.person)?;
                let policy = source.communication.filter(|policy| policy.share)?;
                if !destination
                    .communication
                    .is_some_and(|policy| policy.receive)
                {
                    return None;
                }
                let observation = source.observations.last()?;
                Some((
                    recipient,
                    MechanicalTestimony {
                        speaker,
                        tick,
                        apparatus: observation.apparatus,
                        claimed_outcome: policy.misrepresent.unwrap_or(observation.outcome),
                    },
                ))
            })
            .collect();
        if reports.is_empty() {
            return None;
        }
        reports.par_sort_unstable_by_key(|(recipient, report)| {
            (*recipient, report.speaker, report.apparatus)
        });
        let mut memories: Vec<_> = owner.memories.par_iter().cloned().collect();
        memories.par_iter_mut().for_each(|memory| {
            let begin = reports.partition_point(|row| row.0 < memory.person);
            let end = reports.partition_point(|row| row.0 <= memory.person);
            memory
                .testimony
                .extend(reports[begin..end].iter().map(|row| row.1));
            if memory.testimony.len() > memory.capacity {
                memory
                    .testimony
                    .drain(..memory.testimony.len() - memory.capacity);
            }
        });
        Some(memories)
    }

    pub(super) fn publish_mechanical_testimony(&mut self, memories: Vec<MechanicalMemory>) {
        self.mechanics
            .as_mut()
            .expect("testimony requires mechanics")
            .memories = memories;
    }

    /// Propose a simple trial of a carried apparatus for opted-in people who
    /// have no authored experiment policy. No component strength is consulted.
    pub fn preview_personal_device_trials(
        &self,
        people: &[u64],
        occupied: &[u64],
    ) -> Result<Vec<MechanicalPolicy>, String> {
        if people.par_windows(2).any(|row| row[0] >= row[1])
            || occupied.par_windows(2).any(|row| row[0] >= row[1])
        {
            return Err("invalid mechanical trial people".into());
        }
        if people.is_empty() {
            return Ok(Vec::new());
        }
        let Some(owner) = &self.mechanics else {
            return Ok(Vec::new());
        };
        let rest = self.blocked_with_rest(occupied);
        let occupied = rest.as_deref().unwrap_or(occupied);
        let active: HashSet<_> = self
            .core
            .physical
            .work()
            .par_iter()
            .filter(|row| row.phase == game_lot_runtime::Phase::Working)
            .map(|row| row.job.worker)
            .collect();
        let parts: HashMap<_, _> = owner
            .world
            .apparatus()
            .par_iter()
            .flat_map_iter(|machine| machine.parts.iter().map(move |part| (part.id, machine)))
            .collect();
        Ok(people
            .par_iter()
            .filter_map(|&actor| {
                let person = indexed(&self.core.people, actor, |row| row.id)?;
                let index = owner
                    .policies
                    .partition_point(|row| row.experiment.actor < actor);
                if person.controlled
                    || owner
                        .policies
                        .get(index)
                        .is_some_and(|row| row.experiment.actor == actor)
                {
                    return None;
                }
                let capability = indexed(&owner.capabilities, actor, |row| row.person)?;
                let container = self
                    .core
                    .physical
                    .custody(game_lots::Place::Person(actor))?;
                container
                    .lots
                    .iter()
                    .filter_map(|lot| parts.get(&lot.item?).copied())
                    .filter_map(|machine| {
                        let experiment = game_mechanics::Experiment {
                            actor,
                            apparatus: machine.id,
                            output_part: machine.parts.last()?.id,
                            input_force: capability.max_force.min(100),
                            input_distance: capability.max_distance.min(100),
                            load_force: 1,
                        };
                        let reaction = indexed(&owner.memories, actor, |row| row.person)
                            .and_then(|memory| {
                                let reactions = memory.reactions?;
                                let outcome = memory
                                    .observations
                                    .iter()
                                    .rev()
                                    .find(|row| row.apparatus == machine.id)
                                    .map(|row| row.outcome)
                                    .or_else(|| {
                                        memory
                                            .communication
                                            .filter(|policy| policy.use_testimony)
                                            .and_then(|_| {
                                                memory
                                                    .testimony
                                                    .iter()
                                                    .rev()
                                                    .find(|row| row.apparatus == machine.id)
                                                    .map(|row| row.claimed_outcome)
                                            })
                                    })?;
                                Some(match outcome {
                                    game_mechanics::Outcome::Moved => reactions.moved,
                                    game_mechanics::Outcome::Stalled => reactions.stalled,
                                    game_mechanics::Outcome::Broken => reactions.broken,
                                    game_mechanics::Outcome::Contended => reactions.contended,
                                })
                            })
                            .unwrap_or_default();
                        owner
                            .eligible(&self.core, &experiment, occupied, &active)
                            .then_some(MechanicalPolicy {
                                id: machine.id,
                                experiment,
                                value: 1i64.saturating_add(reaction),
                                willing: true,
                            })
                    })
                    .min_by_key(|policy| policy.id)
            })
            .collect())
    }

    /// Form a bounded candidate from each willing designer's carried parts.
    /// Component ratings are not inspected while choosing a design.
    pub fn preview_mechanical_designs(
        &self,
        designers: &[u64],
        occupied: &[u64],
    ) -> Result<Vec<game_mechanics::Assembly>, String> {
        if designers.par_windows(2).any(|row| row[0] >= row[1])
            || occupied.par_windows(2).any(|row| row[0] >= row[1])
        {
            return Err("invalid mechanical design people".into());
        }
        let Some(owner) = &self.mechanics else {
            return Ok(Vec::new());
        };
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        let rest = self.blocked_with_rest(occupied);
        let occupied = rest.as_deref().unwrap_or(occupied);
        let active: HashSet<_> = self
            .core
            .physical
            .work()
            .par_iter()
            .filter(|row| row.phase == game_lot_runtime::Phase::Working)
            .map(|row| row.job.worker)
            .collect();
        let mut materials = owner.materials.clone();
        materials.par_sort_unstable_by_key(|row| (row.good, row.component));
        Ok(designers
            .par_iter()
            .filter_map(|&actor| {
                let person = indexed(&self.core.people, actor, |row| row.id)?;
                if person.controlled
                    || person.health == 0
                    || person.region.is_none()
                    || occupied.binary_search(&actor).is_ok()
                    || active.contains(&actor)
                {
                    return None;
                }
                let container = self
                    .core
                    .physical
                    .custody(game_lots::Place::Person(actor))?;
                let (first, second) = container
                    .lots
                    .iter()
                    .filter_map(|lot| {
                        let item = lot.item?;
                        if owner.owns_part(item)
                            || lot.expires_at.is_some_and(|expires| expires <= tick)
                        {
                            return None;
                        }
                        let index = materials.partition_point(|row| row.good < lot.good);
                        let component = materials
                            .get(index)
                            .filter(|row| row.good == lot.good)?
                            .component;
                        let rank = game_genetics::random(
                            self.core.seed,
                            actor ^ tick.rotate_left(17),
                            item ^ offset,
                        );
                        Some((rank, item, component))
                    })
                    .fold((None, None), |(first, second), candidate| {
                        if first.is_none_or(|value| candidate < value) {
                            (Some(candidate), first)
                        } else if second.is_none_or(|value| candidate < value) {
                            (first, Some(candidate))
                        } else {
                            (first, second)
                        }
                    });
                let mut parts = [first?, second?];
                parts.sort_unstable_by_key(|row| row.1);
                let id = game_genetics::random(
                    self.core.seed,
                    0x6d65636864657369 ^ actor,
                    tick ^ offset,
                )
                .max(1);
                if indexed(owner.world.apparatus(), id, |row| row.id).is_some() {
                    return None;
                }
                Some(game_mechanics::Assembly {
                    actor,
                    apparatus: game_mechanics::Apparatus {
                        id,
                        parts: std::sync::Arc::new(vec![
                            game_mechanics::Part {
                                id: parts[0].1,
                                component: parts[0].2,
                                parent: None,
                                share_per_mille: 1000,
                                broken: false,
                            },
                            game_mechanics::Part {
                                id: parts[1].1,
                                component: parts[1].2,
                                parent: Some(parts[0].1),
                                share_per_mille: 1000,
                                broken: false,
                            },
                        ]),
                    },
                })
            })
            .collect())
    }

    /// Assemble explicitly chosen designs from the actors' actual carried
    /// identified parts. This does not grant access or choose for anyone.
    pub fn assemble_mechanical_apparatus(
        &mut self,
        requests: &[game_mechanics::Assembly],
        occupied: &[u64],
    ) -> Result<Vec<game_mechanics::AssemblyOutcome>, String> {
        if occupied.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before assembly".into());
        }
        let owner = self.mechanics.as_ref().ok_or("mechanics is not enabled")?;
        let rest = self.blocked_with_rest(occupied);
        let occupied = rest.as_deref().unwrap_or(occupied);
        let active: HashSet<_> = self
            .core
            .physical
            .work()
            .par_iter()
            .filter(|row| row.phase == game_lot_runtime::Phase::Working)
            .map(|row| row.job.worker)
            .collect();
        if requests.par_iter().any(|request| {
            indexed(&self.core.people, request.actor, |row| row.id)
                .is_none_or(|person| person.health == 0 || person.region.is_none())
                || occupied.binary_search(&request.actor).is_ok()
                || active.contains(&request.actor)
                || !carries_parts(
                    self.core
                        .physical
                        .custody(game_lots::Place::Person(request.actor)),
                    &request.apparatus,
                    &owner.materials,
                    tick,
                )
        }) {
            return Err("assembly requires an available actor and carried identified parts".into());
        }
        let draft = owner
            .world
            .prepare_assemblies(tick, self.core.seed, requests)?;
        let accepted: HashSet<_> = draft
            .outcomes()
            .par_iter()
            .filter(|row| row.accepted)
            .map(|row| row.actor)
            .collect();
        let mut part_ids = owner.part_ids.clone();
        part_ids.par_extend(
            requests
                .par_iter()
                .filter(|row| accepted.contains(&row.actor))
                .flat_map_iter(|row| row.apparatus.parts.iter().map(|part| part.id)),
        );
        part_ids.par_sort_unstable();
        let owner = self.mechanics.as_mut().expect("mechanics checked");
        let results = owner.world.commit_assemblies(draft)?;
        owner.part_ids = part_ids;
        Ok(results)
    }

    /// Inspect each person's willing local load policies without reserving stock.
    pub fn preview_mechanical_loads(
        &self,
        occupied: &[u64],
    ) -> Result<Vec<MechanicalLoadPolicy>, String> {
        if occupied.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(owner) = &self.mechanics else {
            return Ok(Vec::new());
        };
        if owner.load_policies.is_empty() {
            return Ok(Vec::new());
        }
        let rest = self.blocked_with_rest(occupied);
        let occupied = rest.as_deref().unwrap_or(occupied);
        let active: HashSet<_> = self
            .core
            .physical
            .work()
            .par_iter()
            .filter(|row| row.phase == game_lot_runtime::Phase::Working)
            .map(|row| row.job.worker)
            .collect();
        let claims = self.claims.as_ref().map(game_economy_claims::Claims::index);
        let mut sources: Vec<_> = owner
            .load_policies
            .par_iter()
            .filter(|policy| policy.request.movement.item.is_none())
            .map(|policy| (policy.request.movement.from, policy.request.movement.good))
            .collect();
        sources.par_sort_unstable();
        sources.dedup();
        let unsafe_sources: HashSet<_> = sources
            .into_par_iter()
            .filter(|(place, good)| {
                self.core.physical.custody(*place).is_some_and(|container| {
                    container.lots.par_iter().any(|lot| {
                        lot.good == *good && lot.item.is_some_and(|item| owner.owns_part(item))
                    })
                })
            })
            .collect();
        Ok(owner
            .load_policies
            .par_iter()
            .filter(|policy| {
                let request = policy.request;
                let Some(person) = indexed(&self.core.people, request.actor, |row| row.id)
                    .filter(|person| !person.controlled)
                else {
                    return false;
                };
                let Some(region) = person.region else {
                    return false;
                };
                let Some(destination) = request.movement.to else {
                    return false;
                };
                let local = |place| match place {
                    game_lots::Place::Person(id) => id == request.actor,
                    game_lots::Place::Stock {
                        region: place_region,
                        ..
                    } => place_region == region,
                    game_lots::Place::Inventory(id) => {
                        self.local_transport.as_ref().is_some_and(|transport| {
                            indexed(&transport.stores, id, |store| store.id)
                                .is_some_and(|store| store.region == region)
                        })
                    }
                    _ => false,
                };
                let Some(material) = indexed(
                    &owner.load_materials,
                    u64::from(request.movement.good),
                    |row| u64::from(row.good),
                ) else {
                    return false;
                };
                let Some(force) = material
                    .force_per_unit
                    .checked_mul(request.movement.quantity)
                else {
                    return false;
                };
                let experiment = game_mechanics::Experiment {
                    actor: request.actor,
                    apparatus: request.apparatus,
                    output_part: request.output_part,
                    input_force: request.input_force,
                    input_distance: request.input_distance,
                    load_force: force,
                };
                policy.willing
                    && local(request.movement.from)
                    && local(destination)
                    && request
                        .movement
                        .item
                        .is_none_or(|item| !owner.owns_part(item))
                    && !(request.movement.item.is_none()
                        && unsafe_sources.contains(&(request.movement.from, request.movement.good)))
                    && (!matches!(request.movement.from, game_lots::Place::Stock { .. })
                        || claims.as_ref().is_none_or(|index| {
                            index.allows(request.actor, region, request.movement.good)
                        }))
                    && owner.eligible(&self.core, &experiment, occupied, &active)
            })
            .copied()
            .collect())
    }

    /// Inspect personally willing experiment opportunities without executing them.
    pub fn preview_mechanical_experiments(
        &self,
        occupied: &[u64],
    ) -> Result<Vec<MechanicalPolicy>, String> {
        if occupied.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(owner) = &self.mechanics else {
            return Ok(Vec::new());
        };
        let rest = self.blocked_with_rest(occupied);
        let occupied = rest.as_deref().unwrap_or(occupied);
        let active: HashSet<_> = self
            .core
            .physical
            .work()
            .par_iter()
            .filter(|work| work.phase == game_lot_runtime::Phase::Working)
            .map(|work| work.job.worker)
            .collect();
        Ok(owner
            .policies
            .par_iter()
            .filter(|policy| {
                policy.willing
                    && indexed(&self.core.people, policy.experiment.actor, |person| {
                        person.id
                    })
                    .is_some_and(|person| !person.controlled)
                    && owner.eligible(&self.core, &policy.experiment, occupied, &active)
            })
            .copied()
            .collect())
    }

    /// Install the owner at a clean decision boundary; this does not grant
    /// anyone possessions, access, or an intention to experiment.
    pub fn enable_mechanics(&mut self, owner: CoreMechanics) -> Result<(), String> {
        if self
            .schedule
            .as_ref()
            .is_some_and(|schedule| !matches!(schedule.phase, DecisionPhase::Meals))
        {
            return Err("finish the pending decision before changing mechanics".into());
        }
        self.mechanics = Some(owner.compile(&self.core)?);
        Ok(())
    }

    /// Execute explicitly selected experiments. Every part must be an actual,
    /// matching identified item carried by that actor. Callers own the choice.
    pub fn experiment_mechanics(
        &mut self,
        requests: &[game_mechanics::Experiment],
        occupied: &[u64],
    ) -> Result<Vec<game_mechanics::Observation>, String> {
        if occupied.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let owner = self.mechanics.as_ref().ok_or("mechanics is not enabled")?;
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before experiments".into());
        }
        let rest = self.blocked_with_rest(occupied);
        let occupied = rest.as_deref().unwrap_or(occupied);
        let active: HashSet<_> = self
            .core
            .physical
            .work()
            .par_iter()
            .filter(|work| work.phase == game_lot_runtime::Phase::Working)
            .map(|work| work.job.worker)
            .collect();
        if requests
            .par_iter()
            .any(|request| !owner.eligible(&self.core, request, occupied, &active))
        {
            return Err("experiment requires an available person with capable effort and carried matching parts".into());
        }
        self.commit_chosen_mechanics(requests)
    }

    /// Execute personally chosen apparatus attempts against real local loads.
    /// Physical access is staged first; unavailable loads never create a
    /// mechanical observation. Successful travel transfers the exact lot.
    pub fn move_mechanical_loads(
        &mut self,
        requests: &[MechanicalLoadRequest],
        occupied: &[u64],
    ) -> Result<Vec<MechanicalLoadResult>, String> {
        if occupied.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before mechanical loads".into());
        }
        let owner = self.mechanics.as_ref().ok_or("mechanics is not enabled")?;
        let mut actors: Vec<_> = requests.par_iter().map(|row| row.actor).collect();
        actors.par_sort_unstable();
        if actors.par_windows(2).any(|rows| rows[0] == rows[1]) {
            return Err("overlapping mechanical load actors".into());
        }
        let rest = self.blocked_with_rest(occupied);
        let occupied = rest.as_deref().unwrap_or(occupied);
        let active: HashSet<_> = self
            .core
            .physical
            .work()
            .par_iter()
            .filter(|row| row.phase == game_lot_runtime::Phase::Working)
            .map(|row| row.job.worker)
            .collect();
        let claims = self.claims.as_ref().map(game_economy_claims::Claims::index);
        let local = |place: game_lots::Place, actor: u64, region: u64| match place {
            game_lots::Place::Person(id) => id == actor,
            game_lots::Place::Stock {
                region: place_region,
                ..
            } => place_region == region,
            game_lots::Place::Inventory(id) => {
                self.local_transport.as_ref().is_some_and(|transport| {
                    indexed(&transport.stores, id, |store| store.id)
                        .is_some_and(|store| store.region == region)
                })
            }
            _ => false,
        };
        let prepared: Vec<_> = requests
            .par_iter()
            .enumerate()
            .map(|(index, request)| -> Result<_, String> {
                let person = indexed(&self.core.people, request.actor, |row| row.id)
                    .ok_or("unknown mechanical load actor")?;
                let region = person
                    .region
                    .ok_or("mechanical load requires a local actor")?;
                let movement = request.movement;
                let destination = movement
                    .to
                    .ok_or("mechanical load requires a destination")?;
                let material = indexed(&owner.load_materials, u64::from(movement.good), |row| {
                    u64::from(row.good)
                })
                .ok_or("unknown mechanical load material")?;
                let force = material
                    .force_per_unit
                    .checked_mul(movement.quantity)
                    .ok_or("mechanical load force overflow")?;
                if movement.id != 0
                    || movement.quantity == 0
                    || request.required_travel == 0
                    || movement.from == destination
                    || !local(movement.from, request.actor, region)
                    || !local(destination, request.actor, region)
                    || (matches!(movement.from, game_lots::Place::Stock { .. })
                        && claims.as_ref().is_some_and(|claims| {
                            !claims.allows(request.actor, region, movement.good)
                        }))
                {
                    return Err("invalid or inaccessible mechanical load".into());
                }
                let experiment = game_mechanics::Experiment {
                    actor: request.actor,
                    apparatus: request.apparatus,
                    output_part: request.output_part,
                    input_force: request.input_force,
                    input_distance: request.input_distance,
                    load_force: force,
                };
                if !owner.eligible(&self.core, &experiment, occupied, &active) {
                    return Err(
                        "mechanical load requires an available capable actor and carried parts"
                            .into(),
                    );
                }
                let action = u64::try_from(index)
                    .map_err(|_| "too many mechanical loads")?
                    .checked_add(1)
                    .ok_or("mechanical load action overflow")?;
                let mut movement = movement;
                movement.id = action;
                Ok((
                    experiment,
                    game_lots::Transaction {
                        id: action,
                        movements: vec![movement],
                        creations: vec![],
                    },
                    region,
                    request.required_travel,
                ))
            })
            .collect::<Result<_, String>>()?;
        let actions: Vec<_> = prepared.par_iter().map(|row| row.1.clone()).collect();
        let access = self
            .core
            .physical
            .prepare_interval_transfer(&actions, self.core.seed)?;
        if access
            .item_updates()
            .par_iter()
            .any(|(item, _)| owner.owns_part(*item))
        {
            return Err("mechanical loads cannot move attached apparatus parts".into());
        }
        let admitted: Vec<_> = prepared
            .par_iter()
            .filter(|row| {
                access
                    .transactions()
                    .binary_search_by_key(&row.1.id, |outcome| outcome.id)
                    .ok()
                    .is_some_and(|index| access.transactions()[index].accepted)
            })
            .map(|row| row.0)
            .collect();
        if admitted.is_empty() {
            return Ok(requests
                .par_iter()
                .map(|request| MechanicalLoadResult {
                    actor: request.actor,
                    outcome: MechanicalLoadOutcome::Unavailable,
                    observation: None,
                })
                .collect());
        }
        let mechanical = owner.world.prepare(tick, self.core.seed, &admitted)?;
        let mut by_actor: Vec<_> = prepared.par_iter().map(|row| (row.0.actor, row)).collect();
        by_actor.par_sort_unstable_by_key(|row| row.0);
        let moved: Vec<_> = mechanical
            .observations()
            .par_iter()
            .filter_map(|observation| {
                let index = by_actor
                    .binary_search_by_key(&observation.actor, |row| row.0)
                    .ok()?;
                let row = by_actor[index].1;
                (observation.outcome == game_mechanics::Outcome::Moved
                    && observation.travel >= row.3)
                    .then(|| row.1.clone())
            })
            .collect();
        let physical = if moved.is_empty() {
            None
        } else {
            let draft = self
                .core
                .physical
                .prepare_interval_transfer(&moved, self.core.seed)?;
            if draft
                .item_updates()
                .par_iter()
                .any(|(item, _)| owner.owns_part(*item))
            {
                return Err("mechanical loads cannot move attached apparatus parts".into());
            }
            if draft.transactions().par_iter().any(|row| !row.accepted) {
                return Err("mechanical load reservation changed during staging".into());
            }
            Some(draft)
        };
        let uses: Vec<_> = moved
            .par_iter()
            .filter_map(|action| {
                let index = usize::try_from(action.id.checked_sub(1)?).ok()?;
                let (experiment, _, region, _) = &prepared[index];
                let movement = action.movements.first()?;
                matches!(movement.from, game_lots::Place::Stock { .. }).then_some(
                    game_economy_claims::Use {
                        action: action.id,
                        person: experiment.actor,
                        region: *region,
                        good: movement.good,
                        quantity: movement.quantity,
                        job: false,
                    },
                )
            })
            .collect();
        let mut claims_state = self.claims.clone().filter(|_| !uses.is_empty());
        let mut opinions = claims_state.as_ref().map(|_| self.opinions.clone());
        if let (Some(claims), Some(physical)) = (&mut claims_state, &physical) {
            let events = game_lot_runtime::Events {
                tick,
                physical_actions: physical.transactions().par_iter().cloned().collect(),
                started: vec![],
                start_denied: vec![],
                abandoned: vec![],
                suspended: vec![],
                adopted: vec![],
                finished: vec![],
                delivered: vec![],
                output_lost: vec![],
                interrupted: vec![],
                returned: vec![],
                empty_returns: vec![],
                spoiled: vec![],
                physical_movements: Default::default(),
            };
            let prior: Vec<_> = std::mem::take(&mut claims.events)
                .into_par_iter()
                .filter(|event| event.tick == tick)
                .collect();
            game_economy_claims::publish(
                claims,
                &uses,
                &events,
                &self.core.people,
                opinions.as_mut().expect("claims stage opinions"),
                None,
            );
            claims.events.par_extend(prior);
            claims.events.par_sort_unstable_by_key(|event| {
                (
                    event.tick,
                    event.claimant,
                    event.person,
                    event.region,
                    event.good,
                )
            });
        }
        let results: Vec<_> = requests
            .par_iter()
            .map(|request| {
                let observation =
                    indexed(mechanical.observations(), request.actor, |row| row.actor).cloned();
                let outcome = match observation.as_ref() {
                    None => MechanicalLoadOutcome::Unavailable,
                    Some(row) => match row.outcome {
                        game_mechanics::Outcome::Contended => MechanicalLoadOutcome::Contended,
                        game_mechanics::Outcome::Stalled => MechanicalLoadOutcome::Stalled,
                        game_mechanics::Outcome::Broken => MechanicalLoadOutcome::Broken,
                        game_mechanics::Outcome::Moved if row.travel < request.required_travel => {
                            MechanicalLoadOutcome::InsufficientTravel
                        }
                        game_mechanics::Outcome::Moved => MechanicalLoadOutcome::Moved,
                    },
                };
                MechanicalLoadResult {
                    actor: request.actor,
                    outcome,
                    observation,
                }
            })
            .collect();
        let memories = (!owner.memories.is_empty()).then(|| {
            owner
                .memories
                .par_iter()
                .cloned()
                .map(|mut memory| {
                    if memory.retain
                        && let Some(observation) =
                            indexed(mechanical.observations(), memory.person, |row| row.actor)
                    {
                        if memory.observations.len() == memory.capacity {
                            memory.observations.remove(0);
                        }
                        memory.observations.push(observation.clone());
                    }
                    memory
                })
                .collect::<Vec<_>>()
        });
        let tools = physical
            .as_ref()
            .map(|draft| self.stage_tools(&[], None, draft.item_updates()))
            .transpose()?
            .flatten();
        owner.world.validate_draft(&mechanical)?;
        if let Some(draft) = &physical {
            self.core.physical.validate_interval_consumption(draft)?;
        }
        if let Some(draft) = physical {
            self.core.physical.commit_interval_consumption(draft)?;
        }
        let owner = self.mechanics.as_mut().expect("owner checked");
        let observations = owner.world.commit(mechanical)?;
        if let Some(memories) = memories {
            owner.memories = memories;
        }
        owner.observations = observations;
        if let Some(tools) = tools {
            self.tool_wear = Some(tools);
        }
        if let Some(claims) = claims_state {
            self.claims = Some(claims);
        }
        if let Some(opinions) = opinions {
            self.opinions = opinions;
        }
        self.work_checked_revision = None;
        Ok(results)
    }

    /// The scheduler calls this immediately after exact-intent and physical
    /// access revalidation, without an intervening mutation.
    pub(super) fn commit_chosen_mechanics(
        &mut self,
        requests: &[game_mechanics::Experiment],
    ) -> Result<Vec<game_mechanics::Observation>, String> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        let owner = self.mechanics.as_ref().ok_or("mechanics is not enabled")?;
        let draft = owner
            .world
            .prepare(self.core.physical.tick(), self.core.seed, requests)?;
        let memories = (!owner.memories.is_empty()).then(|| {
            owner
                .memories
                .par_iter()
                .cloned()
                .map(|mut memory| {
                    if memory.retain
                        && let Some(observation) =
                            indexed(draft.observations(), memory.person, |row| row.actor)
                    {
                        if memory.observations.len() == memory.capacity {
                            memory.observations.remove(0);
                        }
                        memory.observations.push(observation.clone());
                    }
                    memory
                })
                .collect::<Vec<_>>()
        });
        let owner = self.mechanics.as_mut().expect("owner checked");
        let observations = owner.world.commit(draft)?;
        if let Some(memories) = memories {
            owner.memories = memories;
        }
        owner.observations = observations.par_iter().cloned().collect();
        Ok(observations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_carried_assemblies_require_every_matching_item() {
        let apparatus = game_mechanics::Apparatus {
            id: 1,
            parts: std::sync::Arc::new(
                (1..=40)
                    .map(|id| game_mechanics::Part {
                        id,
                        component: 1,
                        parent: None,
                        share_per_mille: 1000,
                        broken: false,
                    })
                    .collect(),
            ),
        };
        let lot = |id| game_lots::Lot {
            batch: None,
            item: Some(id),
            good: 1,
            quantity: 1,
            expires_at: None,
        };
        let bag = game_lots::Container::new(
            game_lots::Place::Person(1),
            100,
            (1..=40).map(lot).collect(),
        )
        .unwrap();
        let missing =
            game_lots::Container::new(game_lots::Place::Person(1), 100, (1..40).map(lot).collect())
                .unwrap();
        assert!(carries_parts(
            Some(&bag),
            &apparatus,
            &[MechanicalMaterial {
                component: 1,
                good: 1
            }],
            0
        ));
        assert!(!carries_parts(
            Some(&missing),
            &apparatus,
            &[MechanicalMaterial {
                component: 1,
                good: 1
            }],
            0
        ));
        assert!(!carries_parts(
            Some(&bag),
            &apparatus,
            &[MechanicalMaterial {
                component: 1,
                good: 2
            }],
            0
        ));
        assert!(!carries_parts(Some(&bag), &apparatus, &[], 0));
    }
}
