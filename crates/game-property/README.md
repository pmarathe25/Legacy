# game-property

`game-property` manages ownership claims, claim recognition, revocable per-person access permissions, and authorized or unauthorized taking of goods and plots.

**Why it matters:** Property in Legacy is a social relationship rather than a physical lock—people can hold competing claims, grant or revoke access to stores, and physically take contested goods at the risk of witnesses and grievances.

## Responsibilities

- **Claims & permissions (`Property`):** Stores competing ownership claims on inventories and plots, recognition sets, and per-inventory access lists (`property.state`), backed by a dense `game-packed` reverse index per person.
- **Physical taking (`Take`):** Transfers goods via `game-cargo`, checking whether the actor has standing permission or is explicitly acting `unauthorized`.
- **Contested use (`Use`):** Records whether an action on an inventory or plot defied a recognized claim and identifies awake, living local witnesses via `game-physiology`.
- **Conscience cost (`reluctance`):** Computes a person's internal reluctance to take or use contested property from honesty, independence, claim recognition, fear, and grievance.

## Boundaries

- **Social claims vs. physical movement:** Tracks who claims and permits access; [`game-cargo`](../game-cargo/README.md) enforces physical quantities and [`game-grievances`](../game-grievances/README.md) / [`game-justice`](../game-justice/README.md) handle reactions to witnessed interference.

## How It Works

- **Dual permission indexing:** Maintains both forward (`InventoryId -> permitted people`) snapshots and a journaled reverse (`PersonId -> accessible inventories`) index rebuilt on load for fast autonomous foraging lookups.
- **Witnessed interference:** When `Take { unauthorized: true }` or contested `Use` executes, `game-property` does not block the physical action; instead it emits `Taken` / `Interference` containing only co-located awake witnesses.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`property`, resource `property.state`).
- **Requests & Events:** Handles `ClaimInventory`, `ClaimPlot`, `Recognize`, `Permit`, `Revoke`, `Take`, `Use`; emits `Taken` and `Interference`.
- **Callers:** Bound to `Command::Take` in `game-assembly` and queried by `game-provisioning`, `game-horticulture`, `game-livelihoods`, `game-institutions`, and `game-grievances`.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-storage`](../game-storage/README.md), [`game-packed`](../game-packed/README.md), [`game-sim`](../game-sim/README.md), [`game-cargo`](../game-cargo/README.md), [`game-transport`](../game-transport/README.md), [`game-traits`](../game-traits/README.md), [`game-vitality`](../game-vitality/README.md), [`game-relationships`](../game-relationships/README.md).
- **Testing:** `cargo test -p game-property` verifies competing claims, permission revocation, authorized vs. unauthorized taking, witness filtering, and rollback.
