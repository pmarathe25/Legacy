use game_core::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Burden {
    pub burden: u64,
    pub exposure: u64,
    pub uninterrupted: u64,
    pub burden_remainder: u16,
    pub exposure_remainder: u16,
    pub temporary_identity: u16,
    pub emotional_disruption: u16,
    pub permanent_identity: u16,
    pub lifespan_loss: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Episode {
    pub actor: PersonId,
    pub starts: SimTime,
    pub ends: SimTime,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Consent {
    pub temporary: u8,
    pub permanent: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum InvocationCost {
    Strain,
    Emotional,
    Identity,
    LossOfControl,
    Lifespan,
}
