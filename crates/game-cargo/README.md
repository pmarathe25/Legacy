# game-cargo

Physical inventories, checked quantity accounting, atomic local transfers, carried custody, and output-capacity reservations. Permission belongs to property; goods cannot move between areas without transport.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-transport](../game-transport/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-cargo`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
