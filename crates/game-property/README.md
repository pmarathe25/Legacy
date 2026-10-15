# game-property

Conflicting ownership claims, recognition, revocable per-person permission, and physical taking. Unauthorized taking conserves goods and only informs explicit witnesses.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-cargo](../game-cargo/README.md), [game-transport](../game-transport/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-property`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
