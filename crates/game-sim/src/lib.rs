//! Logical scheduling and deterministic execution. A composition supplies
//! immutable facts, registered callbacks, and one prepared world transaction.
use game_core::{Error, SimDuration, SimTime};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::marker::PhantomData;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ModuleId(pub u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PhaseId(pub u16);
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PhaseKey {
    pub module: ModuleId,
    pub phase: PhaseId,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clock {
    pub time: SimTime,
    #[serde(skip)]
    pub revision: u64,
}
/// The composition maps these operations onto its existing authoritative time
/// and noncanonical publication counter; the engine stores neither separately.
pub trait ClockWorld {
    fn clock(&self) -> Clock;
    fn publish_clock(&mut self, clock: Clock);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interval {
    pub from: SimTime,
    pub to: SimTime,
}
impl Interval {
    pub fn elapsed(self) -> SimDuration {
        SimDuration(self.to.0 - self.from.0)
    }
}

pub type Boundary<W> = fn(&W, Interval) -> Result<SimTime, Error>;
pub type InputStop<W, S> = fn(&W, &Clock) -> Result<Option<S>, Error>;

/// A registration contains no mutable world or scheduling clock. Callbacks
/// must be pure; a boundary must fall strictly after the current committed time.
pub struct Module<W, S> {
    pub id: ModuleId,
    pub boundary: Option<Boundary<W>>,
    pub input_stop: Option<InputStop<W, S>>,
    phases: BTreeMap<PhaseId, BTreeSet<PhaseKey>>,
    duplicate_phase: bool,
}
impl<W, S> Module<W, S> {
    pub fn new(id: ModuleId) -> Self {
        Self {
            id,
            boundary: None,
            input_stop: None,
            phases: BTreeMap::new(),
            duplicate_phase: false,
        }
    }
    pub fn phase(mut self, id: PhaseId, after: impl IntoIterator<Item = PhaseKey>) -> Self {
        if self
            .phases
            .insert(id, after.into_iter().collect())
            .is_some()
        {
            self.duplicate_phase = true;
        }
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Execution {
    pub workers: usize,
    pub reverse_jobs: bool,
}
impl Default for Execution {
    fn default() -> Self {
        Self {
            workers: 1,
            reverse_jobs: false,
        }
    }
}

pub struct Executor {
    pool: rayon::ThreadPool,
    execution: Execution,
}
impl Executor {
    pub fn new(execution: Execution) -> Result<Self, Error> {
        if execution.workers == 0 {
            return Err(Error::InvalidConfiguration(
                "at least one worker is required".into(),
            ));
        }
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(execution.workers)
            .build()
            .map_err(|error| Error::InvalidConfiguration(error.to_string()))?;
        Ok(Self { pool, execution })
    }
    /// Reuse the bounded executor for an existing pure batch implementation.
    pub fn install<R: Send>(&self, execute: impl FnOnce() -> R + Send) -> R {
        self.pool.install(execute)
    }
    pub fn pool(&self) -> &rayon::ThreadPool {
        &self.pool
    }
    /// Inputs and evaluators carry no mutable world. Results, including errors,
    /// are merged by logical key rather than submission or completion order.
    pub fn map_keyed<K, I, O>(
        &self,
        input: &[(K, I)],
        evaluate: impl Fn(&I) -> Result<O, Error> + Send + Sync,
    ) -> Result<Vec<(K, O)>, Error>
    where
        K: Ord + Clone + Send + Sync,
        I: Sync,
        O: Send,
    {
        let mut keys = BTreeSet::new();
        if input.iter().any(|(key, _)| !keys.insert(key)) {
            return Err(Error::InvalidConfiguration(
                "duplicate logical job key".into(),
            ));
        }
        let job = |(key, input): &(K, I)| (key.clone(), evaluate(input));
        let results = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.pool.install(|| {
                if self.execution.workers == 1 || input.len() < 256 {
                    if self.execution.reverse_jobs {
                        input.iter().rev().map(job).collect::<Vec<_>>()
                    } else {
                        input.iter().map(job).collect::<Vec<_>>()
                    }
                } else if self.execution.reverse_jobs {
                    input.par_iter().rev().map(job).collect::<Vec<_>>()
                } else {
                    input.par_iter().map(job).collect::<Vec<_>>()
                }
            })
        }))
        .map_err(|_| Error::WorkerFailure)?;
        let mut results = results;
        results.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        results
            .into_iter()
            .map(|(key, result)| result.map(|output| (key, output)))
            .collect()
    }
    /// Executes an already sorted, unique batch without building a key index
    /// or sorting outputs. Indexed parallel collection preserves logical order.
    pub fn map_ordered<K, I, O>(
        &self,
        input: &[(K, I)],
        evaluate: impl Fn(&I) -> Result<O, Error> + Send + Sync,
    ) -> Result<Vec<(K, O)>, Error>
    where
        K: Ord + Clone + Send + Sync,
        I: Sync,
        O: Send,
    {
        if input.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
            return Err(Error::InvalidConfiguration(
                "logical job keys must be strictly increasing".into(),
            ));
        }
        let job = |(key, input): &(K, I)| (key.clone(), evaluate(input));
        let mut results = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.pool.install(|| {
                if self.execution.workers == 1 || input.len() < 256 {
                    if self.execution.reverse_jobs {
                        input.iter().rev().map(job).collect::<Vec<_>>()
                    } else {
                        input.iter().map(job).collect::<Vec<_>>()
                    }
                } else if self.execution.reverse_jobs {
                    input.par_iter().rev().map(job).collect::<Vec<_>>()
                } else {
                    input.par_iter().map(job).collect::<Vec<_>>()
                }
            })
        }))
        .map_err(|_| Error::WorkerFailure)?;
        if self.execution.reverse_jobs {
            results.reverse();
        }
        results
            .into_iter()
            .map(|(key, result)| result.map(|output| (key, output)))
            .collect()
    }
}

