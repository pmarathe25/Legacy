//! Optional sampled call stacks, outside gameplay and disabled in ordinary builds.
use std::{
    backtrace::Backtrace,
    cell::Cell,
    collections::BTreeMap,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
static ACTIVE: AtomicBool = AtomicBool::new(false);
static SAMPLES: Mutex<Vec<Backtrace>> = Mutex::new(Vec::new());
thread_local! { static BUSY: Cell<bool> = const { Cell::new(false) }; }
/// Begin sampling advancement after checkpoint loading has finished.
pub fn start() {
    ACTIVE.store(false, Ordering::Relaxed);
    SAMPLES.lock().unwrap().clear();
    ACTIVE.store(true, Ordering::Relaxed);
}
pub(super) fn record(count: usize) {
    if !ACTIVE.load(Ordering::Relaxed) {
        return;
    }
    let mixed = (count as u64).wrapping_mul(0x9e3779b97f4a7c15);
    if (mixed ^ (mixed >> 32)) & 8191 != 0 {
        return;
    }
    // Unwinding, storage and mutex bookkeeping can allocate. They must not sample
    // recursively, including while a worker's thread-local storage is being torn down.
    let _ = BUSY.try_with(|busy| {
        if busy.replace(true) {
            return;
        }
        let stack = Backtrace::force_capture();
        let mut samples = SAMPLES.lock().unwrap();
        if samples.len() < 8192 {
            samples.push(stack);
        }
        drop(samples);
        busy.set(false);
    });
}
/// Resolve and group stacks only after sampling stops. Formatting is excluded.
pub fn finish() -> BTreeMap<String, usize> {
    ACTIVE.store(false, Ordering::Relaxed);
    let samples = std::mem::take(&mut *SAMPLES.lock().unwrap());
    let mut counts = BTreeMap::new();
    for stack in samples {
        *counts.entry(stack.to_string()).or_default() += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sampled_allocator_stacks_can_stop_and_restart_without_sampling_their_own_formatting() {
        start();
        record(0);
        assert!(!finish().is_empty());
        record(0);
        assert!(finish().is_empty());
        start();
        record(0);
        assert!(!finish().is_empty());
    }
}
