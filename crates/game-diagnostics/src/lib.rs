//! Optional wall-clock measurements, kept outside simulation state and saves.
use serde::Serialize;
mod interactions;
#[cfg(feature = "interaction-profile")]
pub use interactions::{count, interaction_enabled, interaction_meal, ordered_proposal};
use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
    time::Instant,
};
#[derive(Default, Clone, Serialize)]
pub struct Measurement {
    pub calls: u64,
    pub nanoseconds: u64,
    #[cfg(feature = "allocations")]
    pub allocations: u64,
}
static DATA: OnceLock<Mutex<BTreeMap<&'static str, Measurement>>> = OnceLock::new();
static ENABLED: OnceLock<bool> = OnceLock::new();
#[cfg(feature = "allocations")]
static ALLOCATIONS: OnceLock<fn() -> u64> = OnceLock::new();
#[cfg(feature = "allocations")]
pub fn install_allocation_counter(counter: fn() -> u64) -> bool {
    ALLOCATIONS.set(counter).is_ok()
}
#[cfg(feature = "allocations")]
fn allocation_count() -> u64 {
    ALLOCATIONS.get().map_or(0, |read| read())
}
pub struct Span {
    inner: Option<(&'static str, Instant)>,
    #[cfg(feature = "allocations")]
    allocated: u64,
}
pub fn span(key: &'static str) -> Span {
    let inner = if *ENABLED.get_or_init(|| std::env::var_os("LEGACY_PROFILE").is_some()) {
        Some((key, Instant::now()))
    } else {
        None
    };
    Span {
        #[cfg(feature = "allocations")]
        allocated: if inner.is_some() {
            allocation_count()
        } else {
            0
        },
        inner,
    }
}
impl Drop for Span {
    fn drop(&mut self) {
        if let Some((key, at)) = self.inner {
            #[cfg(feature = "allocations")]
            let allocated = allocation_count().saturating_sub(self.allocated);
            let mut data = DATA.get_or_init(Default::default).lock().unwrap();
            let m = data.entry(key).or_default();
            m.calls += 1;
            #[cfg(feature = "allocations")]
            {
                m.allocations = m.allocations.saturating_add(allocated);
            }
            m.nanoseconds = m
                .nanoseconds
                .saturating_add(at.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64);
        }
    }
}
pub fn snapshot() -> BTreeMap<&'static str, Measurement> {
    DATA.get_or_init(Default::default).lock().unwrap().clone()
}

static FAILURES: OnceLock<Mutex<BTreeMap<String, u64>>> = OnceLock::new();
pub fn failure<T, E: std::fmt::Display>(result: &Result<T, E>) {
    if !*ENABLED.get_or_init(|| std::env::var_os("LEGACY_PROFILE").is_some()) {
        return;
    }
    if let Err(error) = result {
        let mut all = FAILURES.get_or_init(Default::default).lock().unwrap();
        let reason = error.to_string();
        let n = all.entry(reason.clone()).or_default();
        if *n == 0 {
            eprintln!("simulation failure: {reason}");
        }
        *n += 1;
    }
}
pub fn failures() -> BTreeMap<String, u64> {
    FAILURES
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .clone()
}

#[cfg(all(test, feature = "allocations"))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNT: AtomicU64 = AtomicU64::new(0);
    fn read() -> u64 {
        COUNT.load(Ordering::Relaxed)
    }
    #[test]
    fn optional_spans_measure_counter_deltas_without_modifying_simulation_state() {
        let _ = ENABLED.set(true);
        assert!(install_allocation_counter(read));
        COUNT.store(10, Ordering::Relaxed);
        let measured = span("counter-test");
        COUNT.store(37, Ordering::Relaxed);
        drop(measured);
        let result = snapshot();
        assert_eq!(result["counter-test"].calls, 1);
        assert_eq!(result["counter-test"].allocations, 27);
    }
}
