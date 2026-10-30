//! Compact canonical job receipts, expanded only at external boundaries.
use game_parallel_packed_jobs_model::Block;
use game_parallel_production_model::{Event, EventKind};
use game_parallel_production_output::Progress;
use rayon::prelude::*;
struct Run {
    first: u64,
    count: u32,
    flags: u8,
}
enum Inner {
    Raw(Vec<Event>),
    Runs { tick: u64, blocks: Vec<Vec<Run>> },
}
pub struct Receipts {
    inner: Inner,
}
impl Default for Receipts {
    fn default() -> Self {
        Self {
            inner: Inner::Raw(Vec::new()),
        }
    }
}
impl Receipts {
    pub fn from_events(events: Vec<Event>) -> Self {
        Self {
            inner: Inner::Raw(events),
        }
    }
    pub fn expand(&self) -> Vec<Event> {
        match &self.inner {
            Inner::Raw(events) => events.par_iter().cloned().collect(),
            Inner::Runs { tick, blocks } => blocks
                .par_iter()
                .flat_map_iter(|block| {
                    block.iter().flat_map(|run| {
                        (0..run.count).flat_map(move |offset| {
                            let job = run.first + u64::from(offset);
                            [
                                (run.flags & 1 != 0).then_some(Event {
                                    tick: *tick,
                                    job,
                                    kind: EventKind::Finished,
                                }),
                                (run.flags & 2 != 0).then_some(Event {
                                    tick: *tick,
                                    job,
                                    kind: EventKind::Delivered,
                                }),
                            ]
                            .into_iter()
                            .flatten()
                        })
                    })
                })
                .collect(),
        }
    }
}
pub fn prepare(
    blocks: &[Block],
    progress: &[Vec<Progress>],
    denied: Option<&[Vec<bool>]>,
    tick: u64,
) -> Receipts {
    let blocks = blocks
        .par_iter()
        .zip(progress.par_iter())
        .enumerate()
        .map(|(block_index, (block, states))| {
            let mut runs: Vec<Run> = Vec::new();
            for (index, (row, state)) in block.rows.iter().zip(states).enumerate() {
                let delivered =
                    state.waiting && denied.is_none_or(|denied| !denied[block_index][index]);
                let flags = u8::from(state.finished) | (u8::from(delivered) << 1);
                if flags == 0 {
                    continue;
                }
                if let Some(run) = runs.last_mut()
                    && run.flags == flags
                    && run.first.checked_add(u64::from(run.count)) == Some(row.id)
                    && run.count < u32::MAX
                {
                    run.count += 1;
                } else {
                    runs.push(Run {
                        first: row.id,
                        count: 1,
                        flags,
                    });
                }
            }
            runs
        })
        .collect();
    Receipts {
        inner: Inner::Runs { tick, blocks },
    }
}

/// Compress statuses indexed by an already job-ID-sorted metadata array.
pub fn prepare_flags(
    jobs: &[Vec<game_parallel_packed_jobs_model::Row>],
    flags: &[std::sync::atomic::AtomicU8],
    tick: u64,
) -> Receipts {
    use std::sync::atomic::Ordering;
    assert_eq!(jobs.iter().map(Vec::len).sum::<usize>(), flags.len());
    let blocks = jobs
        .par_iter()
        .zip(flags.par_chunks(4096))
        .map(|(jobs, flags)| {
            let mut runs: Vec<Run> = Vec::new();
            for (job, flag) in jobs.iter().zip(flags) {
                let flags = flag.load(Ordering::Relaxed);
                if flags == 0 {
                    continue;
                }
                if let Some(run) = runs.last_mut()
                    && run.flags == flags
                    && run.first.checked_add(u64::from(run.count)) == Some(job.id)
                    && run.count < u32::MAX
                {
                    run.count += 1;
                } else {
                    runs.push(Run {
                        first: job.id,
                        count: 1,
                        flags,
                    });
                }
            }
            runs
        })
        .collect();
    Receipts {
        inner: Inner::Runs { tick, blocks },
    }
}
