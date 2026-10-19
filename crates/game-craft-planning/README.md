# game-craft-planning

Pure multi-step production planner that chains gathering and workshop recipes to satisfy target item goals.

**Why it matters:** Crafting a tool or meal often requires intermediate components, tools, and raw materials. A pure, bounded search planner lets characters figure out multi-step production chains without hardcoding scripts.

## Responsibilities

- **Backward Recipe Search:** Searches backward from desired item tags or kinds through available `game-workshop` recipes and local `game-ecology` gathering sources.
- **Cognitive Search Bounds:** Enforces caller-supplied limits on search depth, total plan length, and candidate node evaluations so planning scales with character intelligence.
- **Actionable Step Ordering:** Produces an ordered sequence of `Step` actions (`Gather` or `Craft`) ready for immediate execution.

## Boundaries

- **Pure Planning Library:** Owns no ECS state, registers no simulation plugin, and mutates no world resources.
- **No Goal Selection:** Does not decide which items a character wants or score competing desires (owned by `game-livelihoods` and `game-food-actions`).

## How It Works

- **Stock-Aware Decomposition:** Compares required recipe inputs and tools against a caller-provided snapshot of accessible stock. Missing intermediates are recursively expanded into prerequisite recipes or wild gathering steps.
- **Bounded Evaluation:** Terminates cleanly when depth, step count, or node check budgets are exhausted, returning the shortest valid production plan found within the actor's cognitive limits.

## Game Integration

- **Plugin / Entry Point:** Pure library crate (`Plan`, `Step`, `Limits`, `search`); requires no `game-sim` plugin registration.
- **Requests & Events:** None. Accepts stock/recipe views and returns deterministic `Plan` values.
- **Callers:** Used by `game-livelihoods` and `game-food-actions` to plan multi-step toolmaking, clothing, and meal preparation.

## Dependencies & Testing

- **Dependencies:** Pure Rust crate with no workspace dependencies.
- **Testing:** Unit and integration tests cover multi-stage recipe chains, tool prerequisites, stock reuse, cycle avoidance, and cognitive budget cutoffs.
