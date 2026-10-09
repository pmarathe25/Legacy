//! Personal practical mastery, independent of titles, possessions, or past lives.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Skill {
    Medicine,
    Craft,
    Farming,
    Combat,
    Invocation,
    Negotiation,
    Leadership,
}
impl Skill {
    pub const ALL: [Self; 7] = [
        Self::Medicine,
        Self::Craft,
        Self::Farming,
        Self::Combat,
        Self::Invocation,
        Self::Negotiation,
        Self::Leadership,
    ];
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillView {
    pub skill: Skill,
    pub level: u16,
    pub practice_ms: u64,
    pub progress_ms: u64,
}

/// Frozen at the start of a physical job. The denominator and remainder preserve
/// identical progress when the host divides an interval into different slices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillRate {
    pub numerator: u64,
    pub denominator: u64,
}
impl Default for SkillRate {
    fn default() -> Self {
        Self {
            numerator: 1_000,
            denominator: 1_000,
        }
    }
}
