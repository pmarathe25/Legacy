//! Serializable physical production state.
use game_lot_model::State;
use game_production_model::{Good, Job, Recipe};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Working,
    /// The last lead has departed; custody and site capacity remain reserved
    /// until the given boundary, when normal return begins if nobody takes over.
    Suspended {
        until: u64,
    },
    Waiting,
    Returning,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Work {
    pub job: Job,
    pub phase: Phase,
    /// Fractional work already accumulated, in units of the time quantum.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub progress_remainder: u64,
}
fn is_zero(value: &u64) -> bool {
    *value == 0
}

/// Elapsed time consumed before the work finished or the interval ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkProgress {
    pub consumed_millis: u64,
    pub completed: bool,
}

impl Work {
    /// Milliseconds until completion at this rate, rounded up to a whole millisecond.
    /// No deadline exists at a zero rate. This does not publish output or change custody.
    pub fn completion_after(
        &self,
        rate: u64,
        quantum_millis: u64,
    ) -> Result<Option<u128>, &'static str> {
        if quantum_millis == 0
            || self.progress_remainder >= quantum_millis
            || (self.job.remaining == 0 && self.progress_remainder != 0)
        {
            return Err("invalid fractional work progress");
        }
        if self.phase != Phase::Working {
            return Err("work is not active");
        }
        if self.job.remaining == 0 {
            return Ok(Some(0));
        }
        if rate == 0 {
            return Ok(None);
        }
        let needed = u128::from(self.job.remaining) * u128::from(quantum_millis)
            - u128::from(self.progress_remainder);
        Ok(Some(needed.div_ceil(u128::from(rate))))
    }

    /// Advance only this work counter. The caller owns action reservations,
    /// completion receipts, and the effects of publishing any output.
    pub fn advance_elapsed(
        &mut self,
        rate: u64,
        elapsed_millis: u64,
        quantum_millis: u64,
    ) -> Result<WorkProgress, &'static str> {
        let deadline = self.completion_after(rate, quantum_millis)?;
        let consumed = deadline.map_or(u128::from(elapsed_millis), |deadline| {
            deadline.min(u128::from(elapsed_millis))
        }) as u64;
        let credit = u128::from(rate) * u128::from(consumed) + u128::from(self.progress_remainder);
        let whole = credit / u128::from(quantum_millis);
        if whole >= u128::from(self.job.remaining) {
            self.job.remaining = 0;
            self.progress_remainder = 0;
        } else {
            self.job.remaining -= whole as u64;
            self.progress_remainder = (credit % u128::from(quantum_millis)) as u64;
        }
        Ok(WorkProgress {
            consumed_millis: consumed,
            completed: self.job.remaining == 0,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub custody: State,
    pub goods: Vec<Good>,
    pub recipes: Vec<Recipe>,
    pub tick_millis: u64,
    pub work: Vec<Work>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_production_model::JobPhase;
    use rayon::prelude::*;

    fn work() -> Work {
        Work {
            job: Job {
                id: 1,
                worker: 1,
                region: 1,
                recipe: 1,
                remaining: 3,
                phase: JobPhase::Working,
                reserved: vec![].into(),
                outputs: vec![].into(),
            },
            phase: Phase::Working,
            progress_remainder: 0,
        }
    }

    #[test]
    fn fractional_progress_is_partition_independent_and_survives_reload() {
        let mut whole = work();
        assert_eq!(whole.completion_after(2, 1000).unwrap(), Some(1500));
        assert_eq!(
            whole.advance_elapsed(2, 1000, 1000).unwrap(),
            WorkProgress {
                consumed_millis: 1000,
                completed: false
            }
        );
        let mut split = work();
        split.advance_elapsed(2, 333, 1000).unwrap();
        assert_eq!((split.job.remaining, split.progress_remainder), (3, 666));
        split = serde_json::from_str(&serde_json::to_string(&split).unwrap()).unwrap();
        split.advance_elapsed(2, 667, 1000).unwrap();
        assert_eq!(split, whole);
        assert_eq!(
            split.advance_elapsed(2, 1000, 1000).unwrap(),
            WorkProgress {
                consumed_millis: 500,
                completed: true
            }
        );
        assert_eq!((split.job.remaining, split.progress_remainder), (0, 0));
    }

    #[test]
    fn rate_changes_keep_remainders_and_invalid_inputs_are_inert() {
        let mut state = work();
        state.advance_elapsed(1, 500, 1000).unwrap();
        assert_eq!(state.completion_after(2, 1000).unwrap(), Some(1250));
        state.advance_elapsed(2, 250, 1000).unwrap();
        assert_eq!((state.job.remaining, state.progress_remainder), (2, 0));
        let before = state.clone();
        assert!(state.advance_elapsed(1, 1, 0).is_err());
        assert_eq!(state, before);
        state.phase = Phase::Suspended { until: 3 };
        let before = state.clone();
        assert!(state.advance_elapsed(1, 1, 1000).is_err());
        assert_eq!(state, before);
        let mut old = serde_json::to_value(work()).unwrap();
        old.as_object_mut().unwrap().remove("progress_remainder");
        assert_eq!(
            serde_json::from_value::<Work>(old)
                .unwrap()
                .progress_remainder,
            0
        );
    }

    #[test]
    fn independent_fractional_work_matches_across_worker_counts() {
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut jobs: Vec<_> = (1..=4096)
                        .map(|id| {
                            let mut row = work();
                            row.job.id = id;
                            row.job.worker = id;
                            row
                        })
                        .collect();
                    jobs.par_iter_mut().for_each(|job| {
                        let rate = job.job.id % 5 + 1;
                        job.advance_elapsed(rate, 317, 1000).unwrap();
                        job.advance_elapsed(rate, 683, 1000).unwrap();
                    });
                    jobs
                })
        };
        assert_eq!(run(1), run(4));
    }
}
