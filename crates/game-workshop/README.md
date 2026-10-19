# game-workshop

Configurable crafting recipes, input reservation, timed labor, and cultivar propagation across transformation chains.

**Why it matters:** Raw resources rarely satisfy human needs directly. Workshops turn gathered materials and harvests into tools, clothing, preserved foods, and building kits through skilled labor.

## Responsibilities

- **Recipe Registry:** Loads data-driven crafting recipes from `content/workshop/*.toml` (inputs, tool requirements, skill tags, work duration, and outputs).
- **Input Reservation & Labor:** Reserves required input stacks in `game-cargo`, runs timed `Craft` actions, and awards practice to `game-skills`.
- **Cultivar Propagation:** Preserves plant cultivar identity (`game-cultivars`) when processing harvested crops into seed lots or derived products.
- **Interruption & Waste:** Releases reservations or records wasted materials if crafting is interrupted or if the target inventory moves away.

## Boundaries

- **No Multi-Step Planning:** Does not search multi-step recipe chains or decide which goal to pursue (owned by `game-craft-planning` and `game-livelihoods`).
- **No Post-Craft Curing:** Does not age or ferment outputs after the recipe completes (owned by `game-maturation`).

## How It Works

- **Reservation-Backed Execution:** When a character starts `Craft`, `game-workshop` validates tool access (`game-property`), reserves inputs in `game-cargo`, and schedules completion scaled by character skill (`game-skills`) and physiology (`game-physiology`).
- **Completion & Event Emission:** On completion, consumes reserved inputs, deposits outputs into `game-cargo`, propagates any cultivar metadata, and emits crafting completion events for `game-observations`.

## Game Integration

- **Plugin / Entry Point:** `Plugin` loads workshop recipes from `game-content` and registers crafting state and `Craft` request handlers in `game-assembly`.
- **Requests & Events:** Exposes `Craft` requests, recipe catalog queries (`Recipes`), and crafting completion/practice events.
- **Callers:** Invoked by `game-livelihoods`, `game-food-actions`, and scenario fixtures; observed by `game-observations`.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-content`, `game-objects`, `game-cargo`, `game-property`, `game-transport`, `game-physiology`, `game-skills`, `game-cultivars`.
- **Testing:** Integration tests in `tests/` cover recipe validation, input reservation, skill-scaled crafting times, cultivar seed propagation, and interruption cleanup.
