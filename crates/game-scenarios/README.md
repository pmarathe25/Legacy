# game-scenarios

`game-scenarios` defines the declarative TOML schema, symbolic reference validation, population group expansion, and catalog discovery for starting scenarios.

**Why it matters:** Scenarios need a self-contained data representation—referencing areas, people, inventories, plots, and bonds by symbolic string keys—that can be validated before any simulation host is constructed.

## Responsibilities

- **Scenario schema (`Definition`):** Models authored areas, roads, population groups (`PersonGroup`), inventories, permissions, resource patches, agricultural plots, agreements, institutions,directed social bonds, battles, and rule profile keys.
- **Population expansion (`actors`):** Expands person groups with `count > 1` into individual symbolic keys (e.g., `citizen-1`, `citizen-2`).
- **Structural validation:** Verifies non-empty keys, unique area/person/inventory identifiers, valid bidirectional road endpoints, permission selectors (`"*"`, group key, or actor key), and relationship bounds.
- **Catalog discovery:** Reads `manifest.toml` for the default scenario and lists available scenarios (separating canonical starts from `fixture = true` developer starts).

## Boundaries

- **Data schema only:** Has no dependency on `game-sim` or feature execution; [`game-scenario-assets`](../game-scenario-assets/README.md) resolves cross-catalog references and [`game-openings`](../game-openings/README.md) installs the scenario into a live `Host`.

## How It Works

- **Symbolic cross-referencing:** All entities inside a scenario TOML refer to each other by human-readable strings (`"home"`, `"ruler"`, `"store"`), which `game-openings` later maps to numeric runtime IDs.
- **Seeded random selection:** Supports `--scenario random` by filtering out `fixture = true` definitions and picking deterministically from the seed.

## Game Integration

- **Plugin / Entry Point:** Pure library crate (`Definition`, `Scenario`, `load`, `scenarios`, `default_scenario`); registers no `game-sim` plugin.
- **Requests & Events:** None.
- **Callers:** Produced by [`game-worldgen`](../game-worldgen/README.md) or loaded from TOML by [`game-openings`](../game-openings/README.md) and validated alongside [`game-scenario-assets`](../game-scenario-assets/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-content`](../game-content/README.md), [`game-physiology`](../game-physiology/README.md), [`game-relationships`](../game-relationships/README.md).
- **Testing:** `cargo test -p game-scenarios` checks group expansion, symbolic reference validation, duplicate detection, and manifest discovery.
