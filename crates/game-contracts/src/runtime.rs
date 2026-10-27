//! Deterministic scheduling and durable command-receipt bookkeeping.
use crate::{Envelope, Job, Notice, Receipt, json_error};
use game_core::{Error, Result, ScheduleId, SimTime, require};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::BTreeMap;
pub type ScheduleKey = (SimTime, u8, ScheduleId);
pub type ScheduledPrefix<P> = Vec<(Option<ScheduleKey>, P)>;

/// Capture a contiguous due phase; other request kinds are visibility barriers.
pub fn scheduled_prefix<P: Clone>(
    jobs: &BTreeMap<(SimTime, u8, ScheduleId), Job<P>>,
    first: Option<(SimTime, u8, ScheduleId)>,
    request: P,
    limit: usize,
    same: impl Fn(&P, &P) -> bool,
) -> ScheduledPrefix<P> {
    let mut requests = vec![(first, request)];
    if let Some(first) = first {
        for (key, job) in jobs
            .range((std::ops::Bound::Excluded(first), std::ops::Bound::Unbounded))
            .take(limit - 1)
        {
            if (key.0, key.1) != (first.0, first.1) || !same(&requests[0].1, &job.request) {
                break;
            }
            requests.push((Some(*key), job.request.clone()));
        }
    }
    requests
}

pub fn validate_notices<'a>(
    notices: impl IntoIterator<Item = &'a Notice>,
    time: SimTime,
    next_id: u64,
    next_notice: u64,
) -> Result<()> {
    require(next_id > 0 && next_notice > 0, "invalid saved counters")?;
    let mut previous = 0;
    for notice in notices {
        let valid = notice.at <= time && notice.sequence < next_notice;
        require(valid && notice.sequence > previous, "invalid saved event")?;
        previous = notice.sequence;
    }
    Ok(())
}

pub fn previous_receipt<T: DeserializeOwned>(
    receipts: &BTreeMap<u64, Receipt>,
    sequence: u64,
    envelope: &Envelope,
) -> Result<Option<T>> {
    let Some(receipt) = receipts.get(&sequence) else {
        return Ok(None);
    };
    if receipt.request != *envelope {
        return Err(Error::SequenceConflict);
    }
    serde_json::from_value(receipt.response.clone())
        .map(Some)
        .map_err(json_error)
}
pub fn record_receipt(
    receipts: &mut BTreeMap<u64, Receipt>,
    sequence: u64,
    request: Envelope,
    response: Value,
) {
    receipts.insert(sequence, Receipt { request, response });
}
pub fn commit_jobs<P>(
    jobs: &mut BTreeMap<(SimTime, u8, ScheduleId), Job<P>>,
    keys: &mut BTreeMap<ScheduleId, (SimTime, u8, ScheduleId)>,
    removed: usize,
    cancelled: Vec<ScheduleId>,
    pending: Vec<Job<P>>,
) {
    for _ in 0..removed {
        let (key, _) = jobs.pop_first().expect("committed due prefix");
        keys.remove(&key.2);
    }
    for id in cancelled {
        if let Some(key) = keys.remove(&id) {
            jobs.remove(&key);
        }
    }
    for job in pending {
        let key = (job.at, job.priority, job.id);
        keys.insert(job.id, key);
        jobs.insert(key, job);
    }
}
