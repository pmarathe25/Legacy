# game-assembly

`game-assembly` connects the game's domain crates, registers shared component schemas, builds playable openings, and binds player commands and knowledge-limited views to the headless simulation. Cross-feature effects pass through their owners' typed interfaces.

This crate temporarily retains domain handlers and entity maps from the earlier coordinator. They are being extracted into their owners; the assembly's final responsibility is registration, factual adapters, and cross-feature transactions. `game-world` supplies generic host infrastructure, `game-sim` supplies scheduling and execution, and `game-session` hosts the public engine port.
