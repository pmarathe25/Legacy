//! Shared immutable combat topology and lazy attributed receipts.
use game_parallel_conflict::Event;
use rayon::prelude::*;
use std::sync::Arc;
mod pages;
pub use pages::Pages;
pub const WORDS: usize = 32;
pub const CHUNK: usize = WORDS * 64;

pub struct Encounter {
    pub id: u64,
    pub region: u64,
}
pub struct Target {
    pub person: u64,
    pub encounter: usize,
    pub damage: u32,
}
pub struct Topology {
    pub encounters: Vec<Encounter>,
    pub targets: Vec<Target>,
    pub offsets: Vec<usize>,
    pub attackers: Vec<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Damage {
    pub target: u64,
    pub amount: u32,
}

pub struct Round {
    pub topology: Arc<Topology>,
    pub tick: u64,
    pub valid: Vec<[u64; WORDS]>,
    pub damage: Vec<Damage>,
    pub effective_damage: Option<Pages<u32>>,
    /// Populated only for rounds with attacker-specific damage.
    pub edge_damage: Option<Vec<u32>>,
}

pub fn count_range(valid: &[[u64; WORDS]], start: usize, end: usize) -> u64 {
    if start == end {
        return 0;
    }
    let first = start / 64;
    let last = (end - 1) / 64;
    let word = |index: usize| valid[index / WORDS][index % WORDS];
    let first_mask = u64::MAX << (start % 64);
    let last_mask = u64::MAX >> (63 - ((end - 1) % 64));
    if first == last {
        return u64::from((word(first) & first_mask & last_mask).count_ones());
    }
    let middle: u64 = if last - first > 256 {
        (first + 1..last)
            .into_par_iter()
            .map(|i| u64::from(word(i).count_ones()))
            .sum()
    } else {
        (first + 1..last)
            .map(|i| u64::from(word(i).count_ones()))
            .sum()
    };
    u64::from((word(first) & first_mask).count_ones())
        + u64::from((word(last) & last_mask).count_ones())
        + middle
}

fn aggregate(topology: &Topology, valid: &[[u64; WORDS]], values: &[u32]) -> Vec<Damage> {
    topology
        .targets
        .par_iter()
        .enumerate()
        .filter_map(|(index, target)| {
            let start = topology.offsets[index];
            let end = topology.offsets[index + 1];
            if count_range(valid, start, end) == 0 {
                return None;
            }
            let rows = &values[start..end];
            let sum: u128 = if rows.len() > CHUNK {
                rows.par_iter().map(|&d| u128::from(d)).sum()
            } else {
                rows.iter().map(|&d| u128::from(d)).sum()
            };
            Some(Damage {
                target: target.person,
                amount: sum.min(u128::from(u32::MAX)) as u32,
            })
        })
        .collect()
}

impl Round {
    /// Resolve attacker-specific damage in parallel, including a single hot target.
    pub fn vary_damage(&mut self, damage: impl Fn(u64, u64) -> u32 + Sync) {
        let topology = &self.topology;
        let valid = &self.valid;
        let damage = &damage;
        let values: Vec<u32> = topology
            .attackers
            .par_chunks(CHUNK)
            .enumerate()
            .flat_map_iter(|(chunk, attackers)| {
                let base = chunk * CHUNK;
                let mut target = topology.offsets.partition_point(|&offset| offset <= base) - 1;
                (base..base + attackers.len()).map(move |edge| {
                    while topology.offsets[target + 1] <= edge {
                        target += 1;
                    }
                    let word = valid[edge / CHUNK][(edge % CHUNK) / 64];
                    if word & (1u64 << (edge % 64)) == 0 {
                        0
                    } else {
                        damage(topology.attackers[edge], topology.targets[target].person)
                    }
                })
            })
            .collect();
        self.damage = aggregate(&self.topology, &self.valid, &values);
        self.edge_damage = Some(values);
    }
    /// Reduce each incoming strike using the target's actual protection.
    /// All strikes at one target share this value, preserving compact hot-target reduction.
    pub fn protect(&mut self, reduction: impl Fn(u64) -> u32 + Sync) {
        if let Some(values) = &mut self.edge_damage {
            let reductions: Vec<_> = self
                .topology
                .targets
                .par_iter()
                .map(|target| reduction(target.person))
                .collect();
            let offsets = &self.topology.offsets;
            values
                .par_chunks_mut(CHUNK)
                .enumerate()
                .for_each(|(chunk, rows)| {
                    let base = chunk * CHUNK;
                    let mut target = offsets.partition_point(|&offset| offset <= base) - 1;
                    for (i, value) in rows.iter_mut().enumerate() {
                        while offsets[target + 1] <= base + i {
                            target += 1;
                        }
                        *value = value.saturating_sub(reductions[target]);
                    }
                });
            self.damage = aggregate(&self.topology, &self.valid, values);
            return;
        }
        let effective: Vec<_> = self
            .topology
            .targets
            .par_iter()
            .enumerate()
            .map(|(index, target)| {
                let damage = self
                    .effective_damage
                    .as_ref()
                    .map_or(target.damage, |values| values[index]);
                damage.saturating_sub(reduction(target.person))
            })
            .collect();
        self.damage = self
            .topology
            .targets
            .par_iter()
            .enumerate()
            .filter_map(|(index, target)| {
                let count = count_range(
                    &self.valid,
                    self.topology.offsets[index],
                    self.topology.offsets[index + 1],
                );
                (count > 0).then_some(Damage {
                    target: target.person,
                    amount: (u128::from(count) * u128::from(effective[index]))
                        .min(u128::from(u32::MAX)) as u32,
                })
            })
            .collect();
        self.effective_damage = Some(Pages::new(effective));
    }
    pub fn damage(&self) -> &[Damage] {
        &self.damage
    }
    pub fn event_count(&self) -> u64 {
        self.valid
            .par_iter()
            .map(|words| {
                words
                    .iter()
                    .map(|word| u64::from(word.count_ones()))
                    .sum::<u64>()
            })
            .sum()
    }
    fn event(&self, target_index: usize, edge: usize) -> Option<Event> {
        let target = &self.topology.targets[target_index];
        let word = self.valid.get(edge / CHUNK)?[(edge % CHUNK) / 64];
        (word & (1u64 << (edge % 64)) != 0).then(|| Event {
            tick: self.tick,
            encounter: self.topology.encounters[target.encounter].id,
            attacker: self.topology.attackers[edge],
            target: target.person,
            damage: self.edge_damage.as_ref().map_or_else(
                || {
                    self.effective_damage
                        .as_ref()
                        .map_or(target.damage, |values| values[target_index])
                },
                |values| values[edge],
            ),
        })
    }
    /// Stream canonical (target, encounter, attacker) receipts without expanding
    /// a whole round. No attacker attribution is discarded when damage saturates.
    pub fn events(&self) -> impl Iterator<Item = Event> + '_ {
        self.topology
            .targets
            .iter()
            .enumerate()
            .flat_map(move |(index, _)| {
                (self.topology.offsets[index]..self.topology.offsets[index + 1])
                    .filter_map(move |edge| self.event(index, edge))
            })
    }
    /// Compatibility/export boundary. This allocation is deliberately outside
    /// simulation preparation and can be replaced by streaming serialization.
    pub fn expand(&self) -> Vec<Event> {
        if self.valid.is_empty() {
            return Vec::new();
        }
        self.topology
            .attackers
            .par_chunks(CHUNK)
            .enumerate()
            .flat_map_iter(|(chunk, attackers)| {
                let base = chunk * CHUNK;
                let mut target = self
                    .topology
                    .offsets
                    .partition_point(|&offset| offset <= base)
                    - 1;
                (base..base + attackers.len()).filter_map(move |edge| {
                    while self.topology.offsets[target + 1] <= edge {
                        target += 1;
                    }
                    self.event(target, edge)
                })
            })
            .collect()
    }
}
