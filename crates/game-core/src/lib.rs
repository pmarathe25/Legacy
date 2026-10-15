//! Shared units and identities. Feature state belongs to feature crates.
use serde::{Deserialize, Serialize};
macro_rules! ids { ($($name:ident),* $(,)?) => {$ (
 #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
 pub struct $name(pub u64);
)*}; }
ids!(
    PersonId,
    AreaId,
    InventoryId,
    GoodId,
    PlotId,
    ProjectId,
    AgreementId,
    InstitutionId,
    CaseId,
    MissionId,
    ArmyId,
    LifeId,
    SimTime,
    SimDuration,
    ScheduleId
);
pub const MINUTE: SimDuration = SimDuration(60_000);
pub const HOUR: SimDuration = SimDuration(3_600_000);
pub const DAY: SimDuration = SimDuration(86_400_000);
impl SimTime {
    pub fn after(self, d: SimDuration) -> Result<Self> {
        self.0.checked_add(d.0).map(Self).ok_or(Error::Overflow)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Error {
    Rejected(String),
    Missing(String),
    Duplicate(String),
    InvalidConfiguration(String),
    InvalidSave(String),
    SequenceConflict,
    Overflow,
    Limit,
    WorkerFailure,
}
pub type Result<T> = std::result::Result<T, Error>;
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
pub fn require(condition: bool, reason: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(Error::Rejected(reason.into()))
    }
}
/// Counter-based randomness: results depend on identities, never evaluation order.
pub fn random(seed: u64, identity: u64, counter: u64) -> u64 {
    let mut x =
        seed ^ identity.wrapping_mul(0x9e3779b97f4a7c15) ^ counter.wrapping_mul(0xd1b54a32d192ed03);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}
