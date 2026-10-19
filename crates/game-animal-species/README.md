# game-animal-species

`game-animal-species` defines and validates the runtime catalog of animal species from `content/fauna/*.toml`.

**Why it matters:** Separating static species definitions and reference validation from live animal state (`game-fauna`) keeps species data modular and pre-validated before world creation.

## Responsibilities

- **Species catalog (`Species`, `AnimalDef`):** Stores each species' symbolic key, body mass, speed, development time, natural lifespan, gestation duration, daily forage intake, warmth/rain tolerance, and meat/product object references.
- **Catalog validation (`load`):** Resolves symbolic food and output keys against `game-objects::Objects` and rejects non-positive durations, masses, or invalid temperature bands.

## Boundaries

- **Static definitions only:** Owns no `game-sim` plugin, living animal entities, or character taming decisions; [`game-fauna`](../game-fauna/README.md) owns live animals and [`game-attachments`](../game-attachments/README.md) owns human–animal care motives.

## How It Works

- **Startup resolution:** `load(store, objects)` parses all documents under `content/fauna/` and binds symbolic object names to numeric `GoodId`s before simulation starts.

## Game Integration

- **Plugin / Entry Point:** Pure catalog library (`Species`, `AnimalDef`, `load`); registers no `game-sim` plugin.
- **Requests & Events:** None.
- **Callers:** Loaded by [`game-scenario-assets`](../game-scenario-assets/README.md) and consumed by [`game-fauna`](../game-fauna/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-content`](../game-content/README.md), [`game-objects`](../game-objects/README.md).
- **Testing:** Exercised by `cargo test -p game-fauna` and `cargo test -p game-assembly --test content`.
