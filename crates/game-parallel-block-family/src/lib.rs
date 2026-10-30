//! Stage lifecycle changes against private next-generation block buffers.
use game_parallel_block_data::{BLOCK, Block, Journey};
use game_parallel_families::{Families, LifeInput};
use game_parallel_people::{Person, Region};
use game_parallel_production_model::{PersonSnapshot, person_index};
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};
pub struct Context<'a> {
    pub blocks: &'a [Block],
    pub regions: &'a [Region],
    pub people: &'a [PersonSnapshot],
    pub positions: &'a [(usize, usize)],
    pub families: &'a Families,
    pub seed: u64,
    pub tick: u64,
    pub max_health: u32,
}
pub struct Draft {
    families: Families,
    births: Vec<Block>,
    deaths: BTreeSet<u64>,
}
impl Draft {
    pub fn mark_deaths(&self, blocks: &mut [Block]) {
        blocks.par_iter_mut().for_each(|block| {
            for (id, health) in block.ids.iter().zip(&mut block.next_health) {
                if self.deaths.contains(id) {
                    *health = 0;
                }
            }
        });
    }
    pub fn commit(self, blocks: &mut Vec<Block>, families: &mut Option<Families>) -> bool {
        let born = !self.births.is_empty();
        blocks.extend(self.births);
        *families = Some(self.families);
        born
    }
}
impl Context<'_> {
    pub fn prepare(&self) -> Result<Draft, &'static str> {
        if self.families.rules.newborn_health > self.max_health {
            return Err("newborn health exceeds maximum");
        }
        let mut physical: Vec<LifeInput> = self
            .blocks
            .par_iter()
            .flat_map_iter(|block| {
                let region = match block.journey {
                    None => Some(self.regions[block.region].id),
                    Some(journey) if journey.remaining == 1 => Some(journey.destination),
                    Some(_) => None,
                };
                (0..block.ids.len()).map(move |i| LifeInput {
                    id: block.ids[i],
                    region,
                    health: block.next_health[i],
                    hunger: block.next_hunger[i],
                })
            })
            .collect();
        let draft = self.families.advance(&physical, self.seed, self.tick)?;
        let regions: HashMap<_, _> = self
            .regions
            .iter()
            .enumerate()
            .map(|(index, region)| (region.id, index))
            .collect();
        let mut groups: BTreeMap<(usize, Option<Journey>), Vec<Person>> = BTreeMap::new();
        for birth in &draft.births {
            let (region, journey) = if let Some(region) = birth.region {
                (*regions.get(&region).ok_or("birth region missing")?, None)
            } else {
                let index =
                    person_index(self.people, birth.mother).ok_or("traveling mother missing")?;
                let block = &self.blocks[self.positions[index].0];
                let mut journey = block.journey.ok_or("traveling mother missing")?;
                journey.remaining = journey
                    .remaining
                    .checked_sub(1)
                    .filter(|&remaining| remaining > 0)
                    .ok_or("invalid birth journey")?;
                (block.region, Some(journey))
            };
            groups.entry((region, journey)).or_default().push(Person {
                id: birth.id,
                hunger: 0,
                health: birth.health,
            });
        }
        physical.par_iter_mut().for_each(|person| {
            if draft.deaths.contains(&person.id) {
                person.health = 0;
            }
        });
        physical.extend(draft.births.iter().map(|birth| LifeInput {
            id: birth.id,
            region: birth.region,
            health: birth.health,
            hunger: 0,
        }));
        draft
            .families
            .validate_committed(&physical, self.tick, self.max_health)
            .map_err(|_| "invalid staged family state")?;
        let mut births = Vec::new();
        for ((region, journey), mut people) in groups {
            people.sort_unstable_by_key(|p| p.id);
            for chunk in people.chunks(BLOCK) {
                let mut block = Block::new(region, chunk);
                block.journey = journey;
                births.push(block);
            }
        }
        Ok(Draft {
            families: draft.families,
            births,
            deaths: draft.deaths,
        })
    }
}
