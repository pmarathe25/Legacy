//! Stage lifecycle changes against private next-generation block buffers.
use game_parallel_families::{Families, LifeInput};
pub use game_parallel_family_lifecycle::Runtime;
use game_parallel_family_model::PhysicalView;
use game_parallel_fused_data::{BLOCK, Block};
use game_parallel_fused_metadata::Store;
use game_parallel_people::{Person, Region};
use rayon::prelude::*;
use std::collections::HashMap;
struct View<'a> {
    blocks: &'a [Block],
    regions: &'a [Region],
    positions: &'a [(u64, usize, usize)],
    dense: Option<u64>,
    controlled: Option<u64>,
}
impl PhysicalView for View<'_> {
    fn controlled(&self, id: u64) -> bool {
        self.controlled == Some(id)
    }
    fn read(&self, id: u64) -> LifeInput {
        let index = self
            .dense
            .and_then(|first| id.checked_sub(first))
            .and_then(|offset| usize::try_from(offset).ok())
            .filter(|&i| i < self.positions.len())
            .or_else(|| {
                if self.dense.is_none() {
                    self.positions.binary_search_by_key(&id, |row| row.0).ok()
                } else {
                    None
                }
            })
            .expect("validated family identity");
        let (_, block, offset) = self.positions[index];
        let block = &self.blocks[block];
        let region = match block.journey {
            None => Some(self.regions[block.region].id),
            Some(journey) if journey.remaining == 1 => Some(journey.destination),
            Some(_) => None,
        };
        LifeInput {
            id,
            region,
            health: block.next_health[offset],
            hunger: block.next_hunger[offset],
        }
    }
}
pub struct Context<'a> {
    pub blocks: &'a [Block],
    pub regions: &'a [Region],
    pub positions: &'a [(u64, usize, usize)],
    pub jobs: &'a Store,
    pub runtime: &'a mut Runtime,
    pub seed: u64,
    pub tick: u64,
    pub max_health: u32,
    pub lifespan_loss: Option<(u64, u64)>,
    pub controlled: Option<u64>,
}
pub struct Draft {
    family: game_parallel_family_lifecycle::Draft,
    births: Vec<Block>,
}
impl Draft {
    pub fn mark_deaths(&self, blocks: &mut [Block], positions: &[(u64, usize, usize)]) {
        if self.family.deaths.is_empty() {
            return;
        }
        let mut slots: Vec<_> = self
            .family
            .deaths
            .iter()
            .map(|id| {
                let index = positions
                    .binary_search_by_key(id, |row| row.0)
                    .expect("validated family identity");
                (positions[index].1, positions[index].2)
            })
            .collect();
        slots.par_sort_unstable();
        blocks
            .par_iter_mut()
            .enumerate()
            .for_each(|(index, block)| {
                let start = slots.partition_point(|slot| slot.0 < index);
                let end = slots.partition_point(|slot| slot.0 <= index);
                for &(_, offset) in &slots[start..end] {
                    block.next_health[offset] = 0;
                }
            });
    }
    pub fn commit(
        self,
        blocks: &mut Vec<Block>,
        families: &mut Option<Families>,
        runtime: &mut Runtime,
    ) -> bool {
        let born = !self.births.is_empty();
        blocks.extend(self.births);
        runtime
            .commit(self.family)
            .expect("validated family generation");
        let families = families.as_mut().expect("enabled family owner");
        families.members.clear();
        families.next_id = runtime.next_id;
        families.last_tick = runtime.last_tick;
        families.totals = runtime.totals.clone();
        families.events.clear();
        born
    }
}
impl Context<'_> {
    pub fn prepare(&mut self) -> Result<Draft, &'static str> {
        if self.runtime.rules.newborn_health > self.max_health {
            return Err("newborn health exceeds maximum");
        }
        let dense = self.positions.first().and_then(|first| {
            self.positions.last().and_then(|last| {
                (last.0.checked_sub(first.0) == Some((self.positions.len() - 1) as u64))
                    .then_some(first.0)
            })
        });
        let physical = View {
            blocks: self.blocks,
            regions: self.regions,
            positions: self.positions,
            dense,
            controlled: self.controlled,
        };
        let draft = self
            .runtime
            .prepare(&physical, self.seed, self.tick, self.lifespan_loss)?;
        let regions: HashMap<_, _> = self
            .regions
            .iter()
            .enumerate()
            .map(|(index, region)| (region.id, index))
            .collect();
        let mut grouped: Vec<_> = draft
            .births
            .par_iter()
            .map(|birth| -> Result<_, &'static str> {
                let (region, journey) = if let Some(region) = birth.region {
                    (*regions.get(&region).ok_or("birth region missing")?, None)
                } else {
                    let index = self
                        .positions
                        .binary_search_by_key(&birth.mother, |row| row.0)
                        .map_err(|_| "traveling mother missing")?;
                    let block = &self.blocks[self.positions[index].1];
                    let mut journey = block.journey.ok_or("traveling mother missing")?;
                    journey.remaining = journey
                        .remaining
                        .checked_sub(1)
                        .filter(|&remaining| remaining > 0)
                        .ok_or("invalid birth journey")?;
                    (block.region, Some(journey))
                };
                Ok((
                    region,
                    journey,
                    Person {
                        id: birth.id,
                        hunger: 0,
                        health: birth.health,
                    },
                ))
            })
            .collect::<Result<_, _>>()?;
        if grouped
            .par_windows(2)
            .any(|pair| (pair[0].0, pair[0].1, pair[0].2.id) > (pair[1].0, pair[1].1, pair[1].2.id))
        {
            grouped.par_sort_unstable_by_key(|row| (row.0, row.1, row.2.id));
        }
        let empty = HashMap::new();
        let regions = self.regions;
        let jobs = self.jobs;
        let births = grouped
            .par_chunk_by(|a, b| (a.0, a.1) == (b.0, b.1))
            .flat_map(|group| {
                let empty = &empty;
                group.par_chunks(BLOCK).map(move |chunk| {
                    let (region, journey) = (chunk[0].0, chunk[0].1);
                    let people: Vec<_> = chunk.iter().map(|row| row.2.clone()).collect();
                    Block::new(region, regions[region].id, &people, journey, empty, jobs)
                })
            })
            .collect();
        Ok(Draft {
            family: draft,
            births,
        })
    }
}

/// Prepare and mark a family's unpublished lifecycle generation.
pub fn prepare_optional(
    world: &game_parallel::World,
    runtime: Option<&mut Runtime>,
    blocks: &mut [Block],
    positions: &[(u64, usize, usize)],
    jobs: &Store,
    next: u64,
    lifespan_loss: Option<(u64, u64)>,
) -> Result<Option<Draft>, &'static str> {
    let Some(runtime) = runtime else {
        return Ok(None);
    };
    let draft = Context {
        blocks,
        regions: &world.regions,
        positions,
        jobs,
        runtime,
        seed: world.seed,
        tick: next,
        max_health: world.rules.max_health,
        lifespan_loss,
        controlled: world
            .lineage
            .as_ref()
            .and_then(|lineage| lineage.controlled_person(world.tick)),
    }
    .prepare()?;
    draft.mark_deaths(blocks, positions);
    Ok(Some(draft))
}
