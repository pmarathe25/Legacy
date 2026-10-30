//! Host adapter for shipment admission.
use crate::{Shipment, World};
impl World {
    pub fn launch_shipments(&mut self, shipments: &[Shipment]) -> Result<(), String> {
        game_parallel_logistics::Context {
            regions: &mut self.regions,
            routes: &self.routes,
            shipments: &mut self.shipments,
        }
        .launch_shipments(shipments)
    }
}
