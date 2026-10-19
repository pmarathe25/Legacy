# game-shelter

`game-shelter` manages constructed indoor areas, shelter insulation, and finite fuel-burning hearths.

**Why it matters:** Indoor rooms protect occupants and stored food from harsh weather using the same road, area, and observation primitives as outdoor places—no special-case indoor spatial model required.

## Responsibilities

- **Indoor rooms (`Shelters`):** Tracks constructed shelter areas (`AreaId`), their outdoor parent area, insulation rating (`shelter-insulation`), claimed indoor store, and active hearth heat deadline (`shelter.state`).
- **Shelter construction (`Erect`):** Consumes a crafted shelter kit and builder labor to create a new indoor `AreaId` linked to the outdoor area by a short doorway road, plus a claimed indoor inventory.
- **Hearth heating (`FeedFire`):** Consumes physical fuel items (`hearth-heat` and `fuel-hours` tags, such as firewood or charcoal) to raise the room's temperature for a finite duration.

## Boundaries

- **Physical rooms vs. comfort choices:** Owns indoor area records, climate projection, and fuel burning; [`game-comfort`](../game-comfort/README.md) proposes autonomous shelter-seeking and fuel-gathering actions.

## How It Works

- **Derived indoor climate:** An indoor area has zero rainfall and derives its base temperature from the outdoor area's climate, adding hearth heat while fuel is actively burning and reverting when fuel expires.
- **Ordinary doorway travel:** Entering or leaving a shelter uses [`game-transport`](../game-transport/README.md) across the doorway link, so visibility, conversation, and storage access respect room boundaries automatically.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`shelter`, resource `shelter.state`).
- **Requests & Events:** Handles `Enable`, `Erect`, `FeedFire`; subscribes to climate updates to sync indoor climates.
- **Callers:** Enabled by [`game-campaign`](../game-campaign/README.md), driven autonomously by [`game-comfort`](../game-comfort/README.md), and queried by [`game-exposure`](../game-exposure/README.md) and [`game-food-storage`](../game-food-storage/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-storage`](../game-storage/README.md), [`game-land`](../game-land/README.md), [`game-transport`](../game-transport/README.md), [`game-cargo`](../game-cargo/README.md), [`game-property`](../game-property/README.md), [`game-objects`](../game-objects/README.md), [`game-environment`](../game-environment/README.md), [`game-physiology`](../game-physiology/README.md), [`game-names`](../game-names/README.md).
- **Testing:** `cargo test -p game-shelter` verifies room creation, doorway travel, finite fuel burning, climate propagation, and save/load state.
