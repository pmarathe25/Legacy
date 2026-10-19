# game-logistics

`game-logistics` executes multi-hop physical cargo deliveries (`Haul`) by loading goods into a carried pack, traversing roads, and unloading at the destination.

**Why it matters:** Moving supplies between areas requires a real carrier who loads goods locally, walks each road segment, and unloads upon arrival—preserving custody if delayed or interrupted.

## Responsibilities

- **Active deliveries (`Logistics`):** Tracks in-flight `Haul` jobs, route progress, and retry state (`logistics.deliveries`).
- **Load–travel–unload workflow (`Haul`):** Checks source permission via `game-property`, transfers goods into the carrier's pack via `game-cargo`, routes across `game-land` via `game-spatial`, and unloads into the destination inventory on arrival.
- **Interruption & retry handling:** Leaves loaded goods in the carrier's pack if cancelled or blocked mid-journey.

## Boundaries

- **Execution vs. primitives:** Composes [`game-cargo`](../game-cargo/README.md), [`game-property`](../game-property/README.md), [`game-spatial`](../game-spatial/README.md), and [`game-transport`](../game-transport/README.md) without owning inventory balances or road positions directly.

## How It Works

- **Arrival-driven progression:** Subscribes to `game_transport::Arrived` to advance the carrier along the next hop of the shortest path or perform the final unload transfer at the destination area.
- **Capacity reservation:** Reserves destination space or retains goods in the carried pack if the destination cannot accept the delivery.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`logistics`, resource `logistics.deliveries`).
- **Requests & Events:** Handles `Haul`; subscribes to `game_transport::Arrived`; emits `Delivered` and `Blocked`.
- **Callers:** Bound to `Command::Haul` in [`game-assembly`](../game-assembly/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-storage`](../game-storage/README.md), [`game-sim`](../game-sim/README.md), [`game-land`](../game-land/README.md), [`game-spatial`](../game-spatial/README.md), [`game-transport`](../game-transport/README.md), [`game-cargo`](../game-cargo/README.md), [`game-property`](../game-property/README.md), [`game-physiology`](../game-physiology/README.md).
- **Testing:** `cargo test -p game-logistics` verifies multi-road hauling, capacity enforcement, mid-route cancellation, and blocked-destination handling.
