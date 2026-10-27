//! Saved decision-frame experiment settings.
use serde::{Deserialize, Serialize};
mod tier;
pub use tier::FrameTier;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameTuning {
    pub recover_idle: bool,
    pub recheck_conflicts: bool,
    pub batch_food: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub batch_decisions: bool,
    #[serde(default, skip_serializing_if = "zero_cohort")]
    pub minimum_cohort: usize,
    #[serde(default, skip_serializing_if = "zero_cohort")]
    pub commit_batch: usize,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub urgent_immediate: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub batch_observations: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub meal_arbitration: bool,
    /// Experimental phased self-meal execution for admitted cohort claims.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub meal_wave: bool,
    /// Local care and self-meals share participant arbitration and owner batches.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub local_interactions: bool,
    /// Owner-defined persistent activities precede broad deliberation.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub activity_epochs: bool,
    /// Persistent mechanics sessions with batched physical trials.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mechanics_sessions: bool,
    /// Retain nonurgent offers for at most this many milliseconds; zero disables.
    #[serde(default, skip_serializing_if = "zero_millis")]
    pub nonurgent_millis: u64,
    /// Experimental maximum interval between fresh full reviews; zero disables.
    #[serde(default, skip_serializing_if = "zero_millis")]
    pub full_review_millis: u64,
}
fn zero_cohort(value: &usize) -> bool {
    *value == 0
}
fn zero_millis(value: &u64) -> bool {
    *value == 0
}
impl FrameTuning {
    pub fn ordinary(&self) -> bool {
        *self == Self::default()
    }
    pub fn valid_experiments(&self) -> bool {
        (!self.meal_wave || self.meal_arbitration)
            && (!(self.activity_epochs || self.mechanics_sessions)
                || (self.batch_decisions && self.urgent_immediate && self.commit_batch > 0))
            && (!self.local_interactions
                || (self.meal_arbitration && self.batch_decisions && !self.meal_wave))
            && self.nonurgent_millis <= game_core::DAY.0
            && (self.nonurgent_millis == 0 || self.batch_decisions)
            && (self.full_review_millis == 0
                || (self.full_review_millis <= game_core::DAY.0
                    && self.batch_decisions
                    && self.commit_batch > 0
                    && self.urgent_immediate
                    && !self.meal_arbitration
                    && self.nonurgent_millis == 0))
    }
}