/// Preparation must check every fallible operation before producing this
/// transaction. Commit is infallible and runs once on the sole mutable world;
/// it may request explicit caller input after publishing its prepared effects.
pub trait Transaction<W, S> {
    /// Conservative upper bound on this commit's publication increments.
    /// The engine reserves one additional increment before any mutation.
    fn revision_budget(&self) -> u64 {
        0
    }
    fn commit(self, world: &mut W) -> Option<S>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Advance<S> {
    pub requested: SimTime,
    pub reached: SimTime,
    pub revision: u64,
    pub input: Option<S>,
}

pub struct Engine<W, S> {
    modules: BTreeMap<ModuleId, Module<W, S>>,
    phases: Vec<PhaseKey>,
    executor: Executor,
    marker: PhantomData<fn(&W) -> S>,
}
impl<W, S> Engine<W, S> {
    pub fn new(
        modules: impl IntoIterator<Item = Module<W, S>>,
        execution: Execution,
    ) -> Result<Self, Error> {
        let mut registrations = BTreeMap::new();
        let mut dependencies = BTreeMap::<PhaseKey, BTreeSet<PhaseKey>>::new();
        for module in modules {
            if module.duplicate_phase || registrations.contains_key(&module.id) {
                return Err(Error::InvalidConfiguration(
                    "duplicate module or phase".into(),
                ));
            }
            for (phase, after) in &module.phases {
                dependencies.insert(
                    PhaseKey {
                        module: module.id,
                        phase: *phase,
                    },
                    after.clone(),
                );
            }
            registrations.insert(module.id, module);
        }
        if dependencies
            .values()
            .flatten()
            .any(|key| !dependencies.contains_key(key))
        {
            return Err(Error::InvalidConfiguration(
                "unknown phase dependency".into(),
            ));
        }
        let mut ready: BTreeSet<_> = dependencies
            .iter()
            .filter(|(_, after)| after.is_empty())
            .map(|(key, _)| *key)
            .collect();
        let mut phases = Vec::with_capacity(dependencies.len());
        while let Some(key) = ready.pop_first() {
            phases.push(key);
            for (next, after) in &mut dependencies {
                if after.remove(&key) && after.is_empty() {
                    ready.insert(*next);
                }
            }
        }
        if phases.len() != dependencies.len() {
            return Err(Error::InvalidConfiguration(
                "cyclic phase dependencies".into(),
            ));
        }
        Ok(Self {
            modules: registrations,
            phases,
            executor: Executor::new(execution)?,
            marker: PhantomData,
        })
    }
    pub fn phases(&self) -> &[PhaseKey] {
        &self.phases
    }
    pub fn executor(&self) -> &Executor {
        &self.executor
    }
    /// Fold registered predictions by stable module ID, giving each callback
    /// the interval already clipped by its predecessors.
    pub fn next_boundary(&self, world: &W, requested: SimTime) -> Result<SimTime, Error>
    where
        W: ClockWorld,
    {
        let from = world.clock().time;
        if requested < from {
            return Err(Error::InvalidConfiguration(
                "boundary target precedes current time".into(),
            ));
        }
        let mut next = requested;
        if next == from {
            return Ok(next);
        }
        for module in self.modules.values() {
            if let Some(boundary) = module.boundary {
                let interval = Interval { from, to: next };
                let candidate = boundary(world, interval)?;
                if candidate <= from || candidate > interval.to {
                    return Err(Error::InvalidConfiguration(
                        "module boundary is outside its clipped interval".into(),
                    ));
                }
                next = candidate;
            }
        }
        Ok(next)
    }
    /// Drive registered intervals until the target or explicit caller input.
    pub fn advance<T: Transaction<W, S>>(
        &self,
        world: &mut W,
        duration: SimDuration,
        prepare: impl Fn(&W, Interval, &[PhaseKey], &Executor) -> Result<T, Error>,
    ) -> Result<Advance<S>, Error>
    where
        W: ClockWorld,
    {
        let requested = SimTime(
            world
                .clock()
                .time
                .0
                .checked_add(duration.0)
                .ok_or(Error::Overflow)?,
        );
        while world.clock().time < requested {
            let result = self.advance_one(
                world,
                SimDuration(requested.0 - world.clock().time.0),
                &prepare,
            )?;
            if result.input.is_some() {
                return Ok(result);
            }
        }
        let closing = world.clock();
        Ok(Advance {
            requested,
            reached: closing.time,
            revision: closing.revision,
            input: None,
        })
    }
    /// Commit at most one boundary. The composition may service its own control
    /// decisions between calls without repeating registered predictions.
    pub fn advance_one<T: Transaction<W, S>>(
        &self,
        world: &mut W,
        duration: SimDuration,
        prepare: impl Fn(&W, Interval, &[PhaseKey], &Executor) -> Result<T, Error>,
    ) -> Result<Advance<S>, Error>
    where
        W: ClockWorld,
    {
        let requested = SimTime(
            world
                .clock()
                .time
                .0
                .checked_add(duration.0)
                .ok_or(Error::Overflow)?,
        );
        if world.clock().time < requested {
            let opening = world.clock();
            for module in self.modules.values() {
                if let Some(check) = module.input_stop
                    && let Some(input) = check(world, &opening)?
                {
                    return Ok(Advance {
                        requested,
                        reached: opening.time,
                        revision: opening.revision,
                        input: Some(input),
                    });
                }
            }
            let next = self.next_boundary(world, requested)?;
            opening.revision.checked_add(1).ok_or(Error::Overflow)?;
            let interval = Interval {
                from: opening.time,
                to: next,
            };
            let transaction = prepare(world, interval, &self.phases, &self.executor)?;
            let maximum_revision = opening
                .revision
                .checked_add(transaction.revision_budget())
                .and_then(|value| value.checked_add(1))
                .ok_or(Error::Overflow)?;
            if world.clock() != opening {
                return Err(Error::InvalidConfiguration(
                    "a pure preparation changed the clock".into(),
                ));
            }
            world.publish_clock(Clock {
                time: next,
                ..opening
            });
            let input = transaction.commit(world);
            let closing = world.clock();
            assert_eq!(
                closing.time, next,
                "prepared commit changed authoritative time"
            );
            assert!(
                closing.revision >= opening.revision && closing.revision < maximum_revision,
                "prepared commit exceeded its publication budget"
            );
            let revision = closing.revision + 1;
            world.publish_clock(Clock {
                revision,
                ..closing
            });
            {
                return Ok(Advance {
                    requested,
                    reached: next,
                    revision,
                    input,
                });
            }
        }
        let closing = world.clock();
        Ok(Advance {
            requested,
            reached: closing.time,
            revision: closing.revision,
            input: None,
        })
    }
}

#[cfg(test)]
mod ordered_executor_tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    #[test]
    fn ordered_batches_match_keyed_results_and_first_logical_error() {
        let inputs: Vec<_> = (0..512_u64).map(|key| (key, key)).collect();
        for workers in [1, 2, 4] {
            for reverse_jobs in [false, true] {
                let executor = Executor::new(Execution {
                    workers,
                    reverse_jobs,
                })
                .unwrap();
                assert_eq!(
                    executor
                        .map_ordered(&inputs, |value| Ok(value * 3))
                        .unwrap(),
                    executor.map_keyed(&inputs, |value| Ok(value * 3)).unwrap()
                );
                let evaluate = |value: &u64| {
                    if *value == 8 {
                        Err(Error::Unavailable)
                    } else if *value == 400 {
                        Err(Error::Overflow)
                    } else {
                        Ok(*value)
                    }
                };
                assert_eq!(
                    executor.map_ordered(&inputs, evaluate),
                    Err(Error::Unavailable)
                );
                assert_eq!(
                    executor.map_ordered(&inputs, evaluate),
                    executor.map_keyed(&inputs, evaluate)
                );
            }
        }
    }

