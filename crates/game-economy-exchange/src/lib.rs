//! Individual exchange and requests, reconciled against real lot transfers.
use game_lot_projection::market;
use game_lot_runtime::Events;
use game_lots::{Transaction, View};
use game_market::Market;
pub use game_market_auto::Together;
pub use game_market_proposals::EndpointLookup;
use game_opinions::Opinions;
use game_production_model::{Good, PersonSnapshot};
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
    pub endpoint: Option<&'a game_market_proposals::EndpointLookup<'a>>,
    pub together: Option<&'a game_market_auto::Together<'a>>,
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
            together: self.together,
        }
        .plan(action_base)?;
        reserve(busy, plan.actors());
        Ok(Some(plan))
    }
    pub fn proposals(
        &self,
        busy: &mut Vec<u64>,
    ) -> Result<Option<game_market_proposals::Draft>, String> {
        let Some(market) = self.market else {
            return Ok(None);
        };
        let cargo = game_lot_projection::cargo(self.view, self.capacity)?;
        let plan = game_market_proposals::prepare_at_endpoints(
            market,
            &cargo,
            self.people,
            busy,
            self.view.boundary,
            self.endpoint,
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
    proposals: Option<game_market_proposals::Draft>,
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
