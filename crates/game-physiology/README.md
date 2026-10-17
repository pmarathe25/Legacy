# game-physiology

Composes eating, feeding, and rest over bodily state owned by [game-soma](../game-soma/README.md). Pure numeric batches use a bounded CPU pool. Rules supply baseline rates; [game-metabolism](../game-metabolism/README.md) expresses individual rates through genetics, development, aging, condition, and environment.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-transport](../game-transport/README.md), [game-cargo](../game-cargo/README.md), [game-property](../game-property/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-physiology`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.

[game-soma](../game-soma/README.md) owns body storage and live-row advancement; [game-body](../game-body/README.md) owns numeric forecasts. Fractional hunger and recovery carry across ticks and save/load. Death effects are sorted by stable person ID. Rules and ticking state share the same rollback boundary.

`ConsumeEdible` consumes a specific quantity when the actor is idle. `Serve` and `Feed` also require an idle caregiver and physical contact. Caregiver evidence selects the food; the recipient independently weighs hunger, appetite, remembered preference, dose, and current activity. Rejection preserves inventory and bodily state. These are voluntary food offers; forced administration requires a separate physical mechanic.
