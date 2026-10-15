# game-lineage

Persistent life records, mortal ruler death, indefinite succession gaps, and selection from actual post-death births. First-birth selection is an explicit fixture policy; it never creates a child.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-physiology](../game-physiology/README.md), [game-lifecycle](../game-lifecycle/README.md), [game-skills](../game-skills/README.md), [game-knowledge](../game-knowledge/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-lineage`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
