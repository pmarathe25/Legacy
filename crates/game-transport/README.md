# game-transport

Owns strategic positions, timed movement, arrival notifications, and cancellation on roads. Other features ask for movement and subscribe to arrivals.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-land](../game-land/README.md), [game-spatial](../game-spatial/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-transport`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
