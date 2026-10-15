# game-sim

One generic transactional host: typed resources, due requests, stable event order, command deduplication, bounded advancement, and validated saves. Numeric and sparse resources use `game-storage` snapshots. Nested typed requests invoke their registered owner directly; persisted jobs and public commands use envelopes. Errors and worker panics discard staged writes, IDs, notices, and jobs.

Owners can validate changed records against the committed resource; save construction and loading still validate complete resources. Job cancellation uses an ID index. There are no feature dependencies or alternate kernels.

**Dependencies:** [game-core](../game-core/README.md), [game-storage](../game-storage/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-sim`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
