//! Typed dispatch contracts and serializable runtime messages. No execution policy.
use game_core::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
/// Requests clone independent immutable values; shared mutable aliases are unsupported.
pub trait Request: Clone + Serialize + DeserializeOwned + Send + Sync + 'static {
    const KEY: &'static str;
    type Response: Serialize + DeserializeOwned;
}
pub trait Event: Serialize + DeserializeOwned + 'static {
    const KEY: &'static str;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    pub key: String,
    pub payload: Value,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    pub sequence: u64,
    pub at: SimTime,
    pub kind: String,
    pub data: Value,
    pub witnesses: Vec<PersonId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stop {
    Target,
    Budget,
    Choice(String),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub requested: SimTime,
    pub reached: SimTime,
    pub revision: u64,
    pub stop: Stop,
}

impl Progress {
    #[inline]
    pub fn new(requested: SimTime, reached: SimTime, revision: u64, stop: Stop) -> Self {
        Self {
            requested,
            reached,
            revision,
            stop,
        }
    }
}
