//! Operations the session host needs from an authoritative engine.
use crate::*;

pub trait SessionEngine: Send + 'static {
    fn time(&self) -> SimTime;
    fn revision(&self) -> u64;
    fn observe(&self) -> PlayerView;
    fn submit(&mut self, submission: Submission) -> Result<CommandReceipt, Error>;
    fn next_caller_sequence(&self) -> Result<u64, Error>;
    fn advance(&mut self, duration: SimDuration) -> Result<AdvanceResult, Error>;
    fn save(&self) -> Result<Vec<u8>, Error>;
    fn restored(&self, bytes: &[u8]) -> Result<Self, Error>
    where
        Self: Sized;
    fn debug_json(&self) -> Result<String, Error>;
}
