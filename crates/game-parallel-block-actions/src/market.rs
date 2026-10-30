//! Explicit market actions against the persistent physical index.
use crate::Context;
use game_parallel::{GoodsExchange, TradeDecision, TradeOutcome, TradeProposal};
use game_parallel_production_model::person_index;
use rayon::prelude::*;
impl Context<'_> {
    pub fn propose_trade(&mut self, proposal: TradeProposal) -> Result<u64, String> {
        Ok(self.propose_trades(&[proposal])?[0])
    }
    pub fn propose_trades(&mut self, proposals: &[TradeProposal]) -> Result<Vec<u64>, String> {
        let state = self
            .world
            .production
            .as_ref()
            .ok_or("production is disabled")?;
        state.validate_index(&self.world.regions, self.people, self.world.tick)?;
        let cargo = self
            .world
            .goods_cargo
            .as_ref()
            .ok_or("goods cargo is disabled")?;
        cargo.validate_index(self.people, &state.goods)?;
        let market = self.world.market.as_ref().ok_or("market is disabled")?;
        let goods = state.goods.iter().map(|g| g.id).collect();
        market.validate_index(self.people, &goods, self.world.tick)?;
        let (next, ids) = game_parallel_market_auto::propose(
            market,
            state,
            self.people,
            proposals,
            self.world.tick,
        )?;
        self.world.market = Some(next);
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
    pub fn decide_trades(&mut self, decisions: &[TradeDecision]) -> Result<(), String> {
        let state = self
            .world
            .production
            .as_ref()
            .ok_or("production is disabled")?;
        state.validate_index(&self.world.regions, self.people, self.world.tick)?;
        let cargo = self
            .world
            .goods_cargo
            .as_ref()
            .ok_or("goods cargo is disabled")?;
        cargo.validate_index(self.people, &state.goods)?;
        let market = self.world.market.as_ref().ok_or("market is disabled")?;
        let goods = state.goods.iter().map(|g| g.id).collect();
        market.validate_index(self.people, &goods, self.world.tick)?;
        if decisions
            .par_iter()
            .any(|d| person_index(self.people, d.actor).is_none_or(|i| self.people[i].2 == 0))
        {
            return Err("decision maker must be living".into());
        }
        let (next, offers) = market.decide_batch(decisions, self.world.tick)?;
        let gifts = game_parallel::Market::gift_pairs(&offers, decisions);
        let exchanges: Vec<_> = decisions
            .par_iter()
            .zip(offers.into_par_iter())
            .filter(|(d, _)| d.outcome == TradeOutcome::Accepted)
            .map(|(_, offer)| GoodsExchange {
                a: offer.proposer,
                b: offer.recipient,
                offer_a: offer.give,
                offer_b: offer.receive,
            })
            .collect();
        let cargo = if exchanges.is_empty() {
            None
        } else {
            Some(cargo.exchange(self.people, &state.goods, &state.busy_people(), &exchanges)?)
        };
        if let Some(cargo) = cargo {
            self.world.goods_cargo = Some(cargo);
        }
        self.world.market = Some(next);
        if let Some(opinions) = &mut self.world.opinions {
            opinions.learn_help(&gifts);
        }
        Ok(())
    }
}
