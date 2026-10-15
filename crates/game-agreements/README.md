# game-agreements

Negotiated multi-party promises, acceptance/refusal, outstanding duties, physical fulfillment, and default. Neither a signature nor a missed deadline edits inventories automatically.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-property](../game-property/README.md), [game-cargo](../game-cargo/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-agreements`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
