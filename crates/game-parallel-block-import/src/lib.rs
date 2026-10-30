//! Validate and compile editable physical state into fixed blocks.
use game_parallel::World;
use game_parallel_block_data::{BLOCK, Block, Journey};
use rayon::prelude::*;
use std::{collections::HashMap, mem};
pub struct Compiled {
    pub world: World,
    pub blocks: Vec<Block>,
    pub food: Vec<u32>,
}
pub fn compile(mut world: World) -> Result<Compiled, String> {
    world.validate()?;
    if world
        .families
        .as_ref()
        .is_some_and(|families| families.rules.min_work_age > 0)
        || world
            .property
            .as_ref()
            .is_some_and(|property| property.inheritance.is_some())
        || world
            .agriculture
            .as_ref()
            .is_some_and(|owner| owner.recruitment.is_some())
        || world
            .food_effects
            .as_ref()
            .is_some_and(|effects| !effects.pending.is_empty())
        || world.personality.is_some()
        || world
            .social
            .as_ref()
            .is_some_and(|social| !social.pressures.is_empty())
        || world.nutrition.is_some()
        || world.missions.is_some()
        || world.civic_auto.is_some()
        || world.dreams.is_some()
        || world.caregiving.is_some()
        || world.purpose.is_some()
        || world.goals.is_some()
        || world.cargo.as_ref().is_some_and(|cargo| {
            cargo.auto_eat_local
                || cargo.personal_food_access
                || !cargo.food_aid.is_empty()
                || !cargo.food_sharing.is_empty()
        })
        || world.opinions.is_some()
        || world.autowork.as_ref().is_some_and(|auto| {
            auto.requests.is_some() || auto.policies.iter().any(|p| p.disregard_claims)
        })
        || world.agreements.is_some()
        || world.conflict.is_some()
        || world.care.is_some()
        || world.environment.is_some()
        || world.skills.is_some()
        || world.fatigue.is_some()
        || world.tactics.is_some()
        || world.disease.is_some()
        || game_parallel_lineage::has_physical_strain(world.lineage.as_ref())
    {
        return Err("configured owners require fused runtime".into());
    }
    let food: Vec<_> = world.regions.iter().map(|region| region.food).collect();
    let mut blocks = Vec::new();
    for (region_index, region) in world.regions.iter_mut().enumerate() {
        let mut people = mem::take(&mut region.people);
        people.sort_unstable_by_key(|p| p.id);
        for chunk in people.chunks(BLOCK) {
            blocks.push(Block::new(region_index, chunk));
        }
    }
    let regions: HashMap<_, _> = world
        .regions
        .iter()
        .enumerate()
        .map(|(index, region)| (region.id, index))
        .collect();
    let mut travelers = mem::take(&mut world.travelers);
    travelers
        .par_sort_unstable_by_key(|t| (t.source, t.destination, t.ticks_remaining, t.person.id));
    let mut travel_blocks: Vec<_> = travelers
        .par_chunk_by(|a, b| {
            (a.source, a.destination, a.ticks_remaining)
                == (b.source, b.destination, b.ticks_remaining)
        })
        .flat_map(|group| {
            let source = regions[&group[0].source];
            let journey = Journey {
                source: group[0].source,
                destination: group[0].destination,
                destination_index: regions[&group[0].destination],
                remaining: group[0].ticks_remaining,
            };
            group.par_chunks(BLOCK).map(move |chunk| {
                let people: Vec<_> = chunk
                    .iter()
                    .map(|traveler| traveler.person.clone())
                    .collect();
                let mut block = Block::new(source, &people);
                block.journey = Some(journey);
                block
            })
        })
        .collect();
    blocks.append(&mut travel_blocks);
    blocks.par_sort_unstable_by_key(|block| (block.region, block.ids[0]));
    Ok(Compiled {
        world,
        blocks,
        food,
    })
}

/// Owners that require the shared persistent person projection.
pub fn needs_people(world: &World) -> bool {
    world.production.is_some()
        || world.agriculture.is_some()
        || world.social.is_some()
        || world.knowledge.is_some()
        || world.families.is_some()
        || world.cargo.is_some()
        || world.institutions.is_some()
}
