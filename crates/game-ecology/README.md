# game-ecology

Timed gathering reserves finite stock, consumes one canonical action, produces physical goods, and credits learning. Cancellation releases reservations; renewal is explicitly scheduled.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-cargo](../game-cargo/README.md), [game-transport](../game-transport/README.md), [game-physiology](../game-physiology/README.md), [game-skills](../game-skills/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-ecology`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
