# game-conflict

Configurable tactical fixture with generated continuous geometry, simultaneous movement/attacks, fixed world time per shared round, player input stops, and lasting bodily damage. Final tactical game rules remain a design decision.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-physiology](../game-physiology/README.md), [game-transport](../game-transport/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-conflict`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.

Resolved attacks emit attributed `Struck` events after all simultaneous damage has been applied. Upper social owners can interpret direct evidence without changing the conflict resolver or receiving hidden tactical orders. Pressure engagements use the same damage and cancellation path as other encounters.
