# game-agriculture

`game-agriculture` owns cultivated plots, seed-consuming sowing labor, climate-dependent crop growth, and timed harvesting.

**Why it matters:** Farming requires physical seed stock, uninterrupted sowing and harvesting labor, and suitable seasonal weather, turning raw land and saved seed into harvested food lots.

## Responsibilities

- **Plots & crop catalog (`Agriculture`, `Crops`):** Stores plot locations, crop state (`Fallow`, `Sowing`, `Growing`, `Ripe`), [`game-crop-cohorts`](../game-crop-cohorts/README.md) strain mixtures (`agriculture.plots`), and installed [`game-crops`](../game-crops/README.md) definitions (`agriculture.crops`).
- **Timed sowing (`Sow`):** Consumes seed from an accessible local inventory (`game-cargo`), records genetic seed lots (`game-cultivars`), occupies the worker's canonical action slot, and transitions the plot to `Growing` on completion.
- **Growth & harvesting (`Harvest`):** Integrates seasonal warmth and rain (`game-environment`) across growth, ripens surviving crops (or resets perennial `regrowth` crops after harvest), and mints harvested output into the destination inventory after timed harvest labor.

## Boundaries

- **Physical plot execution vs. planting decisions:** Owns plot states, labor timers, and harvest outputs; [`game-horticulture`](../game-horticulture/README.md) decides when autonomous characters establish or tend plots.

## How It Works

- **Asymmetric interruption consequences:** Cancelling `Sow` mid-action loses the consumed seed and leaves the plot fallow; cancelling `Harvest` mid-action aborts the labor while leaving the ripe crop standing in the plot.
- **Lethal weather failure:** Exposure to temperatures or drought beyond the planted cohort's tolerances kills the crop and returns the plot to `Fallow`.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`agriculture`, resources `agriculture.plots` and `agriculture.crops`) and re-exports `game_crops::load`.
- **Requests & Events:** Handles `InstallCrops`, `CreatePlot`, `Sow`, `Harvest`, and growth tick callbacks; emits `Sown` and `Harvested`.
- **Callers:** Bound to `Command::Sow` and `Command::Harvest` in `game-assembly` and called by [`game-horticulture`](../game-horticulture/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-storage`](../game-storage/README.md), [`game-content`](../game-content/README.md), [`game-objects`](../game-objects/README.md), [`game-crops`](../game-crops/README.md), [`game-crop-cohorts`](../game-crop-cohorts/README.md), [`game-cultivars`](../game-cultivars/README.md), [`game-sim`](../game-sim/README.md), [`game-transport`](../game-transport/README.md), [`game-cargo`](../game-cargo/README.md), [`game-property`](../game-property/README.md), [`game-physiology`](../game-physiology/README.md), [`game-environment`](../game-environment/README.md), [`game-skills`](../game-skills/README.md).
- **Testing:** `cargo test -p game-agriculture` verifies sowing/harvesting labor, interruption consequences, climate failure, perennial regrowth, and cultivar yield propagation.
