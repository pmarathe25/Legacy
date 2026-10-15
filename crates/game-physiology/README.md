# game-physiology

Owns hunger, fatigue, rest, injury, mortal death, and in-place immortal recovery. Pure numeric batches use a bounded CPU pool; costs and rates live in one rules record.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-transport](../game-transport/README.md), [game-cargo](../game-cargo/README.md), [game-property](../game-property/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-physiology`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
