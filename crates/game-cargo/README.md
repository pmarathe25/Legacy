# game-cargo

`game-cargo` owns physical inventories, mass capacity accounting, input/output reservations, carried container custody, and atomic local transfers.

**Why it matters:** Every item in the simulation must reside in a concrete inventory with conserved quantities and finite mass capacity, preventing goods from being duplicated, lost, or teleported across areas.

## Responsibilities

- **Inventories resource (`Cargoes`):** Tracks each `InventoryId`'s capacity, current `game-storage::Table<GoodId, u64>` balances, active reservations, and cached used/reserved mass (`cargo.inventories`).
- **Atomic transfers (`Transfer`):** Moves goods between two co-located inventories, verifying available unreserved stock and destination mass capacity.
- **Input & space reservations:** Supports reserving input goods (`ReserveGood`) or output capacity (`ReserveSpace`) during timed actions like gathering, harvesting, and crafting.
- **Carried container index:** Maintains a derived per-person index of carried `InventoryId`s so dropping or inspecting carried packs never scans all world inventories.

## Boundaries

- **Physical custody vs. legal ownership:** Enforces co-location, quantity conservation, and mass capacity; [`game-property`](../game-property/README.md) owns ownership claims and access permissions.

## How It Works

Experimental read-only stock planning is provided by [`game-cargo-planning`](../game-cargo-planning/README.md); cargo requests remain authoritative.

Each `cargo.changed` notification includes the affected inventory's exact stock
before and after the operation, and transfers include the destination's balances.
These snapshots describe that operation inside its enclosing transaction; an
outer rollback can still undo it. Listeners still run in order, and an earlier
reentrant listener can change current stock after the snapshot. The snapshots do
not yet permit multiple stock mutations to be committed before notification.

- **Cached mass accounting:** Each inventory caches its total occupied and reserved mass so `used()` and capacity checks run in $O(1)$ time, while full save validation recomputes and verifies the sum.
- **Co-location enforcement:** Transfers and pickups query [`game-transport`](../game-transport/README.md) to confirm both endpoints are in the same area.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`cargo`, resource `cargo.inventories`).
- **Requests & Events:** Handles `Create`, `Mint`, `Consume`, `Transfer`, `Pickup`, `Drop`, `ReserveGood`, `ReleaseGood`, `ReserveSpace`, `ReleaseSpace`, `FulfillSpace`; emits `Transferred`, `Consumed`, `Minted`.
- **Callers:** Used by `game-property`, `game-ecology`, `game-agriculture`, `game-workshop`, `game-physiology`, `game-logistics`, `game-items`, and `game-preservation`.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-storage`](../game-storage/README.md), [`game-objects`](../game-objects/README.md), [`game-sim`](../game-sim/README.md), [`game-transport`](../game-transport/README.md).
- **Testing:** `cargo test -p game-cargo` verifies atomic transfers, capacity overflow rejection, input/output reservations, carrier index updates, and save validation.
