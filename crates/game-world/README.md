# game-world

`game-world` provides a generic host over one shared component store. `GenericHost<State, Event, Storage>` owns logical time, checked identity and event counters, typed records, and snapshots. Composition supplies the schema, typed state and events, contextual validation, and systems. An application storage wrapper implements the two-method `StorageAccess` interface; it retains the same single Store.

State and storage are separate fields for disjoint borrowing. `parts_mut()` also lends the existing event records without cloning them. `append_many` advances event sequence only; `bump_revision` publishes a caller revision, while `publish_many` advances both. Callers preflight the complete transaction budget before owner writes.

`Transaction` combines one `Batch`, owner-bound `ReadGuard`s, allocation count, typed events, and an explicit revision budget. Capture `transaction_opening()` before preparation; its opaque lineage prevents reuse across separate hosts, clones, or restores. `commit_transaction` rejects changed time, revision, identity or event cursors, validates all guards, checks empty identity reservations and registered schemas, then publishes the batch and records atomically. New entities must lie in the reserved ID interval. Rejected admission preserves component rows, counters, events, revisions, and runtime state. Owners validate their private metadata before host admission and finalize precomputed metadata infallibly after success.

The playable `game-assembly` uses this host for its live storage and generic counters. The host depends only on core types, the private ECS adapter, and the existing simulation engine. Domain behavior remains in its owners; remaining assembly handlers and entity maps are still being extracted.
