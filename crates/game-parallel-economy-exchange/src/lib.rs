//! Individual exchange and requests, reconciled against real lot transfers.
use game_parallel_lot_projection::market;
use game_parallel_lot_runtime::Events;
use game_parallel_lots::{Transaction, View};
use game_parallel_market::Market;
use game_parallel_opinions::Opinions;
use game_parallel_production_model::{Good, PersonSnapshot};
use rayon::prelude::*;

fn reserve(busy: &mut Vec<u64>, actors: impl IntoIterator<Item = u64>) {
    busy.extend(actors);
    busy.par_sort_unstable();
    busy.dedup();
}
pub struct Context<'a> {
    pub market: Option<&'a Market>,
    pub view: &'a View,
    pub capacity: u64,
    pub people: &'a [PersonSnapshot],
    pub goods: &'a [Good],
    pub opinions: Option<&'a Opinions>,
}
impl Context<'_> {
    pub fn trade(
        &self,
        busy: &mut Vec<u64>,
        action_base: u64,
    ) -> Result<Option<market::Plan>, String> {
        let Some(market) = self.market else {
            return Ok(None);
        };
        let plan = market::Context {
            market,
            view: self.view,
            capacity: self.capacity,
            people: self.people,
            goods: self.goods,
            busy,
            opinions: self.opinions,
        }
        .plan(action_base)?;
        reserve(busy, plan.actors());
        Ok(Some(plan))
    }
    pub fn proposals(
        &self,
        busy: &mut Vec<u64>,
    ) -> Result<Option<game_parallel_market_proposals::Draft>, String> {
        let Some(market) = self.market else {
            return Ok(None);
        };
        let cargo = game_parallel_lot_projection::cargo(self.view, self.capacity)?;
        let plan = game_parallel_market_proposals::prepare(
            market,
            &cargo,
            self.people,
            busy,
            self.view.boundary,
        )?;
        if let Some(plan) = &plan {
            reserve(busy, plan.actors.iter().copied());
        }
        Ok(plan)
    }
}
pub fn actions(plan: Option<&market::Plan>) -> &[Transaction] {
    plan.map_or(&[], market::Plan::actions)
}
pub fn publish(
    trade: Option<&market::Plan>,
    proposals: Option<game_parallel_market_proposals::Draft>,
    market: &mut Option<Market>,
    opinions: &mut Option<Opinions>,
    events: &Events,
) -> Result<(), String> {
    if let (Some(plan), Some(market)) = (trade, market.as_mut())
        && let Some(decisions) = plan.decisions(market, events)?
    {
        if let Some(opinions) = opinions {
            opinions.learn_help(decisions.gifts());
        }
        decisions.commit(market);
    }
    if let (Some(proposals), Some(market)) = (proposals, market) {
        proposals.commit(market);
    }
    Ok(())
}
