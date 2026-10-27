# game-sim

`game-sim` is the single generic transactional simulation host that manages typed resources, scheduled requests, event delivery, canonical actor actions, and validated save files.

**Why it matters:** Centralizing scheduling, atomic transactions, nested rollback, and owner-checked resource access in one kernel guarantees determinism and prevents feature crates from mutating each other's state.

## Responsibilities

- **Plugin & resource registry (`Builder`, `Host`):** Registers named plugins, typed `Resource` state, `Request` handlers, `Event` / `notify` subscribers, and action cancellation hooks.
- **Transactional execution (`Transaction`):** Executes every request or batched phase atomically, enforcing that only a resource's declaring plugin may call `tx.edit::<R>()`.
- **Single simulation clock & scheduler:** Orders due scheduled jobs by `(SimTime, priority, JobId)`, supports bounded `Builder::batch<R>(limit)` phases (up to 128 adjacent requests), and deduplicates public commands by sequence number.
- **Canonical action table (`Actions`):** Ensures each `PersonId` performs at most one timed action at a time, routing interruptions through the owning feature's cancellation request and waking listeners on completion.
- **Save & load validation:** Serializes resources, pending typed requests, action clocks, notices, and command logs into validated JSON saves.

## Boundaries

- **Zero gameplay rules:** Knows nothing about food, combat, or institutions; depends only on `game-core`, `game-storage`, `game-packed`, and `game-snapshot`.
- **Single execution path:** There is no alternate or approximate simulation kernel—both live play and background history runs execute through `Host`.

## How It Works

- **Lazy checkpoints & nested `attempt`:** Writable transactions checkpoint a resource only when first edited. Cold resources clone persistent `game-storage` roots; hot resources capture a `game-packed` row-journal mark. Errors or worker panics restore all active scopes and discard staged jobs, IDs, and notices.
- **Zero-allocation internal dispatch:** Scheduled `Pending` jobs and internal `notify` signals invoke registered handlers with borrowed typed payloads directly, converting to JSON only at save/client boundaries.
- **Incremental validation:** After a phase, edited resources run `validate_changed` against their checkpoint; save construction and loading validate complete resources and bind all pending request payloads before exposing the host.

## Game Integration

- **Plugin / Entry Point:** Core host library (`Builder`, `Host`, `Transaction`, `Resource`, `Request`, `Event`, `Pending`).
- **Requests & Events:** Built-in `Actions` resource (`sim.actions`) and `ActionFinished` / `ActionCancelled` lifecycle notifications.
- **Callers:** Every hosted feature crate registers via `plugin(&mut Builder)`; [`game-assembly`](../game-assembly/README.md) constructs and drives `Host`.

## Dependencies & Testing

- **Dependencies:** [`game-contracts`](../game-contracts/README.md), [`game-core`](../game-core/README.md), [`game-storage`](../game-storage/README.md), [`game-packed`](../game-packed/README.md), [`game-snapshot`](../game-snapshot/README.md).
- **Testing:** `cargo test -p game-sim` verifies owner-checked writes, nested `attempt` rollback, batched phases, action cancellation, worker panic recovery, and save/load round-trips.

`Builder::load_from` decodes one resource at a time through its registered codec using `game-snapshot`, without retaining every serialized resource. The string API uses the same codecs. Both paths validate the complete restored world and bind pending requests before exposing the host.

`Pending::immediate` provides the same typed call path as `Transaction::call` for
work consumed within the current transaction. Retained work must pass through
`Pending::new`; scheduling validates nested payloads before committing. Invalid
nested requests still roll back allocations and state. This avoids serializing
discarded temporary offers.

A request owner can retire its callbacks with
`Transaction::cancel_scheduled::<R>()`. Cancellation includes work staged in the
current transaction, rolls back on failure, and preserves other request types.

With `Host::set_parallel_listeners(true)`, `notify_prepared` drafts explicitly independent subscribers in parallel and merges private resource results in subscriber order. Ordinary subscribers and world effects are serial barriers; rejected speculative drafts retry through the original listener.

Owner-registered `Builder::scheduled_wave` callbacks receive contiguous same-time, same-priority work atomically. They may update disjoint owner rows on workers before publishing ordered events. Urgent jobs created during publication execute after the wave; before-hooks retain ordinary ordered request dispatch. Wave failure rolls back the whole prefix, including jobs and notices.

Resource contracts and erased codecs live in `game-contracts`; streaming envelope
metadata lives in `game-snapshot`. The existing `game-sim` public types are reexports.
The host retains ownership checks and the sole committing execution path.

`notify_prepared` also runs directly inside writable transactions. Ordinary
subscribers are effect barriers between independent drafts. If a draft attempts
an external effect, its callback runs serially at its original position and later
drafts are prepared again against the updated world. Earlier merged drafts remain
part of the same atomic transaction; a later error or panic rolls them back along
with the barrier's effects. Nested isolated `prepare` requests retain their
whole-request fallback because their caller owns the speculative read view.

`Pending::request<R>` inspects an already bound typed payload without serialization
or handler execution. Unbound envelopes return `None`; loading binds scheduled
payloads as usual. `Host::query_at` views the current committed world with a supplied
logical clock at or after `now`; it does not predict future state or advance the
scheduler. It enforces the same read-only contract as `query`. These interfaces
support external advisory preparation without changing the dispatch loop.

The listener execution policy is runtime-only and defaults to ordered callbacks
for all worker counts. It does not change provider barriers, selection, action
execution, IDs or commit boundaries. Large owner-controlled pool operations remain
parallel. Explicit parallel-listener tests continue to cover isolated drafting,
blocked effects, aggregate budgets, ordered merge and rollback.

`review-trace` enables generic causal scopes from `game-contracts/causality` at
request, event, and listener boundaries. It adds no gameplay dependency. Owners
can correlate optional observations with these attempt scopes. The ordinary build
compiles out this backend; see `tools/performance/REVIEWS.md` for diagnostic limits.


Parallel listeners now draft every independent subscriber once across ordered
barriers. A conservative resource-type read footprint is checked at each original
application position; changed inputs retry only that callback. Attempted writes
advance dependency versions even after rollback. Type-hash collisions only cause
extra retries. Missing reads, nested worker reads and caught effect attempts remain
covered. Providers that attempt external effects become serial for that registry;
this classification changes performance only. `draft_statistics` reports attempted,
accepted, stale and effectful drafts. Counts are runtime diagnostics, including
rolled-back attempts, and are not serialized.

`Transaction::draft(scratch, run)` executes a closure with a private scratch
resource. `Transaction::listener(event, listener_name)` invokes a registered
callback under its original owner. A draft returns its scratch state and an
acceptance flag; any attempted external effect rejects acceptance, even when
caught by the callback. Callers may retain only independently completed scratch
groups, never the failed callback's partial result. The opt-in frame experiment
uses these primitives to prepare actor/provider batches; only the existing host
commits worlds.
Generic dependency machinery lives in `game-drafting` through `game-contracts`.
Serializable job/action records and the action row-journal resource contract live
in `game-contracts`; existing public names remain reexports/type aliases.

`Host::executor()` shares the host's worker pool through an `Arc`. A driver can
run its committing loop inside that pool to avoid handing each small batch from
an external coordinator to a worker. Ownership checks, transaction order and
rollback remain the same; reloads create a new host and its own executor.
