//! Boundary-aligned advancement. Sub-boundary progress is not yet modeled.
use crate::Economy;
use game_economy_campaign::InvocationWarning;
use game_lot_runtime::Events;
use std::ops::ControlFlow;

/// A committed boundary and its exact elapsed time under the configured quantum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundaryTime {
    pub tick: u64,
    pub elapsed_millis: u128,
    pub quantum_millis: u64,
}

/// Why a bounded sequence of whole boundaries stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BoundaryStop {
    Completed,
    Yielded,
    Blocked {
        message: String,
        warning: Option<InvocationWarning>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundaryAdvance {
    pub start: BoundaryTime,
    pub end: BoundaryTime,
    pub requested: u64,
    pub completed: u64,
    pub stop: BoundaryStop,
}

impl Economy {
    /// Derived from the persisted tick; no extra clock state needs reconciliation.
    pub fn boundary_time(&self) -> BoundaryTime {
        let tick = self.tick();
        let quantum_millis = self.physical.tick_millis();
        BoundaryTime {
            tick,
            elapsed_millis: u128::from(tick) * u128::from(quantum_millis),
            quantum_millis,
        }
    }

    /// Advance whole boundaries, streaming each committed event set once.
    /// A caller can yield after any commit. Failures retain earlier commits and
    /// report their count; a pending invocation warning is included when present.
    pub fn advance_boundaries(
        &mut self,
        requested: u64,
        mut observe: impl FnMut(&Events) -> ControlFlow<()>,
    ) -> BoundaryAdvance {
        let start = self.boundary_time();
        let mut completed = 0;
        let mut stop = BoundaryStop::Completed;
        while completed < requested {
            match self.advance() {
                Ok(events) => {
                    completed += 1;
                    if observe(&events).is_break() && completed < requested {
                        stop = BoundaryStop::Yielded;
                        break;
                    }
                }
                Err(message) => {
                    let warning = self
                        .campaign
                        .as_ref()
                        .and_then(|campaign| campaign.lineage.invocation.as_ref())
                        .and_then(|invocation| invocation.pending.clone());
                    stop = BoundaryStop::Blocked { message, warning };
                    break;
                }
            }
        }
        BoundaryAdvance {
            start,
            end: self.boundary_time(),
            requested,
            completed,
            stop,
        }
    }
}
