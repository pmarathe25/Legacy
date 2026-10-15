# game-genetics

Seeded inheritance, bounded mutation, and trait expression as pure functions. Does not depend on the host or assign social meaning to genes.

**Dependencies:** [game-core](../game-core/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-genetics`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
