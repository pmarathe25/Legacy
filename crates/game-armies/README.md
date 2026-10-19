# game-armies

Military unit recruitment, roster membership, real ration consumption, and expedition deployment.

**Why it matters:** Armies in Legacy are groups of real people who must be recruited, fed from physical stockpiles, and marched across the land—not abstract troop counters spawned from a global manpower pool.

## Responsibilities

- **Unit Recruitment & Rosters:** Manages individual recruitment into military units and tracks unit membership.
- **Real Food Upkeep:** Consumes actual food supplies (`game-cargo`, `game-property`) through member physiology (`game-physiology`) rather than abstract upkeep numbers.
- **Expedition Deployment:** Deploys assembled units on strategic marches and operations by composing `game-missions` party and travel mechanics.

## Boundaries

- **No Separate Travel Engine:** Delegates multi-person route marching, arrival, and return logistics to `game-missions` and `game-transport`.
- **No Free Rations:** Unfed soldiers suffer normal physiological hunger and fatigue via `game-physiology`.

## How It Works

- **Mission-Backed Operations:** When an army unit deploys, `game-armies` forms or updates a corresponding `game-missions` party, reserves member action slots, and draws rations from permitted unit stores.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers army unit tables, recruitment handlers, and deployment logic in `game-assembly`.
- **Requests & Events:** Exposes unit creation, soldier recruitment, provisioning, and mission deployment requests.
- **Callers:** Used by `game-realm`, `game-api`, `game-cli`, and military scenario fixtures.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-property`, `game-transport`, `game-missions`, `game-cargo`, `game-physiology`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-armies`) cover individual recruitment, ration consumption, mission deployment, and member casualty handling.
