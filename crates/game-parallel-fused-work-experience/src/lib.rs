//! Personal work-request planning and physically witnessed outcomes.
pub use game_parallel_work_asking as ask;
pub mod memory;

#[macro_export]
macro_rules! commit_work_memory {
    ($host:expr, $draft:expr) => {
        if let Some(memory) = $draft.state.work_memory {
            memory.commit(&mut $host.template, $draft.tick);
        }
    };
}
