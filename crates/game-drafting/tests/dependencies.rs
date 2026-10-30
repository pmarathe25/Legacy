use game_drafting::*;
#[path = "support/arbitration.rs"]
mod arbitration;
#[test]
fn negative_reads_and_nested_reads_invalidate_after_attempted_writes() {
    struct Missing;
    struct Nested;
    let outer = Reads::default();
    let nested = Reads::default();
    outer.record::<Missing>();
    nested.record::<Nested>();
    outer.include(&nested);
    let mut versions = Versions::default();
    let at = versions.clock();
    assert!(versions.valid(&outer, at));
    versions.write::<Missing>();
    assert!(!versions.valid(&outer, at));
    let at = versions.clock();
    assert!(versions.valid(&outer, at));
    versions.write::<Nested>();
    assert!(!versions.valid(&outer, at));
}
#[test]
fn preparation_runs_concurrently_and_application_stops_in_order_on_error() {
    use std::sync::{Barrier, Mutex};
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .unwrap();
    let barrier = Barrier::new(2);
    let threads = Mutex::new(vec![]);
    let mut committed = Vec::new();
    let result = ordered(
        &mut committed,
        &pool,
        &[true, false, true],
        |_, i| {
            threads.lock().unwrap().push(std::thread::current().id());
            barrier.wait();
            i
        },
        |committed, i, draft| {
            if i == 1 {
                return Err("barrier failed");
            }
            committed.push(draft.unwrap());
            Ok(())
        },
    );
    assert_eq!(result, Err("barrier failed"));
    assert_eq!(committed, vec![0]);
    let threads = threads.into_inner().unwrap();
    assert_ne!(threads[0], threads[1]);
}
