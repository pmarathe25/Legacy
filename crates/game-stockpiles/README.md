# game-stockpiles

`game-stockpiles` lets characters offload surplus carried goods into finite, personally owned ground caches in their current area.

**Why it matters:** Autonomous foragers and cultivators have limited carried pack capacity; creating a local ground cache frees room for harvests or raw materials while keeping every item physical, local, and subject to property rules.

## Responsibilities

- **Cache index (`Stockpiles`):** Tracks each person's area-bound ground cache `InventoryId` (at most one per person per area) and cached stored mass (`stockpiles.caches`).
- **Surplus offloading (`StoreSurplus`):** Transfers heavy non-food goods (and surplus food beyond an 8-meal personal reserve per edible type) from a carried pack into the actor's local ground cache to free `needed_mass`.

## Boundaries

- **Composer over cargo & property:** Delegates physical container creation and transfers to [`game-cargo`](../game-cargo/README.md), ownership claims to [`game-property`](../game-property/README.md), and inventory memory updates to [`game-knowledge`](../game-knowledge/README.md).
- **Finite capacity:** Each ground cache holds at most $4\times$ the source carried container's capacity; it never destroys goods or spawns unlimited caches when full.

## How It Works

- **Priority eviction:** Sorts carried items to offload non-edible materials with the largest total mass first, preserving essential carried rations.
- **Atomic rollback:** If the cache is full or lacks enough movable surplus to free `needed_mass`, the entire `StoreSurplus` request rolls back cleanly.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`stockpiles`, resource `stockpiles.caches`).
- **Requests & Events:** Handles `StoreSurplus`; emits `Stored`.
- **Callers:** Invoked by [`game-provisioning`](../game-provisioning/README.md), [`game-food-actions`](../game-food-actions/README.md), and [`game-horticulture`](../game-horticulture/README.md) when carried containers fill up.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-storage`](../game-storage/README.md), [`game-cargo`](../game-cargo/README.md), [`game-property`](../game-property/README.md), [`game-transport`](../game-transport/README.md), [`game-physiology`](../game-physiology/README.md), [`game-objects`](../game-objects/README.md), [`game-knowledge`](../game-knowledge/README.md).
- **Testing:** `cargo test -p game-stockpiles` verifies meal retention, capacity limits, ownership claims, knowledge updates, and rollback on full caches.
