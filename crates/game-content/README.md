# game-content

Parse versioned TOML definitions and fingerprint their normalized data. Receiving features validate their own rules.

**Dependencies:** [game-core](../game-core/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-content`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.

**Content:** [Store format and extension guide](../../content/README.md). This crate provides I/O, overlays, document caching, and fingerprints. Feature crates supply schemas and validate meaning.
