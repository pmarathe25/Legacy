//! Boundary actions for ordinary-world production and goods cargo.
use game_parallel_goods_cargo::{Exchange, GoodsCargo, Recovery, Transfer};
use game_parallel_people::Region;
use game_parallel_production::Production;
use game_parallel_transport::Traveler;
use rayon::prelude::*;
/// Apply personal memories only after the physical exchange succeeded.
pub fn learn_help(opinions: Option<&mut game_parallel_opinions::Opinions>, exchanges: &[Exchange]) {
    let Some(opinions) = opinions.filter(|state| state.tracks_help()) else {
        return;
    };
    let gifts: Vec<_> = exchanges
        .par_iter()
        .filter_map(
            |exchange| match (exchange.offer_a.is_empty(), exchange.offer_b.is_empty()) {
                (false, true) => Some((exchange.b, exchange.a)),
                (true, false) => Some((exchange.a, exchange.b)),
                _ => None,
            },
        )
        .collect();
    opinions.learn_help(&gifts);
}
pub struct Context<'a> {
    pub production: &'a mut Option<Production>,
    pub cargo: &'a mut Option<GoodsCargo>,
    pub regions: &'a [Region],
    pub travelers: &'a [Traveler],
    pub tick: u64,
    pub property: Option<&'a game_parallel_property::Property>,
    pub suspended: Option<u64>,
}
impl Context<'_> {
    pub fn transfer(&mut self, requests: &[Transfer]) -> Result<(), String> {
        if self
            .suspended
            .is_some_and(|person| requests.iter().any(|request| request.person == person))
        {
            return Err("player control is suspended".into());
        }
        let production = self.production.as_ref().ok_or("production is disabled")?;
        let people = production.validate_people(self.regions, self.travelers, self.tick)?;
        if let Some(property) = self.property {
            property.check_transfers(&people, requests)?;
        }
        let cargo = self.cargo.as_ref().ok_or("goods cargo is disabled")?;
        let (next, inventories) = cargo.transfer(
            &production.inventories,
            &people,
            &production.goods,
            &production.busy_people(),
            requests,
        )?;
        self.production.as_mut().unwrap().inventories = inventories;
        *self.cargo = Some(next);
        Ok(())
    }
    pub fn recover(&mut self, request: Recovery) -> Result<(), String> {
        if self.suspended == Some(request.actor) {
            return Err("player control is suspended".into());
        }
        let production = self.production.as_ref().ok_or("production is disabled")?;
        let people = production.validate_people(self.regions, self.travelers, self.tick)?;
        if let Some(property) = self.property {
            property.check_recovery(&people, request.actor, request.good)?;
        }
        let cargo = self.cargo.as_ref().ok_or("goods cargo is disabled")?;
        let (next, inventories) = cargo.recover_indexed(
            &production.inventories,
            &people,
            &production.goods,
            &production.busy_people(),
            request,
        )?;
        self.production.as_mut().unwrap().inventories = inventories;
        *self.cargo = Some(next);
        Ok(())
    }
    pub fn exchange(&mut self, exchanges: &[Exchange]) -> Result<(), String> {
        if self.suspended.is_some_and(|person| {
            exchanges
                .iter()
                .any(|exchange| exchange.a == person || exchange.b == person)
        }) {
            return Err("player control is suspended".into());
        }
        let production = self.production.as_ref().ok_or("production is disabled")?;
        let people = production.validate_people(self.regions, self.travelers, self.tick)?;
        let cargo = self.cargo.as_ref().ok_or("goods cargo is disabled")?;
        *self.cargo = Some(cargo.exchange(
            &people,
            &production.goods,
            &production.busy_people(),
            exchanges,
        )?);
        Ok(())
    }
}
