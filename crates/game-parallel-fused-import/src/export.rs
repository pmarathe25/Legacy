//! Export a committed fused generation.
use game_parallel::{Job, Person, Traveler, World};
use game_parallel_fused_data::{Block, NONE};
use game_parallel_fused_metadata::Store;
use game_parallel_production_receipts::Receipts;
use rayon::prelude::*;
pub fn export(
    template: &World,
    blocks: &[Block],
    food: &[u32],
    jobs: &Store,
    waiting: &[usize],
    receipts: &Receipts,
) -> World {
    let mut world = template.clone();
    for (region, &food) in world.regions.iter_mut().zip(food) {
        region.food = food;
    }
    for block in blocks {
        let people = (0..block.ids.len()).map(|i| Person {
            id: block.ids[i],
            health: block.health[i],
            hunger: block.hunger[i],
        });
        if let Some(journey) = block.journey {
            world.travelers.extend(people.map(|person| Traveler {
                person,
                source: journey.source,
                destination: journey.destination,
                ticks_remaining: journey.remaining,
            }));
        } else {
            world.regions[block.region].people.extend(people);
        }
    }
    world
        .travelers
        .par_sort_unstable_by_key(|traveler| traveler.person.id);
    for region in &mut world.regions {
        region.people.par_sort_unstable_by_key(|person| person.id);
    }
    let mut rows: Vec<Job> = blocks
        .par_iter()
        .flat_map_iter(|block| {
            (0..block.work.len()).filter_map(|i| {
                let handle = block.work[i];
                (handle != NONE).then(|| jobs.job(handle, block.remaining[i], false))
            })
        })
        .chain(waiting.par_iter().map(|&handle| jobs.job(handle, 0, true)))
        .collect();
    rows.par_sort_unstable_by_key(|job| job.id);
    if let Some(production) = world.production.as_mut() {
        production.jobs = rows;
        production.events = receipts.expand();
    }
    world
}

/// Materialize every owner at the external snapshot boundary.
pub struct Context<'a> {
    pub template: &'a World,
    pub blocks: &'a [Block],
    pub food: &'a [u32],
    pub jobs: &'a Store,
    pub waiting: &'a [usize],
    pub receipts: &'a Receipts,
    pub positions: &'a [(u64, usize, usize)],
    pub owners: &'a super::Owners,
    pub fatigue_dirty: bool,
    pub disease_dirty: bool,
}
impl Context<'_> {
    pub fn export(&self) -> World {
        let mut world = export(
            self.template,
            self.blocks,
            self.food,
            self.jobs,
            self.waiting,
            self.receipts,
        );
        game_parallel_fused_goods::export(
            &mut world,
            self.blocks,
            self.positions,
            self.owners.goods.as_ref(),
        );
        if let (Some(runtime), Some(families)) = (&self.owners.family, &mut world.families) {
            families.members = runtime.store.export();
            families.events = runtime.events();
        }
        if let (Some(runtime), Some(social)) = (&self.owners.social, &mut world.social) {
            social.relationships = runtime.export();
        }
        if let Some(autonomy) = &self.owners.autonomy {
            world.autonomy = Some(autonomy.export());
        }
        if let Some(knowledge) = &self.owners.knowledge {
            world.knowledge = Some(knowledge.export());
        }
        if self.fatigue_dirty
            && let Some(fatigue) = &mut world.fatigue
        {
            game_parallel_fused_needs::fatigue::export(fatigue, self.blocks, self.positions);
        }
        game_parallel_fused_nutrition::export_nutrition!(world, self);
        if self.disease_dirty
            && let Some(disease) = &mut world.disease
        {
            game_parallel_fused_disease::export(disease, self.blocks, self.positions);
        }
        if let (Some(runtime), Some(conflict)) =
            (self.owners.conflict.as_ref(), &mut world.conflict)
        {
            runtime.export(conflict);
        }
        world
    }
}
