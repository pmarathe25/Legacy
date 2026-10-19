# game-land

`game-land` owns the world's named areas and positive-duration bidirectional roads.

**Why it matters:** Every spatial mechanic—movement, route planning, local observations, and regional climate—needs an authoritative topology of places and travel durations.

## Responsibilities

- **World topology (`Land`):** Stores named `Area` records and bidirectional `Road` links (`land.topology`) using `game-storage` tables.
- **Topology construction:** Validates and allocates stable `AreaId` and `RoadId` identifiers via `CreateArea` and `CreateRoad`.
- **Adjacency queries:** Exposes connected road lookups and travel durations between areas.

## Boundaries

- **Topology only:** Owns areas and road durations; [`game-spatial`](../game-spatial/README.md) computes shortest paths and [`game-transport`](../game-transport/README.md) tracks entity positions and timed movement.

## How It Works

- **Validated links:** `CreateRoad` requires both endpoint areas to exist, forbids self-loops, and enforces strictly positive `SimDuration` travel times.
- **Bidirectional traversal:** Roads connect `a` and `b` symmetrically so either endpoint can initiate travel along the link.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`land`, resource `land.topology`).
- **Requests & Events:** Handles `CreateArea` and `CreateRoad`.
- **Callers:** Populated by [`game-openings`](../game-openings/README.md) at startup and queried by [`game-spatial`](../game-spatial/README.md), [`game-transport`](../game-transport/README.md), and [`game-knowledge`](../game-knowledge/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-storage`](../game-storage/README.md), [`game-sim`](../game-sim/README.md).
- **Testing:** `cargo test -p game-land` verifies area creation, bidirectional road validation, invalid endpoint rejection, and save/load consistency.
