//! The session protocol delegates all game behavior to the authoritative world.
use crate::*;

impl SessionEngine for Simulation {
    fn time(&self) -> SimTime {
        Simulation::time(self)
    }
    fn revision(&self) -> u64 {
        self.revision
    }
    fn observe(&self) -> PlayerView {
        Simulation::observe(self)
    }
    fn submit(&mut self, submission: Submission) -> Result<CommandReceipt, Error> {
        Simulation::submit(self, submission)
    }
    fn next_caller_sequence(&self) -> Result<u64, Error> {
        Simulation::next_caller_sequence(self)
    }
    fn advance(&mut self, duration: SimDuration) -> Result<AdvanceResult, Error> {
        Simulation::advance(self, duration)
    }
    fn save(&self) -> Result<Vec<u8>, Error> {
        Simulation::save(self)
    }
    fn restored(&self, bytes: &[u8]) -> Result<Self, Error> {
        Simulation::load(bytes, self.execution.clone())
    }
    fn debug_json(&self) -> Result<String, Error> {
        serde_json::to_string(self.canonical_state())
            .map_err(|error| Error::InvalidSave(error.to_string()))
    }
}
