# game-realm

Mandates attach to recognized offices and impose jurisdiction and spending limits. Delegates must physically deliver goods. Lineage continuity cannot restore an office or budget.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-institutions](../game-institutions/README.md), [game-property](../game-property/README.md), [game-cargo](../game-cargo/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-realm`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
