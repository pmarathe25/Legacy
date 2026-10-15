# game-session

A dedicated host thread processes input between bounded advances and coalesces ordinary progress into one slot. Terminal stops, accepted command results, and requested saves and views remain explicit responses.

**Dependencies:** [game-core](../game-core/README.md), [game-api](../game-api/README.md), [game-assembly](../game-assembly/README.md), [game-sim](../game-sim/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-session`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
