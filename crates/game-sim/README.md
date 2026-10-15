# game-sim

One generic transactional host: typed resources, due requests, stable event order, command deduplication, bounded advancement, and validated saves. Numeric and sparse resources use `game-storage` snapshots. Nested typed requests invoke their registered owner directly; persisted jobs and public commands use envelopes. Errors and worker panics discard staged writes, IDs, notices, and jobs.

Nested `attempt` checkpoints share the staged resource table; only subsequently edited resources copy their snapshot. Resource owners and registered handlers share immutable names. Internal type lookup uses hash indexes; logical serialization and due work retain stable ordering.

`Builder::batch<R>(limit)` lets an owner group adjacent due requests of the same type, time, and priority into an atomic phase of at most 128 requests. New earlier work closes that phase before the next captured request. Cancellation retains queue order. Public commands remain individual transactions; commit revisions count phases. Physiology opts wake-ups into this batching.

`notify` delivers internal lifecycle signals through the same owner-checked subscribers as events, without adding a player notice. Request hooks and event subscribers borrow typed payloads directly. Internal delivery performs no JSON conversion or subscriber-vector allocation. Published notices serialize once; saves and queued envelopes retain their logical JSON representation. Finishing or cancelling an action wakes interested decision owners; rejected subscriber work rolls back the enclosing phase.

Owners can validate changed records against the committed resource; save construction and loading still validate complete resources. Job cancellation uses an ID index. Save loading borrows resource JSON from its input instead of copying those strings. There are no feature dependencies or alternate kernels.

**Dependencies:** [game-core](../game-core/README.md), [game-storage](../game-storage/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-sim`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
