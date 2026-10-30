//! Host adapters for autonomous decisions and departures.
use crate::World;
use game_parallel_autonomy::{Draft, adapter};

pub(crate) fn prepare(world: &World, busy: &[u64]) -> Result<Option<Draft>, &'static str> {
    adapter::prepare(
        world.autonomy.as_ref(),
        world.knowledge.as_ref(),
        &world.regions,
        &world.travelers,
        &world.routes,
        world.tick,
        busy,
    )
}
pub(crate) fn publish(world: &mut World, draft: Draft) {
    let (requests, available) = adapter::requests(&draft, &world.regions);
    let result = if requests.is_empty() {
        Ok(())
    } else {
        world.move_people_autonomous(&requests)
    };
    world.autonomy = Some(adapter::finish(draft, &available, &result, world.tick));
}
