//! Serializable values for personal skill learning.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub recipe: u32,
    pub completions_per_level: u32,
    pub max_level: u16,
    pub work_per_level: u32,
}
impl Rule {
    pub fn threshold(&self, level: u16) -> u64 {
        let level = u64::from(level);
        u64::from(self.completions_per_level) * level * (level + 1) / 2
    }
    pub fn level(&self, completions: u64) -> u16 {
        let (mut low, mut high) = (0u32, u32::from(self.max_level) + 1);
        while low + 1 < high {
            let middle = (low + high) / 2;
            if self.threshold(middle as u16) <= completions {
                low = middle;
            } else {
                high = middle;
            }
        }
        low as u16
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub person: u64,
    pub recipe: u32,
    pub completions: u64,
    pub credited_jobs: Vec<Attribution>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attribution {
    pub finished_at: u64,
    pub job: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct Completion {
    pub job: u64,
    pub worker: u64,
    pub recipe: u32,
    pub finished_at: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub person: u64,
    pub recipe: u32,
    pub previous_level: u16,
    pub level: u16,
}

/// A related recipe reads and contributes to the canonical skill's personal record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Alias {
    pub recipe: u32,
    pub canonical: u32,
}

/// Authored learning rates used when building rules for a content catalog.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogTraining {
    pub completions_per_level: u32,
    pub max_level: u16,
    pub work_per_level: u32,
}
