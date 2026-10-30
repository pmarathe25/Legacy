//! Host adapter for simultaneous departure admission.
use crate::{Move, World};
impl World {
    pub fn move_people(&mut self, moves: &[Move]) -> Result<(), String> {
        self.move_people_inner(moves, self.suspended_person())
    }
    pub(crate) fn move_people_autonomous(&mut self, moves: &[Move]) -> Result<(), String> {
        self.move_people_inner(moves, None)
    }
    fn move_people_inner(&mut self, moves: &[Move], suspended: Option<u64>) -> Result<(), String> {
        game_parallel_departure::Context {
            regions: &mut self.regions,
            travelers: &mut self.travelers,
            routes: &self.routes,
            rules: &self.rules,
            cargo: self.cargo.as_mut(),
            busy: self
                .production
                .as_ref()
                .map_or_else(Vec::new, |production| production.busy_people()),
            suspended,
        }
        .move_people(moves)
    }
}
