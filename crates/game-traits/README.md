# game-traits

Authored trait names, bounds, defaults, and utility weights, plus individual profiles. New traits can affect existing decision terms through data; no trait-specific branches are required.

Run `cargo test -p game-traits` from the repository root.

**Content:** [Store format and extension guide](../../content/README.md). Definitions are read from files at startup and resolved snapshots survive save/load. The schema and validation live in this crate.

Authored profile effects compile to five numeric weights when a profile changes. Decision reads copy those weights; loading rebuilds the derived cache from definitions and profiles. The cache is excluded from saves and checked against its source data.
