# game-content

`game-content` discovers, loads, overlays, caches, and fingerprints versioned TOML documents from the runtime `content/` store.

**Why it matters:** Keeping goods, crops, recipes, traits, climates, and scenarios in external TOML packs lets designers and modders extend or tune the game without recompiling Rust crates.

## Responsibilities

- **Content store (`Store`):** Resolves base content directories (`Store::standard()`, `Store::fixtures()`, or custom paths) and applies ordered `--mod` overlay directories.
- **Document envelope (`Document<T>`):** Parses and validates the standard `id`, positive `version`, and `[data]` envelope for any serde-deserializable schema `T`.
- **Catalog discovery (`catalog<T>`):** Loads all `.toml` files under a namespace directory in deterministic filename order.
- **Normalized fingerprinting:** Computes a canonical hash over loaded documents so saves can record content provenance.

## Boundaries

- **Schema-agnostic loader:** Owns file I/O, overlay precedence, and caching; individual feature crates define their own `[data]` structs, validate domain constraints, and persist resolved snapshots in their saved resources.
- **Startup-only I/O:** Reads files during world initialization; running simulations and loaded saves never touch the filesystem.

## How It Works

- **Whole-document overlays:** When `--mod` directories are supplied, a matching relative path (`<namespace>/<name>.toml`) replaces the base document, while new filenames extend the catalog.
- **Read-once cache:** Parsed raw documents are cached in memory on first access and fingerprinted deterministically regardless of OS directory listing order.

## Game Integration

- **Plugin / Entry Point:** Pure library crate (`Store`, `Document`); registers no `game-sim` plugin.
- **Requests & Events:** None.
- **Callers:** Created by [`game-assembly`](../game-assembly/README.md) and [`game-cli`](../game-cli/README.md), and passed into [`game-openings`](../game-openings/README.md) and feature loaders during initialization.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md).
- **Testing:** `cargo test -p game-content` verifies TOML envelope validation, catalog ordering, mod overlay precedence, and fingerprint stability.
