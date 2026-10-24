//! Optional conservative resource accesses on the calling thread, never gameplay state.
use std::{cell::RefCell, collections::BTreeSet};

#[derive(Default, Debug)]
pub struct Accesses {
    pub reads: BTreeSet<&'static str>,
    pub writes: BTreeSet<&'static str>,
}
thread_local! {
    static CURRENT: RefCell<Option<Accesses>> = const { RefCell::new(None) };
}
/// Start a single-threaded diagnostic boundary. Worker-thread accesses are not captured.
pub fn begin() {
    CURRENT.with(|slot| *slot.borrow_mut() = Some(Accesses::default()));
}
pub fn finish() -> Accesses {
    CURRENT.with(|slot| slot.borrow_mut().take().unwrap_or_default())
}
pub(crate) fn read(key: &'static str) {
    CURRENT.with(|slot| {
        if let Some(accesses) = slot.borrow_mut().as_mut() {
            accesses.reads.insert(key);
        }
    });
}
pub(crate) fn write(key: &'static str) {
    CURRENT.with(|slot| {
        if let Some(accesses) = slot.borrow_mut().as_mut() {
            accesses.writes.insert(key);
        }
    });
}
