# game-maturation

Asynchronous curing, fermentation, and aging of prepared workshop batches into finished goods.

**Why it matters:** Preserved foods like cheese, cured meat, and fermented provisions require time and stable temperatures after initial preparation, freeing their maker to do other work while batches mature.

## Responsibilities

- **Maturation Tracking:** Tracks prepared, inedible workshop batches marked with `matures` and `maturation-days` tags until their ready timestamp.
- **Transfer Persistence:** Preserves remaining maturation time when curing batches are moved between containers or stores.
- **Temperature-Sensitive Finishing:** Evaluates storage temperature on completion against finishing temperature tags, converting the batch into finished food (then tracked by `game-preservation`) or a spoiled failure.

## Boundaries

- **No Active Crafting Labor:** Does not manage recipe input reservation or active crafting work (owned by `game-workshop`).
- **No Finished Spoilage:** Hands completed edible items off to `game-preservation` for shelf-life tracking.

## How It Works

- **Cargo Event Hooks:** Listens to `game-cargo` stock events to register newly crafted maturing items and follow them across inventory transfers.
- **Scheduled Completion:** Wakes at the batch's maturation deadline, queries `game-food-storage` for the container's thermal conditions, and swaps the maturing item for either its finished product or spoiled waste via `game-cargo`.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers maturation state (`State`), cargo event listeners, and completion timers in `game-assembly`.
- **Requests & Events:** Consumes `game-cargo` events and dispatches `game-cargo` item replacements upon batch completion.
- **Callers:** Driven automatically by `game-workshop` output creation and `game-food-storage` thermal queries.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-cargo`, `game-objects`, `game-food-storage`.
- **Testing:** Integration tests in `tests/` cover curing duration, transfer persistence across inventories, and temperature-dependent success or failure on completion.
