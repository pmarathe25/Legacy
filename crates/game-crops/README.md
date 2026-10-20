# game-crops

`game-crops` owns the authored crop catalog (`content/agriculture/*.toml`), resolving seed/harvest objects, growth durations, yields, and seasonal climate tolerances.

**Why it matters:** Separating crop definitions and climate suitability rules from field labor (`game-agriculture`) allows both cultivated plots and wild botanical stands (`game-botany`) to share one set of plant biology definitions.

## Responsibilities

- **Crop catalog (`Crops`, `CropDef`):** Stores each crop's seed `GoodId`, harvested output `GoodId`, sowing/harvesting labor durations, fixture vs. natural campaign growth durations and yields, minimum/lethal warmth and rain thresholds, regrowth flags, and natural lifespan.
- **Climate evaluation:** Provides helpers to check whether an area's current `game-environment` season is suitable, dormant, or lethal for a crop.

## Boundaries

- **Definitions & rules only:** Owns crop schemas and climate checks; [`game-agriculture`](../game-agriculture/README.md) owns cultivated plots and labor, [`game-cultivars`](../game-cultivars/README.md) owns diploid plant strains, and [`game-crop-cohorts`](../game-crop-cohorts/README.md) computes mixed-strain harvest math.

## How It Works

- **Dual timing profiles:** Supports fast fixture growth (`growth_duration`, `yield_quantity`) for short unit tests alongside full seasonal campaign profiles (`natural_growth`, `natural_yield`) when campaign mode is active.

## Game Integration

- **Plugin / Entry Point:** Library crate (`Crops`, `CropDef`, `load`); installed into `game-agriculture`'s resource via `InstallCrops`.
- **Requests & Events:** Re-exported through [`game-agriculture`](../game-agriculture/README.md).
- **Callers:** Loaded by [`game-scenario-assets`](../game-scenario-assets/README.md) and used by `game-agriculture`, `game-crop-cohorts`, `game-botany`, and `game-horticulture`.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-content`](../game-content/README.md), [`game-objects`](../game-objects/README.md), [`game-environment`](../game-environment/README.md), [`game-cultivars`](../game-cultivars/README.md).
- **Testing:** Exercised by `cargo test -p game-agriculture` and `cargo test -p game-botany`.

Crop definitions use shared records. Proposal preparation can retain an immutable
catalog revision while request hooks run, without copying every crop name. Saved
TOML/JSON definitions and per-strain expression remain unchanged.
