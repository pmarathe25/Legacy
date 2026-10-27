# game-nutrition

`Meals` consumes distinct recipients' diets on private owner rows in the bounded
worker pool, then publishes rows and returns effects in input order. It rejects
scalar meal/registration hooks and duplicate recipients; use the scalar requests
for those cases. Registration and final row installation remain ordered.

`game-nutrition` tracks each person's dietary nutrient reserves, personally experienced food effects, and substance tolerance or dependence.

**Why it matters:** People need a balanced diet rather than a single calorie source, and they learn which foods nourish, sicken, or create cravings only after personally tasting them.

## Responsibilities

- **Dietary state (`Nutrition`):** Stores per-person nutrient balances, experienced food records, and substance dependence/withdrawal state (`nutrition.state`).
- **Intake & depletion (`Digest`):** Updates nutrient reserves when food is eaten, decays reserves over elapsed time according to metabolic demand, and returns health, mood, and alertness deltas.
- **Contextual appetite (`appetite_for`):** Scores how well a previously experienced food addresses current nutrient deficiencies or cravings.

## Boundaries

- **Dietary memory & balance:** Tracks nutrient reserves and learned food experience; [`game-physiology`](../game-physiology/README.md) executes the physical eating action and [`game-food-memory`](../game-food-memory/README.md) combines `appetite_for` with remembered inventory stocks.

## How It Works

- **Experience-gated appraisal:** Before a character eats a `GoodId` for the first time, its specific nutrient profile and side effects are unknown to their decision policy; tasting it records a personal experience entry.
- **Bounded nutrient motive:** `appetite_for` caps a food's deficiency-relief score by the actual nutrients present in a portion, so an energy-only food never promises to cure a protein deficiency.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`nutrition`, resource `nutrition.state`).
- **Requests & Events:** Handles `Enable` and `Digest`; subscribes to `game_physiology::Died`.
- **Callers:** Called by [`game-physiology`](../game-physiology/README.md) on food consumption and queried by [`game-food-memory`](../game-food-memory/README.md), [`game-characters`](../game-characters/README.md), and [`game-nurture`](../game-nurture/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-storage`](../game-storage/README.md), [`game-objects`](../game-objects/README.md), [`game-metabolism`](../game-metabolism/README.md), [`game-vitality`](../game-vitality/README.md), [`game-traits`](../game-traits/README.md).
- **Testing:** `cargo test -p game-nutrition` verifies nutrient depletion, deficiency penalties, first-taste learning, dependence/withdrawal, and `appetite_for` bounds.

Pure dietary records and batched food evaluation live in [`game-diet`](../game-diet/README.md),
with existing public types reexported here. `food_evaluation` borrows the current
personal diet and computes nutrient deficits once across food alternatives.
Tests compare it with the individual query functions, including saturation and
unknown foods. Simulation resource ownership and intake remain in this crate.
