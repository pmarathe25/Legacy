# game-spatial

Pure deterministic shortest-path search. Equal-cost alternatives use stable area ordering. Can be tested without a host.

**Dependencies:** [game-core](../game-core/README.md), [game-land](../game-land/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-spatial`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
