# game-facts

An immutable visible scene can be shared across observers while each observer keeps its own source, time, revision, and confidence. This crate owns the representation; the observing feature determines visibility and when to refresh it.

`Scene<V>` stores sorted keys and shared values. New scene versions reuse unchanged payloads and key layouts. `Facts<V>` combines a scene and one personal evidence stamp with persistent cold facts. Refreshing an unchanged scene replaces one stamp rather than editing every visible fact.

An explicit report or inspection can override a scene entry. The next scene observation supersedes that override. Changing the visible key set freezes old observations into cold history, preserving old evidence for places and objects that are no longer visible.

`get`, `iter`, and `range` return borrowed `FactRef` values. Reading a view does not copy payloads or increment their reference counts. Use `to_owned` when a report or snapshot must outlive that view. Serialization emits the ordinary logical fact map in key order, without exposing scene layout.

The shared decoder uses a typed payload when possible and borrows an existing JSON value for fallback decoding. Namespace scans and evidence-time validation also live with the generic fact representation.

Clones preserve independent evidence and immutable history. Tests cover shared payloads, changes of scene membership, explicit overrides, retained readers, ranges, duplicate scene keys, and exact logical serialization.

Payload deltas carry a private version token. Metadata-only updates retain that token, allowing derived queries to skip unchanged payloads. The token is never serialized and cannot grant knowledge or suppress changes after restoring a different snapshot.

Run `cargo test -p game-facts`.
