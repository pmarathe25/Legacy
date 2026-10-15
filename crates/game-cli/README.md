# game-cli

Small text adapter for the same public commands as a future renderer. JSON command input exposes every typed API command without duplicating gameplay logic.

**Dependencies:** [game-core](../game-core/README.md), [game-api](../game-api/README.md), [game-assembly](../game-assembly/README.md), [game-sim](../game-sim/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-cli`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
