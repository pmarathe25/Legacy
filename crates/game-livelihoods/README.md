# game-livelihoods

Autonomous pursuit of personal material stock goals by chaining gathering and workshop crafting.

**Why it matters:** Characters need tools, containers, clothing, and household kits before emergencies strike. Livelihoods turn declarative stock goals into intelligence-bounded gathering and crafting plans.

## Responsibilities

- **Stock Goal Evaluation:** Loads personal material goals from `content/livelihoods/daily.toml` and compares them against carried possessions, personal claims, and cooperatively shared local supplies.
- **Intelligence-Bounded Planning:** Scales `game-work-planning` search depth (2–32), plan length (4–256), and node checks (32–8,192) using the character's intelligence trait (`game-traits`).
- **Choice Proposal & Execution:** Submits weighted livelihood proposals into `game-choice` and executes the resulting `game-ecology::Gather` or `game-workshop::Craft` step.

## Boundaries

- **No Recipe or Resource Ownership:** Delegates actual resource extraction to `game-ecology` and item transformation to `game-workshop`.
- **No Immediate Hunger Relief:** Routine material provisioning is separate from urgent meal seeking and eating (owned by `game-food-actions` and `game-provisioning`).

## How It Works

- **Accessible Stock Accounting:** Aggregates items the actor carries, owns locally (`game-property`), or can borrow from trusted household/lineage peers (`game-relationships`, `game-lineage`).
- **Plan Caching & Step Execution:** Invokes `game-work-planning` using recipes the actor actually knows (`game-knowledge`), scores the highest-priority missing stock goal, and executes the first unfulfilled step when chosen by `game-choice`.

## Game Integration

- **Plugin / Entry Point:** `Plugin` loads livelihood goals from `game-content` and registers `game-choice` proposal providers in `game-assembly`.
- **Requests & Events:** Submits livelihood proposals during `game-choice::Collect` and dispatches `game-ecology::Gather` or `game-workshop::Craft` requests when selected.
- **Callers:** Driven by `game-choice` decision cycles across all capable characters.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-content`, `game-objects`, `game-cargo`, `game-property`, `game-transport`, `game-physiology`, `game-knowledge`, `game-ecology`, `game-workshop`, `game-characters`, `game-traits`, `game-relationships`, `game-lineage`, `game-choice`, `game-work-planning`, `game-vitality`, `game-food-memory`, `game-diagnostics`.
- **Testing:** Integration tests in `tests/` verify stock goal accounting, cooperative supply sharing, intelligence-bounded multi-step planning, and end-to-end tool/kit crafting.

Queued plans share individual records across transaction snapshots, avoiding
copies of other characters' step queues when one character advances a plan.

[`game-work-planning`](../game-work-planning/README.md) owns read-only stock-goal
ranking and search. It reuses results only when all search inputs match, with a
512-entry, 4 MiB estimated-data cache. Livelihoods retains scheduled work, execution
validation, failure learning and achievements. Goal and Step remain reexports.
