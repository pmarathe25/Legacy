# game-worldgen

`game-worldgen` deterministically generates physical founding conditions—natural areas, biome deposits, and independent founders—from a seed and TOML settings.

**Why it matters:** New campaigns start from a physical world with no pre-built institutions, social bonds, or shared vocabulary, letting social arrangements and livelihoods emerge through actual simulation history.

## Responsibilities

- **Worldgen schema (`Settings`):** Reads `content/worldgen/default.toml` (population size, guardian count, area count, history warmup duration, biome templates, trait/belief ranges, and initial carried goods).
- **Physical world synthesis (`generate`):** Builds a validated `game_scenarios::Definition` with areas linked in a seeded spanning tree, biome-specific climates, wild food/material patches, and fauna populations.
- **Founder generation:** Samples each mortal's traits across all authored catalog ranges, initial social beliefs, phoneme-derived personal name, carried container, and personal food supply.

## Boundaries

- **Pure scenario generator:** Produces a `game_scenarios::Definition` data structure without running a second simulation loop or inventing fake historical prose; [`game-openings`](../game-openings/README.md) installs the generated definition and advances the real `Host`.
- **Zero prebuilt society:** Emits no institutions, offices, agreements, buildings, cultivated plots, or directed social bonds.

## How It Works

- **Population-scaled abundance:** Scales each area's primary food, material, and extra biome resource stocks by the local founding population estimate (`food_stock_per_person`, `food_renew_per_person`).
- **Deterministic RNG streams:** Derives area biomes, road durations, founder traits, and names from the world seed via `game-core` counter-based randomness.

## Game Integration

- **Plugin / Entry Point:** Pure library crate (`Settings`, `load`, `generate`); registers no `game-sim` plugin.
- **Requests & Events:** None.
- **Callers:** Invoked by [`game-openings`](../game-openings/README.md) when initializing the `generated` scenario or `seed_world`.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-content`](../game-content/README.md), [`game-scenarios`](../game-scenarios/README.md), [`game-physiology`](../game-physiology/README.md), [`game-relationships`](../game-relationships/README.md), [`game-traits`](../game-traits/README.md), [`game-communication`](../game-communication/README.md).
- **Testing:** `cargo test -p game-worldgen` verifies seed determinism, biome resource scaling, trait bounds, and the absence of preconstructed institutions or bonds.
