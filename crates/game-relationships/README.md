# game-relationships

Directed trust, affection, fear and grievance; separate social beliefs and slowly learned concept values. Interpretation requires an explicit credible observation.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-relationships`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
