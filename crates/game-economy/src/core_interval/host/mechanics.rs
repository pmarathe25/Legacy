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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreMechanics {
    pub world: game_mechanics::Snapshot,
    pub materials: Vec<MechanicalMaterial>,
    pub capabilities: Vec<MechanicalCapability>,
    #[serde(default)]
    pub observations: Vec<game_mechanics::Observation>,
}

pub(super) struct Runtime {
    world: game_mechanics::World,
    materials: Vec<MechanicalMaterial>,
    capabilities: Vec<MechanicalCapability>,
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
        Ok(Runtime {
            world: game_mechanics::World::compile(self.world)?,
            materials: self.materials,
            capabilities: self.capabilities,
            observations: self.observations,
        })
    }
}

impl Runtime {
    pub(super) fn snapshot(&self) -> CoreMechanics {
        CoreMechanics {
            world: self.world.snapshot(),
            materials: self.materials.clone(),
            capabilities: self.capabilities.clone(),
            observations: self.observations.clone(),
        }
    }
}

impl CoreHost {
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
        let apparatus = owner.world.apparatus();
        let active: HashSet<_> = self
            .core
            .physical
            .work()
            .par_iter()
            .filter(|work| work.phase == game_lot_runtime::Phase::Working)
            .map(|work| work.job.worker)
            .collect();
        let tick = self.core.physical.tick();
        if requests.par_iter().any(|request| {
            let actor = indexed(&self.core.people, request.actor, |person| person.id);
            let capability = indexed(&owner.capabilities, request.actor, |row| row.person);
            let apparatus = indexed(apparatus, request.apparatus, |row| row.id);
            actor.is_none_or(|person| person.health == 0 || person.region.is_none())
                || occupied.binary_search(&request.actor).is_ok()
                || active.contains(&request.actor)
                || capability.is_none_or(|capability| {
                    request.input_force > capability.max_force
                        || request.input_distance > capability.max_distance
                })
                || apparatus.is_none_or(|apparatus| {
                    !carries_parts(
                        self.core
                            .physical
                            .custody(game_lots::Place::Person(request.actor)),
                        apparatus,
                        &owner.materials,
                        tick,
                    )
                })
        }) {
            return Err("experiment requires an available person with capable effort and carried matching parts".into());
        }
        let draft = owner.world.prepare(tick, self.core.seed, requests)?;
        let owner = self.mechanics.as_mut().expect("owner checked");
        let observations = owner.world.commit(draft)?;
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
