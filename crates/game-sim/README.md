# game-sim

One generic transactional host: typed resources, due requests, stable event order, command deduplication, bounded advancement, and validated saves. No feature dependencies or alternate kernels.

**Dependencies:** [game-core](../game-core/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-sim`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
