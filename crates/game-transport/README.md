# game-transport

`game-transport` owns actor and inventory locations, timed road traversal, partial travel progress, cancellation, and carried movement.

**Why it matters:** All physical interactions require co-location in the same area; `game-transport` enforces that moving people, carried containers, or dependent passengers across roads takes real simulation time and respects physical carrying capacity.

## Responsibilities

- **Positions resource (`Positions`):** Tracks each person's `Location` (`Area(AreaId)` or `InTransit` on a road with progress) in dense `game-packed` rows, plus inventory locations and carrier links (`transport.positions`).
- **Timed road movement (`Move`):** Reserves the traveler's canonical action slot, schedules arrival after the road's duration, and moves carried inventories and passengers with the carrier.
- **Partial travel & cancellation:** Records exact elapsed road progress if a journey is cancelled mid-road, allowing the traveler to continue forward or turn back from that point.
- **Passenger custody:** Lets an adult carry a dependent or incapacitated person within physical load limits while preserving the passenger's own attention to rest or signal.

## Boundaries

- **Single-road execution:** Executes one road hop at a time; multi-hop route planning lives in [`game-knowledge`](../game-knowledge/README.md), [`game-logistics`](../game-logistics/README.md), or [`game-wayfaring`](../game-wayfaring/README.md).
- **Load capacity:** Evaluates carrying limits using expressed physical strength from [`game-vitality`](../game-vitality/README.md).

## How It Works

- **Dense indexed positions:** Person locations use `game-packed` row journals so co-location checks and travel updates avoid tree copies.
- **Non-teleporting cancellation:** Cancelling a carrier's movement places both carrier and passengers at the exact fractional road progress reached so far.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`transport`, resource `transport.positions`).
- **Requests & Events:** Handles `PlacePerson`, `PlaceInventory`, `Move`, `AttachInventory`, `DetachInventory`, `CarryPerson`, `SettlePassenger`; emits `Arrived` (`transport.arrived`).
- **Callers:** Bound to `Command::Move` in [`game-assembly`](../game-assembly/README.md) and queried by every local action owner to verify co-location.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-storage`](../game-storage/README.md), [`game-packed`](../game-packed/README.md), [`game-sim`](../game-sim/README.md), [`game-land`](../game-land/README.md), [`game-vitality`](../game-vitality/README.md).
- **Testing:** `cargo test -p game-transport` verifies timed road arrivals, mid-road cancellation and reversal, carried inventories, passenger progress, and save/load state.
