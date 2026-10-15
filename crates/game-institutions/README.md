# game-institutions

Founding and explicit procedures, bounded access-rule proposals, voting with executable permission consequences, offices, and removal. Institutional power is separate from the ruler lineage.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-property](../game-property/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-institutions`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
