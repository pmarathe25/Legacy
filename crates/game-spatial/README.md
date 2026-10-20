# game-spatial

`game-spatial` is a pure library for deterministic shortest-path routing across `game-land` road networks.

**Why it matters:** Characters, haulers, guardians, and expeditions need repeatable multi-hop pathfinding with deterministic tie-breaking that can be evaluated over either authoritative maps or subjective road memories.

## Responsibilities

- **Shortest-path search (`shortest_path`):** Computes the minimum-duration `Route` (sequence of `RoadId`s, next hop `AreaId`, and total `SimDuration`) between two areas.
- **Deterministic tie-breaking:** Resolves equal-duration path alternatives using stable `AreaId` ordering.

## Boundaries

- **Stateless algorithm:** Owns no resources or `game-sim` plugin; [`game-land`](../game-land/README.md) owns the authoritative road graph and [`game-knowledge`](../game-knowledge/README.md) builds personal subgraphs from remembered roads.

## How It Works

- **Dijkstra search:** Explores reachable areas ordered by `(total_duration, area_id)` so identical topologies always yield the exact same route across platforms and threads.
- **Zero-hop & unreachable handling:** Returns an empty route with zero duration when start equals destination, and `None` when no connecting path exists.

## Game Integration

- **Plugin / Entry Point:** Pure library function (`shortest_path`, `Route`); registers no `game-sim` plugin.
- **Requests & Events:** None.
- **Callers:** Used by [`game-knowledge`](../game-knowledge/README.md), [`game-logistics`](../game-logistics/README.md), [`game-missions`](../game-missions/README.md), and [`game-guardians`](../game-guardians/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-land`](../game-land/README.md).
- **Testing:** `cargo test -p game-spatial` verifies multi-hop shortest paths, equal-cost tie-breaking, and disconnected graph handling without a simulation host.

`Routes` bounds derived path reuse to 2,048 entries. Callers supply weak namespace tokens that change whenever perceived roads change; cached failures invalidate too. The cache retains no personal history or world snapshot. Physical travel rates are applied separately to the unchanged base route.

Cached route paths use shared contiguous area slices, so repeated path reads
reuse numeric data instead of copying the entire path.
