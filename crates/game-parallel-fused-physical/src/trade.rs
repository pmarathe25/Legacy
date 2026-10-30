//! Checked custody and market commands against a committed world.
use super::Kernel;
use game_parallel::{GoodsExchange, GoodsTransfer, TradeDecision, TradeOutcome, TradeProposal};
impl Kernel {
    pub fn transfer_goods(&mut self, requests: &[GoodsTransfer]) -> Result<(), String> {
        let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
        game_parallel_fused_goods::transfer(
            &mut self.template,
            &self.blocks,
            &self.positions,
            &mut self.owners.goods,
            requests,
        )?;
        self.revision = revision;
        Ok(())
    }
    /// Attempt a physical unauthorized taking; witnessed owners learn of it.
    pub fn steal_goods(
        &mut self,
        actor: u64,
        good: u32,
        quantity: u64,
    ) -> Result<game_parallel::PropertyTheft, String> {
        game_parallel_fused_conflict::check(&self.template, &[actor])?;
        self.edit_world(|world| game_parallel_block_property::steal(world, actor, good, quantity))
    }
    pub fn exchange_goods(&mut self, requests: &[GoodsExchange]) -> Result<(), String> {
        game_parallel_fused_conflict::check(
            &self.template,
            &requests
                .iter()
                .flat_map(|request| [request.a, request.b])
                .collect::<Vec<_>>(),
        )?;
        self.edit_world(|w| w.exchange_goods(requests))
    }
    pub fn recover_goods(
        &mut self,
        actor: u64,
        deceased: u64,
        good: u32,
        quantity: u64,
    ) -> Result<(), String> {
        game_parallel_fused_conflict::check(&self.template, &[actor])?;
        self.edit_world(|w| w.recover_goods(actor, deceased, good, quantity))
    }
    pub fn propose_trades(&mut self, requests: &[TradeProposal]) -> Result<Vec<u64>, String> {
        game_parallel_fused_conflict::check(
            &self.template,
            &requests
                .iter()
                .flat_map(|request| [request.proposer, request.recipient])
                .collect::<Vec<_>>(),
        )?;
        self.edit_world(|w| w.propose_trades(requests))
    }
    pub fn propose_trade(&mut self, request: TradeProposal) -> Result<u64, String> {
        game_parallel_fused_conflict::check(
            &self.template,
            &[request.proposer, request.recipient],
        )?;
        self.edit_world(|w| w.propose_trade(request))
    }
    pub fn decide_trades(&mut self, requests: &[TradeDecision]) -> Result<(), String> {
        game_parallel_fused_conflict::check(
            &self.template,
            &requests
                .iter()
                .map(|request| request.actor)
                .collect::<Vec<_>>(),
        )?;
        game_parallel_fused_conflict::check_decisions(&self.template, requests)?;
        self.edit_world(|w| w.decide_trades(requests))
    }
    pub fn decide_trade(
        &mut self,
        id: u64,
        actor: u64,
        outcome: TradeOutcome,
    ) -> Result<(), String> {
        game_parallel_fused_conflict::check(&self.template, &[actor])?;
        game_parallel_fused_conflict::check_decisions(
            &self.template,
            &[game_parallel::TradeDecision { id, actor, outcome }],
        )?;
        self.edit_world(|w| w.decide_trade(id, actor, outcome))
    }
}
