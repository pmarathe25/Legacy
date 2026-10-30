//! Prepare automatic trade before reserving other activities.
use game_parallel::World;
use game_parallel_fused_data::{Block, NONE};
use game_parallel_production_model::PersonSnapshot;
use rayon::prelude::*;
pub fn prepare(
    world: &World,
    blocks: &[Block],
    people: &[PersonSnapshot],
    next: u64,
    reserved: &[u64],
) -> Result<Option<game_parallel_market_auto::Draft>, &'static str> {
    let Some(market) = &world.market else {
        return Ok(None);
    };
    let production = world
        .production
        .as_ref()
        .ok_or("market requires production")?;
    let cargo = world
        .goods_cargo
        .as_ref()
        .ok_or("market requires goods cargo")?;
    let mut busy: Vec<_> = blocks
        .par_iter()
        .flat_map_iter(|block| {
            block
                .ids
                .iter()
                .zip(&block.work)
                .filter_map(|(&id, &work)| (work != NONE).then_some(id))
        })
        .collect();
    busy.extend_from_slice(reserved);
    game_parallel_market_auto::prepare_with_opinions(
        market,
        cargo,
        people,
        &production.goods,
        &busy,
        next,
        world.opinions.as_ref(),
    )
    .map_err(|_| "invalid automatic trade")
}

#[macro_export]
macro_rules! exchange_methods {
    () => {
        pub fn commit_exchanges(&mut self, world: &mut World) {
            if let Some(caregiving) = self.caregiving.take() {
                world
                    .caregiving
                    .as_mut()
                    .expect("prepared caregiving")
                    .events = caregiving.events;
            }
            if let Some(trade) = self.trade.take() {
                trade.commit(
                    world.market.as_mut().expect("prepared market"),
                    world.goods_cargo.as_mut().expect("prepared cargo"),
                    world.opinions.as_mut(),
                );
            }
            if let Some(agreements) = self.agreements.take() {
                agreements.commit(
                    world.agreements.as_mut().expect("prepared agreements"),
                    world.goods_cargo.as_mut().expect("prepared cargo"),
                );
            }
            game_parallel_opinions::advance_optional(
                &mut world.opinions,
                world.agreements.as_ref(),
                self.society
                    .social
                    .as_ref()
                    .map_or(&[], |social| social.events.as_slice()),
                world.tick,
            );
        }
    };
}