    #[test]
    fn large_ordered_batch_really_uses_parallel_workers() {
        let executor = Executor::new(Execution {
            workers: 2,
            reverse_jobs: false,
        })
        .unwrap();
        let inputs: Vec<_> = (0..512_u64).map(|key| (key, key)).collect();
        let barrier = Arc::new(Barrier::new(2));
        let output = executor
            .map_ordered(&inputs, |value| {
                if *value == 0 || *value == 256 {
                    barrier.wait();
                }
                Ok((value * 2, rayon::current_thread_index().unwrap()))
            })
            .unwrap();
        assert_ne!(output[0].1.1, output[256].1.1);
        assert_eq!(
            output
                .iter()
                .map(|(key, (value, _))| (*key, *value))
                .collect::<Vec<_>>(),
            inputs
                .iter()
                .map(|(key, value)| (*key, value * 2))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn unordered_or_duplicate_keys_reject_before_evaluation() {
        let executor = Executor::new(Execution::default()).unwrap();
        for inputs in [vec![(1, ()), (1, ())], vec![(2, ()), (1, ())]] {
            assert!(matches!(
                executor.map_ordered(&inputs, |_| -> Result<(), Error> {
                    panic!("invalid keys must not invoke evaluator")
                }),
                Err(Error::InvalidConfiguration(_))
            ));
        }
        assert_eq!(
            executor
                .map_ordered::<u64, (), ()>(&[], |_| Ok(()))
                .unwrap(),
            vec![]
        );
    }

    #[test]
    fn parallel_panics_become_worker_failure_in_both_submission_orders() {
        let inputs: Vec<_> = (0..512_u64).map(|key| (key, key)).collect();
        for reverse_jobs in [false, true] {
            let executor = Executor::new(Execution {
                workers: 4,
                reverse_jobs,
            })
            .unwrap();
            assert_eq!(
                executor.map_ordered(&inputs, |value| {
                    if *value == 320 {
                        panic!("worker test panic");
                    }
                    Ok(*value)
                }),
                Err(Error::WorkerFailure)
            );
        }
    }
}
