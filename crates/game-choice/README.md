# game-choice

Proposal collection, executable offers, timed undertakings, and agenda helpers for competing domain mechanics.

**Why it matters:** Dozens of independent plugins propose actions for the same character at the same time. `game-choice` gathers those offers without coupling feature crates to one another. `game-characters` owns the final decision and commitment policy.

## Responsibilities

- **Proposal Collection:** Broadcasts `Collect` requests so independent feature crates can submit scored `Offer` / `Proposal` candidates for an actor.
- **Bounded Collection:** Retains up to 256 promising offers under the actor's supplied weights, with deterministic ordering and tie-breaking.
- **Execution & Reconsideration:** Dispatches the winning `Run` / `Undertake` callback and handles `Outcome::Reconsider` when an action finishes or becomes invalid.
- **Agenda Scheduling:** Provides `Schedule` and `Next` helpers to wake characters when their current task completes or needs re-evaluation.

## Boundaries

- **Domain-Agnostic:** Does not define what any action does or how domain urgency is calculated (owned by each proposing feature crate).
- **No Direct State Mutation:** Only stores per-actor choice scheduling and active proposal metadata.

## How It Works

- **Provider Registration:** Feature plugins register proposal providers (`provider`) and execution handlers during setup.
- **Collect-Then-Run Cycle:** When a character's decision timer fires (`Next`), `game-choice` gathers proposals from all registered providers, applies `game-utility` scoring and hysteresis against any active task, and invokes the winning handler.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers choice state, `Collect` arbitration, and `Schedule` / `Next` agenda handlers in `game-assembly`.
- **Requests & Events:** Exposes `Collect`, `submit`, `Schedule`, `Next`, and `Undertake` hooks.
- **Callers:** Used by almost all autonomous behavior crates (`game-food-actions`, `game-livelihoods`, `game-agriculture`, `game-care`, `game-social-life`, `game-inquiry`, `game-conflict`, etc.).

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-utility`.
- **Testing:** Integration tests in `tests/` verify order-independent arbitration, hysteresis task persistence, preemption by urgent needs, and reconsideration loops.

Temporary proposals use typed `Pending::immediate` calls and leave no retained
collection at transaction commit. Scheduling an undertaking validates its complete
nested payload through `Pending::new`. Bounded templates reuse immutable offers;
current motives remain independent. An undertaking completing means its callback
was attempted, rather than guaranteeing that the domain action succeeded.

Bounded recall retains a proposal continuing the actor's actual current action.
It still counts toward the ordinary offer limit and receives ordinary utility
scoring; stronger alternatives can win. Unrelated continuation identities receive
no retention preference.

`Keys` indexes append-only offer keys by numeric fingerprints. Hash collisions
still compare complete strings; motive IDs are not treated as key equality.

Provider callbacks prepare independent alternatives. When collected in an isolated
scratch transaction, they may run concurrently. Raw drafts merge in the original
provider order before applying the existing 256-offer ranking, duplicate checks and
continuation preference. Request hooks retain serial collection. Any provider
attempting world effects forces the complete collection back through serial dispatch.
Callers must not use partial offers from other providers to compute their own drafts;
ordinary event subscribers provide ordered serial barriers when needed.

Bounded offer retention uses an in-place min-heap after reaching the offer limit.
The priority remains active continuation, weighted score, numeric motive tie break
and full key. Weight changes rebuild the heap. Hash counts accelerate removal;
full keys still decide duplicates. Integration tests compare retained offers with
a scan oracle under changing weights, saturation, ties and continuations.

Temporary proposals can carry `Pending::immediate` inside undertakings and other
action wrappers. Chosen work passes through the ordinary schedule/action boundary,
which validates nested payloads before commit. This avoids serializing discarded
options. Tests cover deferred serialization, scheduled completion and cross-worker
save/load.
