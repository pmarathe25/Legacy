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

/// Append one committed publication batch, retaining the complete batch or a caller's floor.
pub fn append_notices(
    queue: &mut std::collections::VecDeque<Notice>,
    notices: Vec<Notice>,
    minimum: usize,
) {
    let retained = minimum.max(notices.len());
    queue.extend(notices);
    while queue.len() > retained {
        queue.pop_front();
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job<P> {
    pub id: ScheduleId,
    pub at: SimTime,
    pub priority: u8,
    pub request: P,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action<P> {
    /// Each start receives a distinct identity, including same-time restarts.
    #[serde(default)]
    pub id: u64,
    pub owner: std::sync::Arc<str>,
    pub kind: String,
    pub until: Option<SimTime>,
    pub cancel: P,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub request: Envelope,
    pub response: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actions<P: Clone>(pub game_packed::Packed<PersonId, Action<P>>);
impl<P: Clone> Default for Actions<P> {
    fn default() -> Self {
        Self(game_packed::Packed::default())
    }
}
impl<P: Clone + Serialize + serde::de::DeserializeOwned + Send + Sync + 'static> crate::Resource
    for Actions<P>
{
    const KEY: &'static str = "world.actions";
    fn allocated_ids(&self) -> Vec<u64> {
        self.0
            .values()
            .filter_map(|a| (a.id > 0).then_some(a.id))
            .collect()
    }
    fn checkpoint(&mut self) -> crate::Checkpoint {
        Box::new(self.0.begin())
    }
    fn restore(&mut self, c: crate::Checkpoint) {
        self.0.rollback(*c.downcast().expect("action checkpoint"));
    }
    fn commit(&mut self, c: crate::Checkpoint) {
        self.0.commit(*c.downcast().expect("action checkpoint"));
    }
    fn validate_checkpoint(&self, _: &dyn std::any::Any) -> Result<()> {
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct ActionFinished {
    pub person: PersonId,
    #[serde(default)]
    pub action: u64,
    #[serde(default)]
    pub completed: bool,
}
impl ActionFinished {
    pub fn new(person: PersonId, action: u64, completed: bool) -> Self {
        Self {
            person,
            action,
            completed,
        }
    }
}
impl Event for ActionFinished {
    const KEY: &'static str = "sim.action_finished";
}

/// A disposable private draft; failed state can identify the rejected callback.
pub struct Draft<T> {
    pub state: T,
    pub accepted: bool,
}

impl Receipt {
    pub fn next_sequence(records: &std::collections::BTreeMap<u64, Self>) -> u64 {
        records
            .last_key_value()
            .map_or(1, |(key, _)| key.saturating_add(1))
    }
}
