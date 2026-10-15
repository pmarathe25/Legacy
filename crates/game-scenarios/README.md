# game-scenarios

Declarative scenario schemas, stable symbolic references, population groups, validation, and runtime discovery. Has no simulation or application dependency; feature-owned behavior is invoked by game-openings.

Run `cargo test -p game-scenarios` from the repository root.

**Content:** [Store format and extension guide](../../content/README.md). Definitions are read from files at startup and resolved snapshots survive save/load. The schema and validation live in this crate.
