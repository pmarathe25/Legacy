//! Immutable incoming-strike topology and compact, generation-owned round receipts.
use game_parallel_conflict::{Conflict, Order};
use game_parallel_production_model::{PersonSnapshot, person_index};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use game_parallel_conflict_receipts::{
    CHUNK, Encounter, Pages, Target, Topology, WORDS, count_range,
};
pub use game_parallel_conflict_receipts::{Damage, Round};
/// Rebuild when encounter membership, orders, or damage rules change. A round
/// retains its own immutable generation, so later plans cannot alter old receipts.
#[derive(Clone)]
pub struct Plan {
    attack_slots: Arc<Vec<(u64, usize)>>,
    topology: Arc<Topology>,
    enabled: Option<Pages<[u64; WORDS]>>,
    effective_damage: Option<Pages<u32>>,
    base_damage: u32,
    active_edges: usize,
}

impl Plan {
    /// Validate topology without copying physical state. The host separately
    /// validates encounter identities against its complete world on import.
    pub fn compile(conflict: &Conflict) -> Result<Self, &'static str> {
        if conflict.damage == 0 {
            return Err("invalid conflict damage");
        }
        let mut encounter_ids = HashSet::new();
        let mut actors = HashSet::new();
        for fight in &conflict.encounters {
            if !encounter_ids.insert(fight.id) {
                return Err("duplicate encounter");
            }
            for fighter in &fight.fighters {
                if !actors.insert(fighter.person) {
                    return Err("fighter has multiple encounter slots");
                }
            }
        }
        let mut edges: Vec<_> = conflict
            .encounters
            .par_iter()
            .enumerate()
            .map(|(encounter, fight)| {
                let fighters: HashMap<_, _> =
                    fight.fighters.par_iter().map(|f| (f.person, f)).collect();
                fight
                    .fighters
                    .par_iter()
                    .filter_map(|fighter| {
                        let Order::Attack { target } = fighter.order else {
                            return None;
                        };
                        let result = fighters
                            .get(&target)
                            .filter(|other| other.side != fighter.side)
                            .ok_or("invalid strike target")
                            .map(|other| {
                                let damage = if matches!(other.order, Order::Defend) {
                                    (conflict.damage / 2).max(1)
                                } else {
                                    conflict.damage
                                };
                                (target, fight.id, fighter.person, encounter, damage)
                            });
                        Some(result)
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_par_iter()
            .flatten()
            .collect();
        edges.par_sort_unstable_by_key(|edge| (edge.0, edge.1, edge.2));
        let groups: Vec<_> = edges
            .par_chunk_by(|a, b| (a.0, a.1) == (b.0, b.1))
            .collect();
        let targets: Vec<_> = groups
            .par_iter()
            .map(|group| Target {
                person: group[0].0,
                encounter: group[0].3,
                damage: group[0].4,
            })
            .collect();
        let mut offsets = Vec::with_capacity(groups.len() + 1);
        offsets.push(0);
        for group in &groups {
            offsets.push(offsets.last().unwrap() + group.len());
        }
        let active_edges = edges.len();
        let attackers: Vec<_> = edges.into_par_iter().map(|edge| edge.2).collect();
        let mut attack_slots: Vec<_> = attackers
            .par_iter()
            .enumerate()
            .map(|(slot, &actor)| (actor, slot))
            .collect();
        attack_slots.par_sort_unstable_by_key(|entry| entry.0);
        let effective_damage = Some(Pages::new(
            targets.par_iter().map(|target| target.damage).collect(),
        ));
        let encounters = conflict
            .encounters
            .iter()
            .map(|fight| Encounter {
                id: fight.id,
                region: fight.region,
            })
            .collect();
        Ok(Self {
            attack_slots: Arc::new(attack_slots),
            topology: Arc::new(Topology {
                encounters,
                targets,
                offsets,
                attackers,
            }),
            enabled: Some(Pages::new(vec![
                [u64::MAX; WORDS];
                active_edges.div_ceil(CHUNK)
            ])),
            effective_damage,
            base_damage: conflict.damage,
            active_edges,
        })
    }

    /// Evaluate against sorted, unique committed people. Every edge is evaluated
    /// before effects are applied: a killed attacker still delivers its strike.
    pub fn prepare(&self, people: &[PersonSnapshot], tick: u64) -> Round {
        self.prepare_with(tick, |person, region| {
            person_index(people, person)
                .is_some_and(|index| people[index].1 == Some(region) && people[index].2 > 0)
        })
    }
    /// Direct committed physical reads avoid building an intermediate person snapshot.
    pub fn prepare_with(&self, tick: u64, available: impl Fn(u64, u64) -> bool + Sync) -> Round {
        self.prepare_with_filter(tick, available, |_, _| true)
    }
    /// Optional per-strike eligibility (for example range/visibility) reads the
    /// committed generation. It never permanently disables a cached edge.
    pub fn prepare_with_filter(
        &self,
        tick: u64,
        available: impl Fn(u64, u64) -> bool + Sync,
        eligible: impl Fn(u64, u64) -> bool + Sync,
    ) -> Round {
        let topology = &self.topology;
        let valid = self.validity(available, eligible);
        let damage = if self.active_edges == 0 {
            Vec::new()
        } else {
            topology
                .targets
                .par_iter()
                .enumerate()
                .filter_map(|(index, target)| {
                    let count =
                        count_range(&valid, topology.offsets[index], topology.offsets[index + 1]);
                    let amount = self
                        .effective_damage
                        .as_ref()
                        .map_or(target.damage, |values| values[index]);
                    (count > 0).then_some(Damage {
                        target: target.person,
                        amount: (u128::from(count) * u128::from(amount)).min(u128::from(u32::MAX))
                            as u32,
                    })
                })
                .collect()
        };
        Round {
            topology: Arc::clone(topology),
            tick,
            valid,
            damage,
            effective_damage: self.effective_damage.clone(),
            edge_damage: None,
        }
    }
    /// Lifecycle cleanup only removes edges and may turn surviving attackers into
    /// defenders. Retain the shared topology, with private masks and defense values.
    pub fn filtered(
        &self,
        survives: impl Fn(u64) -> bool + Sync,
        defends: impl Fn(u64, u64) -> bool + Sync,
    ) -> Self {
        if self.active_edges == 0 {
            return self.clone();
        }
        let enabled = self.validity(|person, _| survives(person), |_, _| true);
        let active_edges = enabled
            .par_iter()
            .map(|words| {
                words
                    .iter()
                    .map(|word| word.count_ones() as usize)
                    .sum::<usize>()
            })
            .sum();
        if active_edges == 0 {
            return Self {
                attack_slots: Arc::clone(&self.attack_slots),
                topology: Arc::clone(&self.topology),
                enabled: Some(Pages::new(enabled)),
                effective_damage: self.effective_damage.clone(),
                base_damage: self.base_damage,
                active_edges,
            };
        }
        let effective_damage = self
            .topology
            .targets
            .par_iter()
            .enumerate()
            .map(|(index, target)| {
                if defends(self.topology.encounters[target.encounter].id, target.person) {
                    (self.base_damage / 2).max(1)
                } else {
                    self.effective_damage
                        .as_ref()
                        .map_or(target.damage, |values| values[index])
                }
            })
            .collect();
        Self {
            attack_slots: Arc::clone(&self.attack_slots),
            topology: Arc::clone(&self.topology),
            enabled: Some(Pages::new(enabled)),
            effective_damage: Some(Pages::new(effective_damage)),
            base_damage: self.base_damage,
            active_edges,
        }
    }
    /// Disable original outgoing edges; replacement attacks are owned by an overlay.
    pub fn patch(&mut self, actors: &[u64], defense: &[(u64, bool)]) {
        for &id in actors {
            if let Ok(index) = self.attack_slots.binary_search_by_key(&id, |entry| entry.0) {
                let edge = self.attack_slots[index].1;
                let enabled = self.enabled.as_mut().expect("compiled mask");
                let mut words = enabled[edge / CHUNK];
                let word = (edge % CHUNK) / 64;
                let bit = 1 << (edge % 64);
                if words[word] & bit != 0 {
                    words[word] &= !bit;
                    enabled.set(edge / CHUNK, words);
                    self.active_edges -= 1;
                }
            }
        }
        for &(id, defends) in defense {
            if let Ok(index) = self
                .topology
                .targets
                .binary_search_by_key(&id, |target| target.person)
            {
                self.effective_damage
                    .as_mut()
                    .expect("compiled damage")
                    .set(
                        index,
                        if defends {
                            (self.base_damage / 2).max(1)
                        } else {
                            self.base_damage
                        },
                    );
            }
        }
    }
    fn validity(
        &self,
        available: impl Fn(u64, u64) -> bool + Sync,
        eligible: impl Fn(u64, u64) -> bool + Sync,
    ) -> Vec<[u64; WORDS]> {
        if self.active_edges == 0 {
            return Vec::new();
        }
        let topology = &self.topology;
        topology
            .attackers
            .par_chunks(CHUNK)
            .enumerate()
            .map(|(chunk, attackers)| {
                if self
                    .enabled
                    .as_ref()
                    .is_some_and(|enabled| enabled[chunk].iter().all(|&word| word == 0))
                {
                    return [0; WORDS];
                }
                let base = chunk * CHUNK;
                let mut target = topology.offsets.partition_point(|&offset| offset <= base) - 1;
                let mut words = [0u64; WORDS];
                let mut offset = 0;
                while offset < attackers.len() {
                    let end = (topology.offsets[target + 1] - base).min(attackers.len());
                    let info = &topology.targets[target];
                    let region = topology.encounters[info.encounter].region;
                    if available(info.person, region) {
                        for index in offset..end {
                            if self.enabled.as_ref().is_none_or(|enabled| {
                                enabled[chunk][index / 64] & (1u64 << (index % 64)) != 0
                            }) && available(attackers[index], region)
                                && eligible(attackers[index], info.person)
                            {
                                words[index / 64] |= 1u64 << (index % 64);
                            }
                        }
                    }
                    offset = end;
                    target += 1;
                }
                words
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
