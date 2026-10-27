//! Persisted per-actor full-review deadline and transactional invalidation.
use game_core::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameTier {
    pub full_due: Option<SimTime>,
    pub generation: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_at: Option<SimTime>,
    #[serde(default, skip_serializing_if = "zero")]
    pub retries: u8,
}
fn zero(value: &u8) -> bool {
    *value == 0
}
impl FrameTier {
    pub fn fast_at(self, now: SimTime) -> bool {
        self.full_due.is_some_and(|due| now < due)
    }
    pub fn invalidate(&mut self) -> Result<()> {
        self.generation = self.generation.checked_add(1).ok_or(Error::Overflow)?;
        Ok(())
    }
    /// Allow two urgent jobs per actor at one time; later evidence waits for a frame.
    pub fn immediate_retry(&mut self, now: SimTime) -> bool {
        if self.retry_at != Some(now) {
            self.retry_at = Some(now);
            self.retries = 0;
        }
        if self.retries >= 2 {
            return false;
        }
        self.retries += 1;
        true
    }
    /// Complete only the review whose captured generation remains current.
    pub fn complete_full(
        &mut self,
        now: SimTime,
        interval: SimDuration,
        generation: u64,
    ) -> Result<bool> {
        require(interval.0 > 0, "empty full review interval")?;
        if self.generation != generation {
            return Ok(false);
        }
        self.full_due = Some(now.after(interval)?);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deadline_and_generation_prevent_stale_full_completion() {
        let mut tier = FrameTier::default();
        assert!(!tier.fast_at(SimTime(0)));
        assert!(tier.complete_full(SimTime(0), MINUTE, 0).unwrap());
        assert!(tier.fast_at(SimTime(MINUTE.0 - 1)));
        assert!(!tier.fast_at(SimTime(MINUTE.0)));
        let before = tier;
        tier.invalidate().unwrap();
        assert!(
            !tier
                .complete_full(SimTime(10), MINUTE, before.generation)
                .unwrap()
        );
        assert_eq!(tier.full_due, before.full_due);
        assert!(
            tier.complete_full(SimTime(10), MINUTE, tier.generation)
                .unwrap()
        );
        assert_eq!(tier.full_due, Some(SimTime(MINUTE.0 + 10)));
    }
    #[test]
    fn retry_budget_resets_only_after_time_advances() {
        let mut tier = FrameTier::default();
        assert!(tier.immediate_retry(SimTime(5)));
        assert!(tier.immediate_retry(SimTime(5)));
        assert!(!tier.immediate_retry(SimTime(5)));
        assert!(tier.immediate_retry(SimTime(6)));
    }
}
