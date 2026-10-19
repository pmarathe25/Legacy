# game-provisioning

`game-provisioning` evaluates a character's remembered food stores, wild patches, containers, and routes to construct candidate meal and foraging plans.

**Why it matters:** Hungry characters must turn incomplete personal memories—such as a distant berry patch, a carried basket, and a known road—into concrete, multi-step survival plans without consulting omniscient world state.

## Responsibilities

- **Candidate generation (`options`):** Queries [`game-food-memory`](../game-food-memory/README.md) and remembered patch/road facts (`game-knowledge`) to propose ready meals (`Meal`), gathering trips (`Forage`), reserve stockpiling, or frontier searches when all known food sources fail.
- **Specific food targeting:** Identifies the exact `GoodId` to eat or gather so dietary deficiencies (`game-nutrition`) and unfamiliar-food trials are honored on execution.
- **Step & failure facade (`step`, `acquire`, `learn_failure`):** Delegates physical execution stages to [`game-food-actions`](../game-food-actions/README.md) and refreshes local observations when a remembered store or patch turns out to be empty or blocked.

## Boundaries

- **Stateless planning facade:** Creates serializable `Plan` values stored in the caller (`game-characters` or `game-households`) and owns no independent action clock or inventory ledger.

## How It Works

- **Query-local route & appraisal caching:** Reuses shortest-path lookups (`game-knowledge`) and per-food dietary appraisals across all stores in an area within a single immutable planning call, discarding the temporary cache before any mutation.
- **Frontier fallback:** Only when no remembered edible stock or usable patch exists does `options` propose walking a remembered road toward an unobserved frontier area.

## Game Integration

- **Plugin / Entry Point:** Pure planning library (`Plan`, `Opportunity`, `options`, `step`, `acquire`, `learn_failure`); registers no `game-sim` plugin.
- **Requests & Events:** Dispatches physical actions via [`game-food-actions`](../game-food-actions/README.md).
- **Callers:** Used by [`game-characters`](../game-characters/README.md) for self-feeding and by [`game-nurture`](../game-nurture/README.md) / [`game-households`](../game-households/README.md) for dependent care.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-storage`](../game-storage/README.md), [`game-utility`](../game-utility/README.md), [`game-food-memory`](../game-food-memory/README.md), [`game-food-actions`](../game-food-actions/README.md), [`game-vitality`](../game-vitality/README.md), [`game-relationships`](../game-relationships/README.md), [`game-traits`](../game-traits/README.md), [`game-property`](../game-property/README.md), [`game-stockpiles`](../game-stockpiles/README.md), [`game-objects`](../game-objects/README.md), [`game-knowledge`](../game-knowledge/README.md), [`game-land`](../game-land/README.md), [`game-cargo`](../game-cargo/README.md), [`game-transport`](../game-transport/README.md), [`game-physiology`](../game-physiology/README.md), [`game-ecology`](../game-ecology/README.md), [`game-diagnostics`](../game-diagnostics/README.md).
- **Testing:** `cargo test -p game-provisioning` verifies multi-hop meal/foraging plans, stale-stock recovery, dietary preference weighting, and frontier fallback.
