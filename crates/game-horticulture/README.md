# game-horticulture

`game-horticulture` drives voluntary plot establishment, sowing, and harvesting decisions for characters and exposes the atomic `Plant` action.

**Why it matters:** Generated worlds start with zero cultivated plots; patient characters who acquire seeds in a suitable climate can establish personal gardens, harvest ripe crops, and resow when local stores run low.

## Responsibilities

- **Personal gardens (`Gardens`):** Tracks each person's established `PlotId` stewardship and review cooldowns (`horticulture.gardens`).
- **Atomic planting (`Plant`):** Allocates a new fallow plot in the actor's area, claims it (`game-property`), and immediately begins `game_agriculture::Sow` in one transaction—leaving no empty plot behind if sowing fails.
- **Autonomous tending (`Consider`):** Proposes harvesting ripe plots (offloading carried surplus via [`game-stockpiles`](../game-stockpiles/README.md) if needed), resowing fallow plots when accessible local stocks hold less than one harvest of that crop, or establishing a new plot from carried seeds.

## Boundaries

- **Decision & stewardship layer:** Decides when to plant, resow, or harvest; [`game-agriculture`](../game-agriculture/README.md) owns actual plot state, crop growth, and labor timers.

## How It Works

- **Stock-aware resowing:** Cultivators pause resowing a crop while accessible local inventories already hold at least one full harvest of that output, avoiding wasted seed and labor.
- **Climate pre-check:** Characters only plant or resow a crop when current local `game-environment` warmth and rain satisfy the crop's growth requirements.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`horticulture`, resource `horticulture.gardens`).
- **Requests & Events:** Handles `Enable`, `Plant`, and `Consider`; submits proposals via [`game-choice`](../game-choice/README.md).
- **Callers:** Bound to `Command::Plant` (`plant <crop> <inventory>`) in `game-assembly` and enabled in generated worlds by [`game-campaign`](../game-campaign/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-storage`](../game-storage/README.md), [`game-choice`](../game-choice/README.md), [`game-diagnostics`](../game-diagnostics/README.md), [`game-vitality`](../game-vitality/README.md), [`game-stockpiles`](../game-stockpiles/README.md), [`game-agriculture`](../game-agriculture/README.md), [`game-environment`](../game-environment/README.md), [`game-cargo`](../game-cargo/README.md), [`game-objects`](../game-objects/README.md), [`game-property`](../game-property/README.md), [`game-transport`](../game-transport/README.md), [`game-physiology`](../game-physiology/README.md), [`game-traits`](../game-traits/README.md), [`game-characters`](../game-characters/README.md), [`game-lineage`](../game-lineage/README.md).
- **Testing:** `cargo test -p game-horticulture` verifies atomic `Plant` rollback, climate/stock gating, surplus caching before harvest, and save/load state.
