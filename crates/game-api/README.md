# game-api

Typed player commands and immutable observations for CLI or a future renderer. Commands do not contain an arbitrary actor field; the binding supplies the current controlled person.

**Dependencies:** [game-lineage](../game-lineage/README.md), [game-core](../game-core/README.md), [game-physiology](../game-physiology/README.md), [game-transport](../game-transport/README.md), [game-knowledge](../game-knowledge/README.md), [game-invocation](../game-invocation/README.md), [game-conflict](../game-conflict/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-api`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
