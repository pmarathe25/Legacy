# game-skills

Diminishing skill practice and unique work attribution. Attribution IDs use sparse offsets or dense bitmap pages, preserving exact membership and the legacy ordered map save format without retaining one tree row per completed interval. Past-life mastery stays outside personal learned levels.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-skills`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.
