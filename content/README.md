# Game content

TOML files hold validated definitions for existing game mechanics. Each document has a stable `id`, a positive `version`, and a `[data]` section that its receiving feature validates. Keep IDs stable when editing a definition and increase its version when publishing changes.

**Human definitions:** `human/` contains default physiology and temperament parameters, temperament-learning effects, newborn dispositions, genetic mutation and crossover probabilities, and reproductive allele interpretation. Gene loci are opaque names; their feature owners define what they mean.

**World definitions:** `environment/temperate.toml` sets the calendar and climate. `agriculture/crops.toml` defines crop rules by crop ID. `workshop/temperate.toml` defines production recipes; `workshop/fixtures.toml` is a separate accelerated catalog for fixtures and tests.

**Openings:** `openings/temperate.toml` defines playable opening IDs, settlement names and links, starting stock lots and resource deposits, and workshop bindings. Startup resolves its symbolic references to entity IDs. Population scaling is applied by the opening mechanics.

For ordinary playable startup, `game-cli --content DIRECTORY` selects a content directory with this layout. `--environment PATH.toml` selects a calendar and climate document. Files are read while constructing the world; changing a file does not require a rebuild.

Saves retain the resolved definitions and their content stamps. Loading a save does not require the original files. Tests verify that edited opening names and stock quantities, added openings and workshop bindings, and edited recipe values affect normal play and survive save/load. Agriculture tests verify that changing the authored crop yield changes output; resolved crop catalogs also round-trip.

Adding data that uses existing types is content work. Values such as `Good` and resource kinds are mechanics vocabulary defined in Rust; adding a new vocabulary value or mechanic requires code. Each settlement can define named starting customs in TOML. An opening selects one by key; `game-openings` validates the definition and resolves its symbolic residents, areas, and inventories into the existing institution, property claim, and agreement types during startup. The concrete customs in `openings/temperate.toml` are authored data; the receiving mechanics remain implemented in their feature crates.
