# game-objects

Runtime object catalogs with stable data keys and IDs, names, mass, tags, and edible properties. Loading and validation support new object types without recompilation; the installed catalog is saved with the game.

Run `cargo test -p game-objects` from the repository root.

**Content:** [Store format and extension guide](../../content/README.md). Definitions are read from files at startup and resolved snapshots survive save/load. The schema and validation live in this crate.
