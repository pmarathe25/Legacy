//! A stable coordinator with ordinary stealing or optional dedicated-core waiting.
use std::sync::{
    Mutex,
    atomic::{
        AtomicBool,
        Ordering::{Acquire, Release},
    },
};
struct Done<'a>(&'a AtomicBool);
impl Drop for Done<'_> {
    fn drop(&mut self) {
        self.0.store(true, Release);
    }
}
pub fn run<R: Send>(pool: &rayon::ThreadPool, active: bool, work: impl FnOnce() -> R + Send) -> R {
    if pool.current_num_threads() == 1 {
        return pool.install(work);
    }
    let work = Mutex::new(Some(work));
    let result = Mutex::new(None);
    let done = AtomicBool::new(false);
    pool.broadcast(|context| {
        if context.index() == 0 {
            let _done = Done(&done);
            let work = work.lock().unwrap().take().expect("one coordinator");
            *result.lock().unwrap() =
                Some(std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)));
        } else if active {
            while !done.load(Acquire) {
                let _ = rayon::yield_now();
            }
        }
    });
    match result.into_inner().unwrap().expect("coordinator result") {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use rayon::prelude::*;
    #[test]
    fn resident_and_active_workers_execute_parallel_work_and_join_after_panics() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .unwrap();
        for active in [false, true] {
            let barrier = std::sync::Barrier::new(4);
            let total = run(&pool, active, || {
                assert_eq!(pool.current_thread_index(), Some(0));
                (0u64..4)
                    .into_par_iter()
                    .map(|i| {
                        barrier.wait();
                        i
                    })
                    .sum::<u64>()
            });
            assert_eq!(total, 6);
        }
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run(&pool, true, || panic!("coordinator failure"))
        }));
        assert!(panic.is_err());
        assert_eq!(run(&pool, false, || 7), 7);
    }
}
