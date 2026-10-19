# game-inquiry

Bounded episodic memory of witnessed events, empirical reflection, and voluntary peer-to-peer teaching.

**Why it matters:** Characters should discover how the world works from lived experience rather than global omniscience. `game-inquiry` lets people record what they witness, infer causal rules during quiet reflection, and explain discoveries to others.

## Responsibilities

- **Episodic Memory (`Witness`):** Stores a bounded personal log of perceived numerical episodes per character using copy-on-write `game-storage::Records`.
- **Empirical Reflection:** Periodically feeds witnessed episodes into `game-reasoning` to derive or refine personal causal beliefs and practical knowledge.
- **Voluntary Teaching (`Explain`):** Proposes and executes peer teaching actions—offering detailed, practical, or demonstrated explanations that transfer evidence and knowledge via `game-communication` and `game-testimony`.

## Boundaries

- **No Domain Event Wiring:** Does not subscribe directly to agriculture, crafting, or weather plugins (adapted by `game-observations`).
- **No Pure Inference Math:** Delegates statistical contrast and abstraction math to `game-reasoning`.

## How It Works

- **Witness Recording:** Accepts `Witness` requests containing structured domain variables and outcomes, evicting the oldest or least salient episodes when a character's memory capacity is full.
- **Reflect & Explain Proposals:** Submits reflection and teaching proposals to `game-choice`. When `Explain` runs, the speaker packages supporting observations or conclusions into testimony for the listener.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers inquiry memory (`Records`), reflection handlers, and `Explain` choice providers in `game-assembly`.
- **Requests & Events:** Handles `Witness` and `Explain` requests; dispatches `game-knowledge` updates and `game-testimony` transfers.
- **Callers:** Fed by `game-observations` and `game-food-inquiry`; queried by UI/diagnostics and social learning flows.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-reasoning`, `game-knowledge`, `game-testimony`, `game-choice`, `game-communication`, `game-transport`, `game-physiology`, `game-vitality`, `game-relationships`, `game-social-life`.
- **Testing:** Integration tests in `tests/` cover bounded episode retention, causal rule discovery from witnessed contrasts, and peer-to-peer teaching modes.

Contrasting personally witnessed episodes without a remembered explanation notify
`Uncertain`. The information owner can turn this into curiosity about the subject,
allowing a character to ask for an explanation rather than having to discover it
alone. Notification itself grants no explanation and prescribes no action.
