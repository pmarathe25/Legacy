# game-botany

`game-botany` models wild plant patches as representative stands of diploid plant families with varied ages, finite space, and physical seed banks.

**Why it matters:** Wild food patches are not bottomless dispensers; extreme weather or over-harvesting can deplete a stand and its seed bank to permanent local extinction, while surviving wild plants naturally evolve toward local climates and supply genetic stock for early agriculture.

## Responsibilities

- **Botanical stands (`Botany`):** Tracks per-patch plant family cohorts, standing edible produce lots, and dormant soil seed banks (`botany.state`).
- **Lifecycle & recruitment:** Advances wild plant development, fruit/seed production, age senescence, and weather mortality across seasonal ticks, recruiting surviving seeds from the seed bank into open stand spaces.
- **Genetic gathering hook:** Transfers actual [`game-cultivars`](../game-cultivars/README.md) strain lots into the gatherer's inventory when [`game-ecology::Gather`](../game-ecology/README.md) completes on a botanical patch.

## Boundaries

- **Bounded stand approximation:** Simulates representative plant family slots per patch rather than individual blades of grass; [`game-ecology`](../game-ecology/README.md) still owns the patch's gatherable stock reservation and action clock.

## How It Works

- **True extinction:** If harsh weather or human gathering exhausts both standing plants and the soil seed bank, the patch remains permanently barren—no replacement plants are spawned from nothing.
- **Standing produce decay:** Unharvested ripe produce decays over time via [`game-preservation`](../game-preservation/README.md) rules while replenishing the patch's seed bank.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`botany`, resource `botany.state`).
- **Requests & Events:** Handles `Enable`, stand seeding, and seasonal tick requests; subscribes to `game_ecology::Gathered`.
- **Callers:** Enabled by [`game-campaign`](../game-campaign/README.md) and linked with [`game-ecology`](../game-ecology/README.md) and [`game-cultivars`](../game-cultivars/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-storage`](../game-storage/README.md), [`game-cultivars`](../game-cultivars/README.md), [`game-crops`](../game-crops/README.md), [`game-ecology`](../game-ecology/README.md), [`game-environment`](../game-environment/README.md), [`game-objects`](../game-objects/README.md), [`game-preservation`](../game-preservation/README.md), [`game-diagnostics`](../game-diagnostics/README.md).
- **Testing:** `cargo test -p game-botany` verifies stand recruitment, climate selection, seed-bank exhaustion extinction, and genetic lot transfer on gathering.
