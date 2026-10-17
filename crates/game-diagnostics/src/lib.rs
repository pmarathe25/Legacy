//! Optional wall-clock measurements, kept outside simulation state and saves.
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
    time::Instant,
};
#[derive(Default, Clone, Serialize)]
pub struct Measurement {
    pub calls: u64,
    pub nanoseconds: u64,
}
static DATA: OnceLock<Mutex<BTreeMap<&'static str, Measurement>>> = OnceLock::new();
static ENABLED: OnceLock<bool> = OnceLock::new();
pub struct Span(Option<(&'static str, Instant)>);
pub fn span(key: &'static str) -> Span {
    Span(
        if *ENABLED.get_or_init(|| std::env::var_os("LEGACY_PROFILE").is_some()) {
            Some((key, Instant::now()))
        } else {
            None
        },
    )
}
impl Drop for Span {
    fn drop(&mut self) {
        if let Some((key, at)) = self.0 {
            let mut data = DATA.get_or_init(Default::default).lock().unwrap();
            let m = data.entry(key).or_default();
            m.calls += 1;
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
