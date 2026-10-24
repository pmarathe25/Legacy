# game-characters

Individual mind scheduling, survival-plan commitment, urgent hunger preemption, and idle activity reviews.

Default content uses a 15-minute floor for ordinary interruptions while busy.
Idle decisions and completed actions stay prompt; own urgent needs and newly
observed/worsening urgent care evidence can interrupt immediately. Perceptions
continue updating. Existing saves and the programmatic `Rules::default()` retain
their stored/legacy policy. See [the evaluation](../../tools/performance/BUSY_ATTENTION.md)
for the deliberate change in activity pacing and its quality checks.

Decision parameters and optional routine reuse live in
[`game-autonomy-policy`](../game-autonomy-policy/README.md). Existing `Rules`, `Configure`,
and `load` imports remain reexports. Routine reuse is disabled by default. The
`routines-30` history overlay explicitly enables the behavioral experiment; minds
retain its current executable and deadline, while external wake events invalidate it.

The independent `option_memory` experiment retains recently considered offers
through `game-autonomy-policy`, while this owner retains all mind writes and action
selection. It keeps ordinary action timing and fresh survival planning, but can
delay nonurgent opportunity discovery. It is disabled by default; use the
`options-5` history overlay or the explicit policy comparison tool to evaluate it.

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

- **Dependencies:** `game-autonomy-policy`, `game-core`, `game-sim`, `game-storage`, `game-content`, `game-diagnostics`, `game-objects`, `game-cargo`, `game-property`, `game-transport`, `game-ecology`, `game-physiology`, `game-vitality`, `game-nutrition`, `game-traits`, `game-knowledge`, `game-food-memory`, `game-provisioning`, `game-social-life`, `game-utility`, `game-choice`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-characters`) cover plan continuation, stale-stock recovery, blocked routes, urgent hunger interruption, learned values, and multi-worker save/load consistency.

Recent failures increase the perceived cost of the specific option; they do not
remove it from consideration. Expressed learning and risk tolerance affect that
cost, and a strong motive can justify an immediate retry. Unrelated notifications
retain the evidence; successful progress revises it. The legacy `retry_after`
save field bounds fading failure memory rather than imposing an action cooldown.
Attention uses a minute-based horizon bounded by urgent hunger forecasts and the
authored review cadence, including coarse history profiles.

Task persistence depends on attention and ongoing work costs. The inherited
rigidity trait concerns convictions and preferences, rather than every change of
activity or care recipient; those decisions remain with their context owners.

`Resume` exposes the existing typed scheduled wakeup payload for advisory queue
inspection. It keeps its historical wire key and ordinary owner handler.

`Minds` is re-exported from `game-character-state`, which contains only saved state
and its resource contract. This plugin still registers and owns all writes to it;
the `characters.minds` wire key, owner, and schema are unchanged.

Optional `game-sim/review-trace` telemetry records review attempts, nested request,
event and listener scopes, wake suppression, selected observed state, and ordered
numeric proposal fingerprints. It never controls scheduling or choices. See
`tools/performance/REVIEWS.md` for interpretation and replay verification.


In the explicitly enabled decision-frame model, cohort observations occur together
at the frame boundary. Reviews in that cohort reuse this observation phase until
the next step, while current physiology, urgent food planning and physical action
validation remain live. Ordinary reviews keep their existing observation timing.
`game-autonomy-frames` owns cohort forecasting and the observation adapter; this
owner still controls all mind writes, wakeups and final selection.


Enabled frames coalesce notifications onto the next boundary and mark at most one
review per mind per step, including duplicate scheduled callbacks. A callback
encountered between boundaries is deferred to the next one. New injury and food
cues therefore affect decision making within at most one frame; their physical
state changes and action completions remain immediate. The original event-driven
wakeup path is unchanged with frames absent.

Wake admission and scheduling now live in `game-character-clock`, registered under
this owner's existing identity. Optional frame recovery gives an idle fallback one
next-step recheck, preserving its future wake when the current review finishes.
It does not repeatedly poll an unchanged idle character. A selected snapshot
proposal that its pure provider no longer offers is skipped before execution;
genuine rejected attempts retain failure memory. `game-decision-food` can prepare
food alternatives across actors when separately enabled; scoring and execution
remain here and with their feature owners.

`game-decision-intents` now supplies the shared pure scorer for survival plans,
continuations, trait weights and failure risk. This owner retains execution and
all mind writes. The separate `batch_decisions` frame setting prepares these
scores, food queries and all offer providers in one actor task. Deferred sensing
updates apply only when that actor resolves. A newly started joint activity
defers the complete obsolete turn, including those updates. If preparation
attempts an undeferred effect, that actor uses live review instead.
