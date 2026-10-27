# game-physiology

`ServeLocalMeals` is a strict phased boundary for exclusive, idle local self-meals
and voluntary care offers. It runs each `Serve` hook and exact physical stock
operation in input order, rolls back declined offers' hooks, and settles all
accepted personal digestion, diet, body and mood effects before publishing `Fed`.
Callers must wrap the request in an attempt before ordered fallback. Busy or
overlapping participants, incompatible personal-effect hooks, changed participants
and lethal nutrition require that fallback. This phase intentionally differs from
actor-by-actor listener visibility; output order and worker-count replay are stable.

Serving and eating requests live in
[`game-physiology-eating`](../game-physiology-eating/README.md), registered
under the existing physiology owner. `TryServe` reports actual acceptance or
refusal while preserving `Serve` hooks and rolling their effects back on a
decline; ordinary `Serve` retains its existing error behavior.

`game-physiology` orchestrates eating, feeding others, resting, and batched physiological wake-ups over bodily state owned by [`game-soma`](../game-soma/README.md).

**Why it matters:** Connects physical cargo consumption, dietary nutrition, stomach satiety, and action clocks so eating or resting requires real food, co-location, and recipient consent.

## Responsibilities

- **Eating & serving (`Eat`, `ConsumeEdible`, `Serve`, `Feed`):** Consumes edible portions from an accessible local inventory via `game-cargo`, records dietary intake in `game-nutrition` and stomach fullness in `game-digestion`, and applies hunger relief and food effects to `game-soma` and `game-vitality`.
- **Experimental self-meal wave (`ConsumeSelfMeals`):** Settles stock changes with their listeners in order, then updates surviving bodies as an owner batch and applies all mood effects before feeding observers. The phase intentionally changes observation timing; incompatible hooks, invalidation or failure retry the ordered scalar path transactionally. The character cohort uses it only when the separate `meal_wave` tuning is enabled.
- **Timed rest (`Rest`):** Occupies the actor's canonical action slot while setting `game-soma` into resting recovery until rested or interrupted.
- **Observer query (`Observers`):** Reports awake, living people co-located in an area for witness and perception checks.

## Boundaries

- **Action orchestration vs. body storage:** [`game-soma`](../game-soma/README.md) stores and validates `Bodies`, [`game-body`](../game-body/README.md) provides pure forecasts, [`game-nutrition`](../game-nutrition/README.md) tracks nutrients/cravings, and [`game-digestion`](../game-digestion/README.md) tracks stomach fullness.

## How It Works

- **Voluntary feeding assent:** When a caregiver executes `Serve` or `Feed`, the recipient independently evaluates hunger, appetite, remembered food preference, and current activity; declining leaves inventory and body state untouched.
- **Batched wake-ups:** Uses `Builder::batch` (up to 128 requests) to group adjacent physiological ticks and wake-ups into a single atomic phase.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`physiology`), installing `game-soma` and `game-digestion` hooks and `Eat` / `Rest` / `Serve` handlers.
- **Requests & Events:** Handles `Configure`, `Spawn`, `Eat`, `ConsumeEdible`, `Serve`, `Feed`, `Rest`, `Damage`, `Heal`, and `Observers`; re-exports `Died`, `Ate`, `Fed`.
- **Callers:** Bound to `Command::Eat` and `Command::Rest` in `game-assembly`, and called by `game-food-actions`, `game-nurture`, `game-health`, and `game-conflict`.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-storage`](../game-storage/README.md), [`game-packed`](../game-packed/README.md), [`game-content`](../game-content/README.md), [`game-objects`](../game-objects/README.md), [`game-body`](../game-body/README.md), [`game-soma`](../game-soma/README.md), [`game-nutrition`](../game-nutrition/README.md), [`game-digestion`](../game-digestion/README.md), [`game-vitality`](../game-vitality/README.md), [`game-traits`](../game-traits/README.md), [`game-transport`](../game-transport/README.md), [`game-cargo`](../game-cargo/README.md), [`game-property`](../game-property/README.md).
- **Testing:** `cargo test -p game-physiology` verifies eating, voluntary feeding/rejection, rest interruption, starvation death, immortal recovery, and satiety.

The ordinary physiology profile enables slow natural recovery during nourished rest. Fractional progress persists across ticks and saving; expressed recovery includes inherited potential and current condition. Recovery cannot revive a dead character, replace lost lifespan, or exceed current maximum health. Fixture profiles retain their configured recovery rates. Feeding requires the caregiver's motor readiness; recipients may voluntarily interrupt an activity to accept food.
