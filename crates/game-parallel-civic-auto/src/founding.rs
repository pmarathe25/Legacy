//! Data-defined thresholds for voluntary institution formation.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoundingRules {
    pub minimum_founders: u32,
    pub quorum: u32,
}
impl FoundingRules {
    pub(crate) fn valid(&self) -> bool {
        self.minimum_founders >= 2 && self.quorum > 0 && self.quorum <= self.minimum_founders
    }
}
