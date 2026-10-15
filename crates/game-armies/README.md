# game-armies

Individual recruitment, unit membership, real food consumption, and mission deployment. Unit coordination uses the same party/travel mechanics as expeditions.

**Dependencies:** [game-property](../game-property/README.md), [game-transport](../game-transport/README.md), [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-missions](../game-missions/README.md), [game-cargo](../game-cargo/README.md), [game-physiology](../game-physiology/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-armies`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
