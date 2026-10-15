# game-projects

Proposals, physical in-kind contributions, material consumption, construction time, failure, and an operating site. Claims and contributed quantities remain explicit rather than creating an automatic enterprise ownership rule.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-cargo](../game-cargo/README.md), [game-property](../game-property/README.md), [game-transport](../game-transport/README.md), [game-physiology](../game-physiology/README.md), [game-skills](../game-skills/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-projects`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
