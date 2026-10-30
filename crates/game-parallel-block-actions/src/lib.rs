//! Boundary actions using an already validated compiled person index.
mod goods_cargo;
mod guardian;
mod invocation;
mod market;
mod production;
use game_parallel::World;
use game_parallel_production_model::PersonSnapshot;
pub struct Context<'a> {
    pub world: &'a mut World,
    pub people: &'a [PersonSnapshot],
}
