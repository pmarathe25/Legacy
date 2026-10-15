# game-justice

Explicit complaints, evidence, bounded reasoning over a judge's known precedents, decisions, and physical enforcement. A judgment itself moves no goods.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-property](../game-property/README.md), [game-cargo](../game-cargo/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-justice`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
