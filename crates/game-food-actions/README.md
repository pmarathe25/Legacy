# game-food-actions

`game-food-actions` executes the physical stages of a [`game-provisioning`](../game-provisioning/README.md) plan—traveling roads, picking up containers, caching surplus, gathering patches, taking portions, and eating.

**Why it matters:** Separating physical step execution from source evaluation (`game-provisioning`) allows both personal meals (`step`) and caregiver food errands (`acquire`) to share the exact same travel, container, and gathering state machine.

## Responsibilities

- **Self-feeding execution (`step`):** Advances a committed food plan by one physical action—moving toward a container or source (`game-transport`), attaching a container (`game-cargo`), freeing pack space (`game-stockpiles`), gathering (`game-ecology`), transferring portions (`game-property`), or eating (`game-physiology::ConsumeEdible`).
- **Caregiver acquisition (`acquire`):** Runs the same travel, container pickup, and gathering/taking stages to obtain a target food quantity *without* consuming it, leaving the food in the caregiver's inventory for delivery.
- **Local self-meal description (`local_meal`):** Read-only advisory stock demand for an explicit edible `Meal` whose actor is idle at an area and whose personally remembered destination and actual inventory locality both match. The quantity is the food definition's real portion. A remembered own carried store follows its actor, matching execution. No descriptor is returned for unspecified meals, travel, exploration, or forage stages.

## Boundaries

- **Physical stage runner only:** Owns no `game-sim` plugin, persistent resource, or utility scoring; calls typed requests on `game-transport`, `game-cargo`, `game-stockpiles`, `game-ecology`, and `game-physiology`.
- **Descriptions are not admission or authorization:** `local_meal` does not check available stock, permissions, health, or motor capacity, invoke hooks, reserve goods, or grant knowledge. Empty local stock can still produce a demand. Callers must recheck after intervening changes and execute the original request through its existing checks and hooks. Caregiver `acquire` plans must not be treated as self-meals merely because they use the same `Plan` type.

## How It Works

- **Partial collection support:** Accepts explicit target and minimum acceptable harvest quantities so a gatherer can still use a partially depleted patch rather than failing outright.
- **Resume-aware travel:** Continues interrupted road journeys from the actor's current `InTransit` progress via `game-knowledge` route queries.

## Game Integration

- **Plugin / Entry Point:** Pure execution library (`step`, `acquire`); re-exported through [`game-provisioning`](../game-provisioning/README.md).
- **Requests & Events:** Invokes `Move`, `Pickup`, `StoreSurplus`, `Gather`, `Take`, and `ConsumeEdible`.
- **Callers:** Called by [`game-provisioning`](../game-provisioning/README.md), [`game-characters`](../game-characters/README.md), and [`game-nurture`](../game-nurture/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-knowledge`](../game-knowledge/README.md), [`game-ecology`](../game-ecology/README.md), [`game-objects`](../game-objects/README.md), [`game-cargo`](../game-cargo/README.md), [`game-transport`](../game-transport/README.md), [`game-physiology`](../game-physiology/README.md), [`game-stockpiles`](../game-stockpiles/README.md).
- **Testing:** Exercised by `cargo test -p game-provisioning` and `cargo test -p game-care`.
