# game-assembly

`game-assembly` connects domain crates, registers shared component schemas, builds playable openings, and binds player commands and knowledge-limited views to the headless simulation. Cross-feature effects pass through typed owner interfaces.

The live simulation uses `game-world::GenericHost<AssemblyState, EventKind, Components>`. The host owns the single Store, clock, seed, checked identity counters, and typed event ledger. `AssemblyState` contains feature metadata and application coordination data. `canonical_state()` borrows that metadata and store; owned copies are requested explicitly with `canonical_snapshot()` for save or inspection. Save version 39 wraps the generic host snapshot and restores through registered schemas and contextual validation before recreating query bindings.

This crate temporarily retains domain handlers and entity maps from the earlier coordinator. They are being extracted into their owners; its final responsibility is registration, factual adapters, and cross-feature transaction ordering. `game-sim` supplies scheduling and execution, and `game-session` hosts the public engine port.
