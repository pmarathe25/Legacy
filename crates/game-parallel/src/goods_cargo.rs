use crate::{GoodsExchange, GoodsTransfer, World};
impl World {
    fn goods_actions(&mut self) -> game_parallel_goods_actions::Context<'_> {
        let suspended = self.suspended_person();
        game_parallel_goods_actions::Context {
            production: &mut self.production,
            cargo: &mut self.goods_cargo,
            regions: &self.regions,
            travelers: &self.travelers,
            tick: self.tick,
            property: self.property.as_ref(),
            suspended,
        }
    }
    /// Reserve all requested transfers against the same stock snapshot.
    pub fn transfer_goods(&mut self, requests: &[GoodsTransfer]) -> Result<(), String> {
        self.goods_actions().transfer(requests)
    }
    pub fn recover_goods(
        &mut self,
        actor: u64,
        deceased: u64,
        good: u32,
        quantity: u64,
    ) -> Result<(), String> {
        self.goods_actions()
            .recover(game_parallel_goods_cargo::Recovery {
                actor,
                deceased,
                good,
                quantity,
            })
    }
    pub fn exchange_goods(&mut self, exchanges: &[GoodsExchange]) -> Result<(), String> {
        self.goods_actions().exchange(exchanges)?;
        game_parallel_goods_actions::learn_help(self.opinions.as_mut(), exchanges);
        Ok(())
    }
}
