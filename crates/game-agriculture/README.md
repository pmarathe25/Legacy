# game-agriculture

Sowing consumes seed and labor; growing follows configurable time and climate; harvesting consumes labor and produces food. Interrupted sowing records lost inputs, and interrupted harvest leaves the crop.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-transport](../game-transport/README.md), [game-cargo](../game-cargo/README.md), [game-property](../game-property/README.md), [game-physiology](../game-physiology/README.md), [game-environment](../game-environment/README.md), [game-skills](../game-skills/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-agriculture`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.

**Content:** [Store format and extension guide](../../content/README.md). Definitions are read from files at startup and resolved snapshots survive save/load. The schema and validation live in this crate.
