# game-ecology

`game-ecology` owns gatherable wild resource patches, finite stock reservations, climate-gated daily renewal, and timed gathering labor.

**Why it matters:** Raw materials and wild foods come from finite local patches that must be physically gathered over time, reserved against concurrent gatherers, and renewed only when seasonal weather permits.

## Responsibilities

- **Resource patches (`Ecology`):** Tracks each patch's area, `GoodId`, current stock, capacity, active reservations, daily renewal rate, and optional warmth/rain renewal thresholds (`ecology.patches`).
- **Timed gathering (`Gather`):** Reserves patch stock and destination container space (`game-cargo`), occupies the actor's canonical action slot, mints the gathered goods on completion, and credits gathering skill (`game-skills`).
- **Seasonal renewal:** Runs a scheduled daily renewal pass that replenishes renewable patches up to capacity when local `game-environment` warmth and rain meet authored thresholds.

## Boundaries

- **Bulk patch stock vs. living stands:** Manages patch stock quantities, reservations, and timed gathering; [`game-botany`](../game-botany/README.md) layers representative diploid plant stands and seed banks over edible patches when enabled.

## How It Works

- **Dual reservation safety:** Starting `Gather` reserves both the patch's remaining stock and the destination inventory's mass capacity; cancelling the action releases both reservations without minting goods.
- **Finite deposits:** Patches with `renew = 0` never replenish, modeling exhaustible mineral and stone veins.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`ecology`, resource `ecology.patches`).
- **Requests & Events:** Handles `CreatePatch`, `SetRenewal`, `Gather`, and internal completion/renewal callbacks; emits `Gathered`.
- **Callers:** Bound to `Command::Gather` in `game-assembly`, and called by [`game-food-actions`](../game-food-actions/README.md), [`game-livelihoods`](../game-livelihoods/README.md), and [`game-botany`](../game-botany/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-storage`](../game-storage/README.md), [`game-sim`](../game-sim/README.md), [`game-objects`](../game-objects/README.md), [`game-cultivars`](../game-cultivars/README.md), [`game-environment`](../game-environment/README.md), [`game-cargo`](../game-cargo/README.md), [`game-transport`](../game-transport/README.md), [`game-physiology`](../game-physiology/README.md), [`game-skills`](../game-skills/README.md).
- **Testing:** `cargo test -p game-ecology` verifies stock/space reservations, cancellation rollback, climate-gated renewal, skill practice, and save validation.
