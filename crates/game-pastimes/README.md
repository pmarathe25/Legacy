# game-pastimes

The experimental fast-review policy defers new leisure discovery until a full
review. Ordinary reviews and direct leisure requests retain their normal behavior.

Leisure activities, material supply consumption, skill practice, and interest reinforcement.

**Why it matters:** Human life is more than bare subsistence. When survival needs are met, characters pursue music, writing, drawing, weaving, tea, and reflection—shaping their skills, tastes, and future material goals.

## Responsibilities

- **Activity Catalog:** Loads data-driven leisure activities from `content/pastimes/daily.toml` with required tools (`requires`), consumed supplies (`inputs`), durations, and associated skills/concepts.
- **Timed Execution (`Pursue`):** Consumes `inputs` at start, occupies the actor's action slot, and verifies continued locality, permission (`game-property`), and possession of reusable `requires` instruments on completion.
- **Experience & Value Reinforcement:** Increments personal participation counts, awards practice to `game-skills`, and strengthens the participant's concept value (`game-relationships`) so pastimes influence future leisure and `game-livelihoods` stock goals.
- **Autonomous Selection:** Proposes leisure activities during `game-characters::IdleReview` weighted by traits, learned interests, individual variation, and recent satisfaction decay.

## Boundaries

- **Individual Experience Only:** Playing music or reflecting is a personal experience; it does not automatically broadcast mood buffs or invent distinct cultural artifacts (owned by `game-works`).
- **Player Autonomy Preserved:** Excludes the active player from automatic leisure selection while exposing `Pursue` for explicit player commands.

## How It Works

- **Start Consumption, End Validation:** Consumes expendable inputs immediately when `Pursue` begins. If the activity is cancelled or a required instrument disappears before completion, inputs remain spent and no skill or satisfaction is awarded.
- **Diminishing Immediate Repetition:** Recent completion lowers an activity's immediate utility to encourage variety without strictly forbidding voluntary repetition.

## Game Integration

- **Plugin / Entry Point:** `Plugin` loads pastime definitions (`Install` / `Enable`) and registers `Pursue` handlers and `IdleReview` hooks in `game-assembly`.
- **Requests & Events:** Exposes `Pursue`, `Install`, and `Enable`; emits pastime completion events consumed by `game-works`.
- **Callers:** Driven by `game-characters::IdleReview`, `game-choice`, and explicit player commands.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-content`, `game-objects`, `game-cargo`, `game-property`, `game-transport`, `game-physiology`, `game-skills`, `game-traits`, `game-relationships`, `game-characters`, `game-choice`, `game-utility`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-pastimes`) verify supply consumption, permission checks, cancellation penalties, skill/value learning, satisfaction decay, and save/load consistency.
