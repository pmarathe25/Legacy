# game-confinement

Custodial confinement, explicit release, and bounded escape checks. Confined actors cannot travel; a guardian can regenerate while remaining in the same cell.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-physiology](../game-physiology/README.md), [game-transport](../game-transport/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-confinement`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
