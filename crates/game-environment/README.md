# game-environment

Configurable calendar and periodic climate lookup using integer time. No hidden setting or weather defaults.

`ConfigureArea` assigns a regional climate; `at(transaction, area)` resolves that area's season, falling back to the global `Climate`. The resolved `Regions` resource persists alongside the default. Agriculture uses local conditions at growth completion; ecology can use them to gate daily renewal. Profiles remain authored periodic seasons rather than a weather simulation.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-environment`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
