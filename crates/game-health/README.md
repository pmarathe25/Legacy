# game-health

Actual treatment consumes medicine and caregiver time. Completion rechecks locality and patient survival, so a remote task does not heal a patient.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-physiology](../game-physiology/README.md), [game-cargo](../game-cargo/README.md), [game-property](../game-property/README.md), [game-transport](../game-transport/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-health`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
