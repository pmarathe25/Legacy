# game-objects

`game-objects` owns the runtime catalog of physical goods, including their stable keys, numeric `GoodId`s, unit masses, tags, and nutritional/edible properties.

**Why it matters:** Storing goods as data-driven catalog entries instead of hardcoded Rust enums lets content packs add hundreds of foods, tools, materials, and garments without recompiling.

## Responsibilities

- **Object catalog (`Objects`):** Persists resolved `ObjectDef` records (`objects.catalog`), mapping string keys (e.g., `"bread"`) to stable `GoodId`s.
- **Physical & dietary attributes:** Stores unit `mass`, string `tags` (such as `warmth:N`, `shelf-days:N`, `clonal`, `container`), and optional `Food` properties (portion size, hunger relief, nutrients, shelf life, and physiological effects).
- **Fast ID lookup:** Maintains a derived `GoodId -> ObjectDef` index rebuilt on load so mass and food queries avoid string scans.

## Boundaries

- **Definitions vs. quantities:** Owns static object definitions and mass calculations; [`game-cargo`](../game-cargo/README.md) owns actual inventory quantities and [`game-items`](../game-items/README.md) tracks individual notable units.

## How It Works

- **Catalog merging:** `load(store)` reads all `content/objects/*.toml` files, rejects duplicate string keys or numeric IDs, and validates positive masses and food portions.
- **Checked mass arithmetic:** `Objects::mass(good, quantity)` uses checked multiplication to prevent overflow during capacity checks.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`objects`, resource `objects.catalog`) and provides `load(&mut Store)`.
- **Requests & Events:** Handles `Install`.
- **Callers:** Installed by [`game-scenario-assets`](../game-scenario-assets/README.md) and queried by `game-cargo`, `game-physiology`, `game-nutrition`, `game-workshop`, `game-equipment`, and `game-preservation`.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-content`](../game-content/README.md), [`game-sim`](../game-sim/README.md).
- **Testing:** `cargo test -p game-objects` verifies catalog loading, duplicate key/ID rejection, mass calculations, and save/load index reconstruction.
