# game-core

Typed identities, milliseconds, checked time arithmetic, errors, and deterministic counter-based randomness. No game state or host dependency.

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-core`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
