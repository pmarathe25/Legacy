//! Compose physical ecology, travel and shipments for an ordinary tick.
use game_parallel_cargo::Cargo;
use game_parallel_logistics::Shipment;
use game_parallel_people::{Region, Rules, ecology, people};
use game_parallel_transport::Traveler;
use rayon::prelude::*;
pub struct Context<'a> {
    pub regions: &'a mut Vec<Region>,
    pub travelers: &'a mut Vec<Traveler>,
    pub shipments: &'a mut Vec<Shipment>,
    pub rules: &'a Rules,
    pub cargo: Option<&'a mut Cargo>,
    pub seed: u64,
    pub tick: u64,
}
impl Context<'_> {
    pub fn advance(self) {
        self.regions.par_iter_mut().for_each(|region| {
            ecology::grow(region, self.rules);
            people::advance(region, self.rules, self.seed, self.tick);
        });
        game_parallel_transport::advance(self.regions, self.travelers, self.rules, self.cargo);
        game_parallel_logistics::advance(self.regions, self.shipments, self.rules);
    }
}
