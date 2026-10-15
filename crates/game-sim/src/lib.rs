//! One transactional host. Plugins own all gameplay state and requests.
use game_storage::Records;
mod persistence;
mod registry;
mod transaction;
use game_core::*;
pub use registry::{Builder, Event, Request, Resource};
use registry::{Registry, Stored};
use serde::{Deserialize, Serialize};
use serde_json::Value;
// Keys come only from compiled Rust types; external keys retain ordered maps.
use rustc_hash::FxHashMap as HashMap;
use std::{
    any::TypeId,
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};
pub use transaction::Transaction;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    pub key: String,
    pub payload: Value,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub id: ScheduleId,
    pub at: SimTime,
    pub priority: u8,
    pub request: Envelope,
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
pub struct Action {
    pub owner: Arc<str>,
    pub kind: String,
    pub until: Option<SimTime>,
    pub cancel: Envelope,
}
/// An internal lifecycle signal; it does not append a player notice.
#[derive(Serialize, Deserialize)]
pub struct ActionFinished {
    pub person: PersonId,
}
impl Event for ActionFinished {
    const KEY: &'static str = "sim.action_finished";
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actions(pub Records<PersonId, Action>);
impl Resource for Actions {
    const KEY: &'static str = "world.actions";
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
#[derive(Clone, Serialize, Deserialize)]
struct Receipt {
    request: Envelope,
    response: Value,
}
pub struct Host {
    registry: Arc<Registry>,
    data: HashMap<TypeId, Stored>,
    pool: rayon::ThreadPool,
    pub(crate) time: SimTime,
    pub(crate) seed: u64,
    revision: u64,
    next_id: u64,
    jobs: BTreeMap<(SimTime, u8, ScheduleId), Job>,
    job_keys: BTreeMap<ScheduleId, (SimTime, u8, ScheduleId)>,
    notices: VecDeque<Notice>,
    next_notice: u64,
    receipts: BTreeMap<u64, Receipt>,
}
impl Host {
    pub fn registered<R: Request>(&self) -> bool {
        self.registry.handlers.contains_key(R::KEY)
    }
    /// Queries use owner handlers but may not write, schedule, emit, or allocate.
    pub fn query<R: Request>(&self, request: R) -> Result<R::Response> {
        let mut tx = Transaction::new(self, self.time);
        let result = tx.call(request)?;
        require(tx.read_only(), "query attempted a state change")?;
        Ok(result)
    }
    pub fn now(&self) -> SimTime {
        self.time
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn next_sequence(&self) -> u64 {
        self.receipts
            .last_key_value()
            .map_or(1, |(key, _)| key.saturating_add(1))
    }
    pub fn workers(&self) -> usize {
        self.pool.current_num_threads()
    }
    pub fn get<T: Resource>(&self) -> Result<&T> {
        self.data
            .get(&TypeId::of::<T>())
            .and_then(|s| s.downcast_ref())
            .ok_or_else(|| Error::Missing(T::KEY.into()))
    }
    pub fn notices(&self) -> impl Iterator<Item = &Notice> {
        self.notices.iter()
    }
    /// Internal requests and due work share exactly the same transaction path.
    pub fn dispatch<R: Request>(&mut self, request: R) -> Result<R::Response> {
        let envelope = Envelope {
            key: R::KEY.into(),
            payload: serde_json::to_value(request).map_err(registry::json_error)?,
        };
        let result = self.run(envelope, None)?;
        serde_json::from_value(result).map_err(registry::json_error)
    }
    /// Client sequence numbers are durable: retrying identical input returns its first result.
    pub fn submit<R: Request>(&mut self, sequence: u64, request: R) -> Result<R::Response> {
        let envelope = Envelope {
            key: R::KEY.into(),
            payload: serde_json::to_value(request).map_err(registry::json_error)?,
        };
        if let Some(receipt) = self.receipts.get(&sequence) {
            require(
                receipt.request == envelope,
                "sequence reused with different input",
            )
            .map_err(|_| Error::SequenceConflict)?;
            return serde_json::from_value(receipt.response.clone()).map_err(registry::json_error);
        }
        for guard in &self.registry.guards {
            guard(self, &envelope)?;
        }
        let response = self.run(envelope.clone(), None)?;
        self.receipts.insert(
            sequence,
            Receipt {
                request: envelope,
                response: response.clone(),
            },
        );
        serde_json::from_value(response).map_err(registry::json_error)
    }
    fn run(
        &mut self,
        envelope: Envelope,
        remove: Option<(SimTime, u8, ScheduleId)>,
    ) -> Result<Value> {
        let at = remove.map_or(self.time, |key| key.0);
        let limit = self
            .registry
            .batches
            .get(&envelope.key)
            .copied()
            .unwrap_or(1);
        let registered_key = self
            .registry
            .handlers
            .get_key_value(&envelope.key)
            .map(|(key, _)| key.as_str());
        let jobs = &self.jobs;
        let extra = remove.into_iter().flat_map(|first| {
            jobs.range((std::ops::Bound::Excluded(first), std::ops::Bound::Unbounded))
                .take(limit - 1)
                .take_while(move |(key, job)| {
                    (key.0, key.1) == (first.0, first.1)
                        && Some(job.request.key.as_str()) == registered_key
                })
                .map(|(key, job)| (Some(*key), job.request.clone()))
        });
        let requests = std::iter::once((remove, envelope)).chain(extra);
        let mut removed = 0;
        let mut tx = Transaction::new(self, at);
        let response = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut response = Value::Null;
            for (key, request) in requests {
                if let Some(key) = key {
                    // A newly scheduled earlier phase must run before the next
                    // captured request. Cancellation can remove a captured job.
                    if tx.precedes(key) {
                        break;
                    }
                    removed += 1;
                    if tx.cancelled(key.2) {
                        continue;
                    }
                }
                response = tx.call_envelope(request)?;
            }
            tx.validate()?;
            Ok(response)
        }))
        .map_err(|_| Error::WorkerFailure)??;
        let changes = tx.finish();
        self.data.extend(
            changes
                .writes
                .iter()
                .map(|(id, value)| (*id, value.clone())),
        );
        for _ in 0..removed {
            let (key, _) = self.jobs.pop_first().expect("committed due prefix");
            self.job_keys.remove(&key.2);
        }
        for id in changes.cancelled {
            if let Some(key) = self.job_keys.remove(&id) {
                self.jobs.remove(&key);
            }
        }
        for job in changes.jobs {
            let key = (job.at, job.priority, job.id);
            self.job_keys.insert(job.id, key);
            self.jobs.insert(key, job);
        }
        for notice in changes.notices {
            self.notices.push_back(notice);
        }
        while self.notices.len() > 1024 {
            self.notices.pop_front();
        }
        self.next_id = changes.next_id;
        self.next_notice = changes.next_notice;
        self.time = at;
        self.revision = self.revision.checked_add(1).ok_or(Error::Overflow)?;
        Ok(response)
    }
    /// Bounded work lets a session service pause and input between committed boundaries.
    pub fn advance(&mut self, target: SimTime, budget: usize) -> Result<Progress> {
        require(target >= self.time, "cannot run backwards")?;
        require(budget > 0, "work budget must be positive")?;
        for _ in 0..budget {
            for stop in &self.registry.stops {
                if let Some(reason) = stop(self)? {
                    return Ok(self.progress(target, Stop::Choice(reason)));
                }
            }
            let next = self
                .jobs
                .first_key_value()
                .filter(|(key, _)| key.0 <= target)
                .map(|(key, job)| (*key, job.request.clone()));
            match next {
                Some((key, request)) => {
                    self.run(request, Some(key))?;
                }
                None => {
                    self.time = target;
                    return Ok(self.progress(target, Stop::Target));
                }
            }
        }
        Ok(self.progress(target, Stop::Budget))
    }
    fn progress(&self, target: SimTime, stop: Stop) -> Progress {
        Progress {
            requested: target,
            reached: self.time,
            revision: self.revision,
            stop,
        }
    }
}
