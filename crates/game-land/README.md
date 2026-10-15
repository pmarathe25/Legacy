# game-land

Owns named areas and positive-duration bidirectional links. Construction requests validate and allocate stable IDs; no scenario or movement logic.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-land`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
