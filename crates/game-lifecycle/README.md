# game-lifecycle

Actual scheduled pregnancies and births with stable identities, inherited traits, caregivers, and physical placement. Adulthood and a coarse reproductive window are biological constraints: the window ends at two thirds of the body’s expressed natural lifespan, so inherited longevity changes reproductive aging. Partnership and desire remain separate from fertility. A pregnancy due during travel remains pending until the mother next settles at an area; travel cannot silently erase it. Birth on a road is not yet modeled.

Mortal death leaves possessions at their physical location; it does not settle estate claims.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-physiology](../game-physiology/README.md), [game-transport](../game-transport/README.md), [game-cargo](../game-cargo/README.md), [game-genetics](../game-genetics/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-lifecycle`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
