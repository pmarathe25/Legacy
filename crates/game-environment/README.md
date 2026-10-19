# game-environment

`game-environment` manages the simulation calendar and resolves seasonal warmth and rainfall for each area.

**Why it matters:** Crop growth, wild plant renewal, food spoilage, and thermal exposure depend on local seasonal weather rather than static global constants.

## Responsibilities

- **Global & regional climates (`Climate`, `Regions`):** Persists the default seasonal climate cycle (`environment.climate`) and per-area climate overrides (`environment.regions`).
- **Seasonal lookup (`at`, `current`):** Computes active `Season` warmth, rain, and season name from integer `SimTime` and the area's assigned profile.
- **Content schema (`load`):** Parses periodic season definitions from `content/environment/*.toml`.

## Boundaries

- **Periodic climate table:** Models authored seasonal warmth and rain cycles rather than fluid weather dynamics; downstream owners (`game-agriculture`, `game-botany`, `game-exposure`, `game-food-storage`) interpret how warmth and rain affect their mechanics.

## How It Works

- **Modulo calendar arithmetic:** Sums season durations into a full cycle length and maps `(now + epoch_offset) % cycle_duration` to the active season entry in $O(1)$ time.
- **Regional fallback:** `at(tx, area)` checks `Regions` for an area-specific climate first and falls back to the global `Climate` resource.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`environment`, resources `environment.climate` and `environment.regions`).
- **Requests & Events:** Handles `Configure` and `ConfigureArea`.
- **Callers:** Configured by [`game-scenario-assets`](../game-scenario-assets/README.md) and [`game-openings`](../game-openings/README.md); queried by agriculture, botany, ecology, exposure, shelter, and food storage.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-storage`](../game-storage/README.md), [`game-content`](../game-content/README.md), [`game-sim`](../game-sim/README.md).
- **Testing:** `cargo test -p game-environment` checks periodic season wrap-around, regional climate overrides, and content validation.
