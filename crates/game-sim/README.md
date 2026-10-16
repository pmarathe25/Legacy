# game-sim

One generic transactional host: typed resources, due requests, stable event order, command deduplication, bounded advancement, and validated saves. Hot feature resources use `game-packed` row journals; cold resources retain `game-storage` snapshots. Nested and pending typed requests invoke their registered owner directly; saves and public commands use JSON envelopes. Errors and worker panics discard staged writes, IDs, notices, and jobs.

During a writable request the transaction owns the resource map. Reads use one type index; owners edit unique resources directly after taking a lazy checkpoint. The map is restored to the host before returning either success or failure. Queries borrow committed resources and reject mutation.

Nested `attempt` scopes checkpoint only resources they edit. Cold resources default to independent clones with persistent table roots. Hot owners capture a row-journal mark plus their small metadata, retain successful inner undo until the outer commit, and restore all active scopes after a request/validation error or worker panic. Storage hooks perform infallible bookkeeping; gameplay stays in registered requests and subscribers.

Staged jobs, cancellations, and notices use append-only vectors with length checkpoints. Cancelling staged work records its vector index. Resource owners use registry-local numbers, with names retained in saves. Logical serialization, batching, and due work retain stable order.

`Builder::batch<R>(limit)` lets an owner group adjacent due requests of the same type, time, and priority into an atomic phase of at most 128 requests. New earlier work closes that phase before the next captured request. Cancellation retains queue order. Public commands remain individual transactions; commit revisions count phases. Physiology opts wake-ups into this batching.

`notify` delivers internal lifecycle signals through the same owner-checked subscribers as events, without adding a player notice. Request hooks and event subscribers borrow typed payloads directly. Internal delivery performs no JSON conversion or subscriber-vector allocation. Published notices serialize once; saves retain their logical JSON representation. Finishing or cancelling an action wakes interested decision owners; rejected subscriber work rolls back the enclosing phase.

Scheduled requests and cancellation requests retain immutable typed payloads behind a shared interface; invocation clones their values and uses the same owner-checked executor. Requests implement `Clone` and must not share mutable gameplay data. Live and loaded requests must have the same meaning; derived caches cannot change their effects. Construction validates JSON serialization without retaining a JSON tree. Loading borrows raw request JSON into each registered decoder, binds every pending payload, and rejects unknown or malformed requests before exposing the host.

Owners validate changed records against a checkpoint; save construction and loading still validate complete resources. Job cancellation uses an ID index. Save loading borrows resource JSON from its input; pending payloads use compact raw JSON while waiting to bind. There are no feature dependencies or alternate kernels.

**Dependencies:** [game-core](../game-core/README.md), [game-storage](../game-storage/README.md), [game-packed](../game-packed/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-sim`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
