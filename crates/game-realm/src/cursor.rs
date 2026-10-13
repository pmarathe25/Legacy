//! Event-cursor metadata is admitted by its owner before native publication.
use crate::*;
use game_core::Error;
#[derive(Clone, Debug)]
pub struct PreparedEventCursor {
    before: Runtime,
    through: u64,
}
impl Runtime {
    pub fn prepare_event_cursor(&self, through: u64) -> Result<PreparedEventCursor, Error> {
        if through < self.last_event {
            return Err(Error::Unavailable);
        }
        Ok(PreparedEventCursor {
            before: self.clone(),
            through,
        })
    }
}
impl PreparedEventCursor {
    pub fn validate(&self, runtime: &Runtime) -> Result<(), Error> {
        if runtime != &self.before {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    /// Finish only after validation and successful native publication in the
    /// same serialized transition. No additional effects are emitted here.
    pub fn finish(self, runtime: &mut Runtime) {
        runtime.last_event = self.through;
    }
}
