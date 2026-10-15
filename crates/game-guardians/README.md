# game-guardians

Guardian identity, an innate successor connection, route-by-route search, mentorship assessment, and refusal without abandonment. Recovery remains in physiology and never moves the guardian.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-lineage](../game-lineage/README.md), [game-transport](../game-transport/README.md), [game-land](../game-land/README.md), [game-spatial](../game-spatial/README.md), [game-physiology](../game-physiology/README.md), [game-skills](../game-skills/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-guardians`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
