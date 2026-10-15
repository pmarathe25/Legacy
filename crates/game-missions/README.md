# game-missions

Consenting parties, preparation, shared strategic travel, arrival, return, and mortality consequences. Mission members occupy real actions; the rest of the world continues independently.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-physiology](../game-physiology/README.md), [game-transport](../game-transport/README.md), [game-land](../game-land/README.md), [game-spatial](../game-spatial/README.md), [game-cargo](../game-cargo/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-missions`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
