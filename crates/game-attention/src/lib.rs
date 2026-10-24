//! Saved attention deadlines; features retain perception and action execution.
use game_core::*;
use game_sim::{Request, Transaction};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rates {
    pub ordinary: SimDuration,
    pub evidence: SimDuration,
}
impl Rates {
    pub fn validate(self) -> Result<()> {
        require(
            self.evidence.0 > 0 && self.evidence <= self.ordinary && self.ordinary <= DAY,
            "invalid attention rates",
        )
    }
}

/// Wake-ups coalesce; action completion remains with its feature owner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewClock {
    pub reviewed: Option<SimTime>,
    pending: Option<(SimTime, ScheduleId)>,
}
impl ReviewClock {
    pub fn pending(&self) -> Option<SimTime> {
        self.pending.map(|(at, _)| at)
    }
    pub fn frame(&mut self, now: SimTime, start: SimTime) -> bool {
        if self.reviewed.is_some_and(|at| at >= start) {
            return false;
        }
        self.reviewed = Some(now);
        true
    }

    pub fn in_progress(&self, now: SimTime) -> bool {
        self.reviewed == Some(now) && self.pending.is_none()
    }
    pub fn due(&self, now: SimTime, floor: SimDuration) -> bool {
        self.pending.is_some_and(|(at, _)| at <= now)
            || self
                .reviewed
                .is_none_or(|at| now.0.saturating_sub(at.0) >= floor.0)
    }
    pub fn consume(&mut self, tx: &mut Transaction<'_>) {
        if let Some((_, id)) = self.pending.take() {
            tx.unschedule(id);
        }
    }
    pub fn schedule<R: Request>(
        &mut self,
        tx: &mut Transaction<'_>,
        floor: SimDuration,
        immediate: bool,
        request: R,
    ) -> Result<()> {
        let at = if immediate {
            tx.now()
        } else {
            self.reviewed
                .map(|at| at.after(floor))
                .transpose()?
                .unwrap_or(tx.now())
                .max(tx.now())
        };
        if self.pending.is_some_and(|(old, _)| old <= at) {
            return Ok(());
        }
        self.consume(tx);
        self.pending = Some((at, tx.schedule(at, 50, request)?));
        Ok(())
    }
}
