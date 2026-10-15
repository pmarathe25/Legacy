# game-cargo

Physical inventories, checked quantity accounting, atomic local transfers, carried custody, and output-capacity reservations. Permission belongs to property; goods cannot move between areas without transport.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-transport](../game-transport/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-cargo`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.

Balances and reservations use bounded-copy numeric tables inside each inventory. `used()` reads cached reserved mass; capacity checks do not scan the reservation book. Owner requests update that cache atomically. Full save validation recalculates it and rejects mismatches.

Carrier lookup indexes only carried containers. Dropping an actor’s load visits those containers, with no scan of unrelated world inventories. The index is rebuilt on load and omitted from saves.
