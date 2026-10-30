//! Materialize a committed block generation as an editable world.
use game_parallel::{Person, Traveler, World};
use game_parallel_block_data::Block;
pub fn export(template: &World, blocks: &[Block], food: &[u32]) -> World {
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
        .sort_unstable_by_key(|traveler| traveler.person.id);
    world
}
