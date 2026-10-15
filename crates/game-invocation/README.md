# game-invocation

Selected past-life memories remain temporary. Authored cost thresholds stop advancement for scoped consent; accepted loss of control blocks player input and dismissal while subsequent costs proceed. Saves preserve warnings and committed intervals.

**Dependencies:** [game-conflict](../game-conflict/README.md), [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-lineage](../game-lineage/README.md), [game-physiology](../game-physiology/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-invocation`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.

Combat defines a control query; this plugin serves it and asks combat to resume automatic turns during accepted control loss. No combat rules live here.
