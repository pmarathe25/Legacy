//! Mechanical apparatus tied to real identified personal possessions.
use super::*;

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

/// A person's bounded recollection of their own actual experiments.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanicalMemory {
    pub person: u64,
    pub capacity: usize,
    #[serde(default)]
    pub retain: bool,
    #[serde(default)]
    pub observations: Vec<game_mechanics::Observation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreMechanics {
    pub world: game_mechanics::Snapshot,
    pub materials: Vec<MechanicalMaterial>,
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
