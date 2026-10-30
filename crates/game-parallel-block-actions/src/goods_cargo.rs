use crate::Context;
use game_parallel::{GoodsExchange, GoodsTransfer};
impl Context<'_> {
    pub fn transfer_goods(
        &mut self,
        requests: &[GoodsTransfer],
        busy: &[u64],
    ) -> Result<(), String> {
        let production = self
            .world
            .production
            .as_ref()
            .ok_or("production is disabled")?;
        let cargo = self
            .world
            .goods_cargo
            .as_ref()
            .ok_or("goods cargo is disabled")?;
        let (cargo, inventories) =
            cargo.transfer_validated(&production.inventories, self.people, busy, requests)?;
        self.world.production.as_mut().unwrap().inventories = inventories;
        self.world.goods_cargo = Some(cargo);
        Ok(())
    }
    pub fn exchange_goods(
        &mut self,
        requests: &[GoodsExchange],
        busy: &[u64],
    ) -> Result<(), String> {
        let production = self
            .world
            .production
            .as_ref()
            .ok_or("production is disabled")?;
        let cargo = self
            .world
            .goods_cargo
            .as_ref()
            .ok_or("goods cargo is disabled")?;
        self.world.goods_cargo =
            Some(cargo.exchange(self.people, &production.goods, busy, requests)?);
        Ok(())
    }
    pub fn recover_goods(
        &mut self,
        actor: u64,
        deceased: u64,
        good: u32,
        quantity: u64,
        busy: &[u64],
    ) -> Result<(), String> {
        let production = self
            .world
            .production
            .as_ref()
            .ok_or("production is disabled")?;
        let cargo = self
            .world
            .goods_cargo
            .as_ref()
            .ok_or("goods cargo is disabled")?;
        let (next, inventories) = cargo.recover_indexed(
            &production.inventories,
            self.people,
            &production.goods,
            busy,
            game_parallel_goods_cargo::Recovery {
                actor,
                deceased,
                good,
                quantity,
            },
        )?;
        self.world.production.as_mut().unwrap().inventories = inventories;
        self.world.goods_cargo = Some(next);
        Ok(())
    }
}
