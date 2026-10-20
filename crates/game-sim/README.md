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

`Transaction::prepare` runs a scratch request alongside a read-only query on the
same staged world. `notify_prepared` drafts explicitly independent subscribers in
parallel and merges their private resource results in subscriber order. Ordinary
subscribers are serial barriers. World writes, IDs, notices and schedule changes
are denied during drafting; any such attempt, even if caught, retries the original
request serially. Retained scratch changes also force a serial retry. Selection,
action execution and world commits remain serial. One worker uses the original path.

Resource contracts and erased codecs live in `game-contracts`; streaming envelope
metadata lives in `game-snapshot`. The existing `game-sim` public types are reexports.
The host retains ownership checks and the sole committing execution path.
