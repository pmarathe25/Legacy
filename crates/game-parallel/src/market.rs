use crate::{TradeDecision, TradeOutcome, TradeProposal, World};
impl World {
    fn market_context(&self) -> game_parallel_market_auto::Context<'_> {
        game_parallel_market_auto::Context {
            market: self.market.as_ref(),
            production: self.production.as_ref(),
            cargo: self.goods_cargo.as_ref(),
            regions: &self.regions,
            travelers: &self.travelers,
            tick: self.tick,
            suspended: self.suspended_person(),
        }
    }
    pub fn propose_trade(&mut self, proposal: TradeProposal) -> Result<u64, String> {
        Ok(self.propose_trades(&[proposal])?[0])
    }
    /// Prepare a batch of independent offers; IDs follow the supplied order.
    pub fn propose_trades(&mut self, proposals: &[TradeProposal]) -> Result<Vec<u64>, String> {
        let (market, ids) = self.market_context().propose(proposals)?;
        self.market = Some(market);
        Ok(ids)
    }
    pub fn decide_trade(
        &mut self,
        id: u64,
        actor: u64,
        outcome: TradeOutcome,
    ) -> Result<(), String> {
        self.decide_trades(&[TradeDecision { id, actor, outcome }])
    }
    /// Decisions and all accepted exchanges commit as one batch.
    pub fn decide_trades(&mut self, decisions: &[TradeDecision]) -> Result<(), String> {
        let (market, cargo, gifts) = self.market_context().decide(decisions)?;
        if let Some(cargo) = cargo {
            self.goods_cargo = Some(cargo);
        }
        self.market = Some(market);
        if let Some(opinions) = &mut self.opinions {
            opinions.learn_help(&gifts);
        }
        Ok(())
    }
}
