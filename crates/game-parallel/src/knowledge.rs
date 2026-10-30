use crate::World;
use game_parallel_knowledge::Knowledge;
use game_parallel_social::Social;
pub(crate) fn prepare(
    world: &World,
    social: Option<&Social>,
) -> Result<Option<Knowledge>, &'static str> {
    game_parallel_knowledge::adapter::prepare(
        world.knowledge.as_ref(),
        &world.regions,
        &world.travelers,
        social,
        world.tick,
    )
}
