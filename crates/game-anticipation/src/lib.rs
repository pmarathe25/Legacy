//! Bounded, advisory preparation ahead of canonical scheduled dispatch.
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
pub struct Prepare {
    pub people: Vec<PersonId>,
}
impl Request for Prepare {
    const KEY: &'static str = "anticipation.prepare";
    type Response = ();
}
/// Subscribers may warm validated owner caches; this is never a decision or observation.
#[derive(Serialize, Deserialize)]
pub struct Preview {
    pub people: Vec<PersonId>,
}
impl Event for Preview {
    const KEY: &'static str = "anticipation.preview";
}
/// Register under the caller's owner, without adding a resource or save plugin.
pub fn register(b: &mut Builder) -> Result<()> {
    b.request::<Prepare>(|tx, r| tx.notify(Preview { people: r.people }))
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct Statistics {
    pub batches: u64,
    pub actors: u64,
    pub failures: u64,
    pub maximum: usize,
}
/// An external driver's lookahead cursor. It never consumes or reorders jobs.
pub struct Window {
    width: usize,
    through: Option<(SimTime, u8, ScheduleId)>,
    pub statistics: Statistics,
}
impl Default for Window {
    fn default() -> Self {
        Self::new(0).expect("disabled preparation")
    }
}
impl Window {
    /// Zero disables preparation. Width counts all queued jobs, including other owners.
    pub fn new(width: usize) -> Result<Self> {
        require(width <= 128, "preparation window must be 0..=128")?;
        Ok(Self {
            width,
            through: None,
            statistics: Statistics::default(),
        })
    }
    /// Advance through canonical commits, preparing before each boundary when enabled.
    pub fn advance<R: Request>(
        &mut self,
        host: &mut Host,
        target: SimTime,
        budget: usize,
        person: impl Fn(&R) -> PersonId,
    ) -> Result<Progress> {
        require(target >= host.now(), "cannot run backwards")?;
        require(budget > 0, "work budget must be positive")?;
        if self.width == 0 {
            return host.advance(target, budget);
        }
        for remaining in (0..budget).rev() {
            self.prepare::<R>(host, target, &person);
            let progress = host.advance(target, 1)?;
            if remaining == 0 || progress.stop != Stop::Budget {
                return Ok(progress);
            }
        }
        unreachable!("positive work budget")
    }
    pub fn prepare<R: Request>(
        &mut self,
        host: &Host,
        target: SimTime,
        person: impl Fn(&R) -> PersonId,
    ) {
        if self.width == 0 {
            return;
        }
        let mut queued = host.scheduled();
        let Some(first) = queued.next().filter(|j| j.at <= target) else {
            return;
        };
        let key = (first.at, first.priority, first.id);
        if self
            .through
            .is_some_and(|end| (end.0, end.1) == (key.0, key.1) && key.2 <= end.2)
        {
            return;
        }
        let mut people = Vec::new();
        for job in std::iter::once(first)
            .chain(queued)
            .take(self.width)
            .take_while(|j| (j.at, j.priority) == (key.0, key.1))
        {
            self.through = Some((job.at, job.priority, job.id));
            if let Some(request) = job.request.request::<R>() {
                people.push(person(request));
            }
        }
        // Duplicate wakeups do not need duplicate query preparation.
        people.sort_unstable();
        people.dedup();
        if people.is_empty() {
            return;
        }
        self.statistics.batches += 1;
        self.statistics.actors += people.len() as u64;
        self.statistics.maximum = self.statistics.maximum.max(people.len());
        // Failed or panicking advisory work must not suppress canonical execution.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            host.query_at(key.0, Prepare { people })
        }));
        if !matches!(result, Ok(Ok(()))) {
            self.statistics.failures += 1;
        }
    }
}
