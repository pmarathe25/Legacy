# game-logistics

Physical load–travel–unload delivery with capacity, local access, multi-road routing, and blocked-delivery retries. Interrupted deliveries leave cargo with the carrier.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-land](../game-land/README.md), [game-spatial](../game-spatial/README.md), [game-transport](../game-transport/README.md), [game-cargo](../game-cargo/README.md), [game-property](../game-property/README.md), [game-physiology](../game-physiology/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-logistics`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
