mod pending;
mod persistence;
mod scheduled;
pub use pending::Pending;
mod registry;
mod transaction;
pub use game_contracts::{ActionFinished, Envelope, Event, Notice, Progress, Request, Stop};
use game_core::*;
pub use registry::{Builder, Checkpoint, Resource};
use registry::{Registry, Stored};
use rustc_hash::FxHashMap as HashMap;
use serde_json::Value;
use std::{
    any::TypeId,
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};
pub use transaction::{AttemptOutcome, Transaction};
pub type Job = game_contracts::Job<Pending>;
pub type Action = game_contracts::Action<Pending>;
pub type Actions = game_contracts::Actions<Pending>;
use game_contracts::Receipt;
pub struct Host {
    registry: Arc<Registry>,
    data: HashMap<TypeId, Stored>,
    pool: Arc<rayon::ThreadPool>,
    parallel_listeners: bool,
    drafts: game_contracts::drafting::Counters,
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
        self.registry.typed.contains_key(&TypeId::of::<R>())
    }
    /// Queries use owner handlers but may not write, schedule, emit, or allocate.
    pub fn query<R: Request>(&self, request: R) -> Result<R::Response> {
        self.query_at(self.time, request)
    }
    /// Inspect the current world at a future logical time without advancing it.
    pub fn query_at<R: Request>(&self, at: SimTime, request: R) -> Result<R::Response> {
        require(at >= self.time, "query time precedes world")?;
        let mut tx = Transaction::new(self, at);
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
        Receipt::next_sequence(&self.receipts)
    }
    pub fn executor(&self) -> Arc<rayon::ThreadPool> {
        self.pool.clone()
    }
    /// Runtime-only execution policy; event order and commits are unchanged.
    pub fn set_parallel_listeners(&mut self, enabled: bool) {
        self.parallel_listeners = enabled;
    }
    pub fn workers(&self) -> usize {
        self.pool.current_num_threads()
    }
    pub fn draft_statistics(&self) -> game_contracts::drafting::Statistics {
        self.drafts.snapshot()
    }
    pub fn get<T: Resource>(&self) -> Result<&T> {
        registry::read(&self.data)
    }
    /// Scheduled work in deterministic dispatch order; inspection does not advance the clock.
    pub fn scheduled(&self) -> impl Iterator<Item = &Job> {
        self.jobs.values()
    }
    pub fn notices(&self) -> impl Iterator<Item = &Notice> {
        self.notices.iter()
    }
    pub fn notices_since(&self, sequence: u64) -> impl Iterator<Item = &Notice> {
        let first = self.notices.partition_point(|n| n.sequence <= sequence);
        self.notices.range(first..)
    }
    pub fn dispatch<R: Request>(&mut self, request: R) -> Result<R::Response> {
        let result = self.run(Pending::new(request)?, None)?;
        serde_json::from_value(result).map_err(registry::json_error)
    }
    /// Client sequence numbers are durable: retrying identical input returns its first result.
    pub fn submit<R: Request>(&mut self, sequence: u64, request: R) -> Result<R::Response> {
        let envelope = Envelope {
            key: R::KEY.into(),
            payload: serde_json::to_value(request).map_err(registry::json_error)?,
        };
        if let Some(response) =
            game_contracts::previous_receipt(&self.receipts, sequence, &envelope)?
        {
            return Ok(response);
        }
        for guard in &self.registry.guards {
            guard(self, &envelope)?;
        }
        let response = self.run(self.registry.bind(envelope.clone())?, None)?;
        game_contracts::record_receipt(&mut self.receipts, sequence, envelope, response.clone());
        serde_json::from_value(response).map_err(registry::json_error)
    }
    fn run(
        &mut self,
        request: Pending,
        remove: Option<(SimTime, u8, ScheduleId)>,
    ) -> Result<Value> {
        let data = std::mem::take(&mut self.data);
        let at = remove.map_or(self.time, |key| key.0);
        let limit = self
            .registry
            .batches
            .get(request.key())
            .copied()
            .unwrap_or(1);
        let wave = remove
            .and_then(|_| self.registry.waves.get(request.key()))
            .filter(|_| !self.registry.before.contains_key(request.key()));
        let requests =
            game_contracts::scheduled_prefix(&self.jobs, remove, request, limit, |a, b| {
                a.key() == b.key()
            });
        let mut removed = 0;
        let mut tx = Transaction::owned(self, at, data);
        let response = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut response = Value::Null;
            if let Some(wave) = wave {
                removed = requests.len();
                wave(
                    &mut tx,
                    &requests.iter().map(|(_, r)| r.clone()).collect::<Vec<_>>(),
                )?;
            } else {
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
                    response = request.invoke(&mut tx)?;
                }
            }
            tx.validate()?;
            Ok(response)
        }))
        .map_err(|_| Error::WorkerFailure)
        .and_then(|r| r);
        let changes = tx.finish(response.is_ok());
        self.data = changes.data;
        let response = response?;
        game_contracts::commit_jobs(
            &mut self.jobs,
            &mut self.job_keys,
            removed,
            changes.cancelled,
            changes.jobs,
        );
        game_contracts::append_notices(&mut self.notices, changes.notices, 1024);
        self.next_id = changes.next_id;
        self.next_notice = changes.next_notice;
        self.time = at;
        self.revision = self.revision.checked_add(1).ok_or(Error::Overflow)?;
        Ok(response)
    }
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
        Progress::new(target, self.time, self.revision, stop)
    }
}
