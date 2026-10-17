# game-workshop

Configurable input recipes, time-consuming production, attributed learning, and explicit waste from interruptions. One worker cannot manufacture and travel simultaneously.

Completion also requires the worker and output inventory to remain together. If another carrier has taken the supplies away, the work is canceled, reserved space is released, and consumed inputs become waste. No output or sentimental recognition is created at a distance.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-cargo](../game-cargo/README.md), [game-property](../game-property/README.md), [game-transport](../game-transport/README.md), [game-physiology](../game-physiology/README.md), [game-skills](../game-skills/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-workshop`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.

**Content:** [Store format and extension guide](../../content/README.md). Definitions are read from files at startup and resolved snapshots survive save/load. The schema and validation live in this crate.
