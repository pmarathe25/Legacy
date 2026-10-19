# game-characters

Individual mind scheduling, survival-plan commitment, urgent hunger preemption, and idle activity reviews.

**Why it matters:** Characters need a coherent mind loop that prioritizes urgent survival, commits to multi-step plans without thrashing, and delegates open time to care, work, or companionship.

## Responsibilities

- **Mind Lifecycle & Deadlines:** Owns per-character mind records, indexed wake-up deadlines, failed-option cooldowns, and death cleanup.
- **Survival Commitment & Preemption:** Commits to food acquisition plans from `game-provisioning`, resumes plans across travel/gathering steps, and interrupts lower-priority work or rest when urgent hunger thresholds hit.
- **Idle Activity Dispatch (`IdleReview`):** Emits `IdleReview` notifications when survival needs are satisfied so upper owners (care, horticulture, livelihoods, pastimes, social life) can propose activities without creating a second scheduler.
- **Ruler Autonomy:** Steps in to control the current ruler only while explicit player control is absent.

## Boundaries

- **No Direct Food Search:** Delegates food candidate evaluation and route scoring to `game-provisioning`.
- **No Social Matching:** Delegates companion selection and conversation execution to `game-social-life`.
- **No Hidden World Access:** Evaluates choices solely from the character's own carried supplies and personal beliefs (`game-knowledge`).

## How It Works

- **Batched Deadline Reviews:** Processes at most 128 due minds per committed step in deterministic person order. Sleeping minds wait in an indexed deadline queue until their action finishes or physiology forecasts an urgent hunger threshold.
- **56-Byte Candidate Scoring:** Evaluates candidate actions against five trait-weighted numeric channels, authored switching margins, and learned food values (`game-food-memory`).
- **Personal Attention:** Expressed attention extends routine reconsideration horizons; hunger and fatigue shorten them. Feeding, injury, arrivals, completed actions, and new evidence can wake a character immediately. This does not forbid activities or impose a leisure cooldown.
- **Immediate Reconsideration:** Wakes a character immediately when an active food intention or unmet survival need remains after eating or finishing a travel leg, avoiding artificial review-interval delays.

## Game Integration

- **Plugin / Entry Point:** `Plugin` loads rules from `content/characters/*.toml` and registers mind storage, review clocks, and action completion listeners in `game-assembly`.
- **Requests & Events:** Handles mind enrollment, review ticks, and action interruptions; emits `IdleReview` notifications and dispatches travel/gather/eat requests.
- **Callers:** Coordinates with `game-physiology`, `game-provisioning`, `game-food-actions`, `game-social-life`, `game-choice`, and upper-level activity plugins.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-content`, `game-diagnostics`, `game-objects`, `game-cargo`, `game-property`, `game-transport`, `game-ecology`, `game-physiology`, `game-vitality`, `game-nutrition`, `game-traits`, `game-knowledge`, `game-food-memory`, `game-provisioning`, `game-social-life`, `game-utility`, `game-choice`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-characters`) cover plan continuation, stale-stock recovery, blocked routes, urgent hunger interruption, learned values, and multi-worker save/load consistency.

Recent failures increase the perceived cost of the specific option; they do not
remove it from consideration. Expressed learning and risk tolerance affect that
cost, and a strong motive can justify an immediate retry. Unrelated notifications
retain the evidence; successful progress revises it. The legacy `retry_after`
save field bounds fading failure memory rather than imposing an action cooldown.
Attention uses a minute-based horizon bounded by urgent hunger forecasts and the
authored review cadence, including coarse history profiles.
