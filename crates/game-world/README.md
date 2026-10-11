# game-world

`game-world` provides a generic host over one shared component store. `GenericHost<State, Event, Storage>` owns logical time, checked identity and event counters, typed records, and snapshots. Composition supplies the schema, typed state and events, contextual validation, and systems. An application storage wrapper implements the two-method `StorageAccess` interface; it retains the same single Store.

State and storage are separate fields for disjoint borrowing. `parts_mut()` also lends the existing event records without cloning them. `append_many` advances event sequence only; `bump_revision` publishes a caller revision, while `publish_many` advances both. Callers preflight the complete transaction budget before owner writes.

The playable `game-assembly` uses this host for its live storage and generic counters. The host depends only on core types, the private ECS adapter, and the existing simulation engine. Domain behavior remains in its owners; remaining assembly handlers and entity maps are still being extracted.
