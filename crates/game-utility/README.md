# game-utility

Pure 64-bit utility scoring, trait-weighted channel projection, hysteresis margins, and deterministic tie-breaking.

**Why it matters:** Characters constantly weigh survival needs, comfort, social duties, and curiosity. A shared, deterministic scoring algebra lets disparate mechanics compare options fairly without oscillation or floating-point drift.

## Responsibilities

- **Multi-Channel Scoring:** Represents candidate motivations across distinct channels (`Candidate`) using saturating 64-bit signed arithmetic.
- **Trait-Weighted Projection:** Projects multi-channel candidate scores through an actor's personality and trait weights into a single scalar utility.
- **Urgency & Hysteresis:** Filters out low-urgency proposals and enforces switching margins so characters finish ongoing tasks instead of thrashing between near-equal choices.
- **Deterministic Tie-Breaking:** Breaks exact score ties using stable candidate identifiers rather than evaluation order.

## Boundaries

- **Pure Math Library:** Owns no ECS state, registers no simulation plugin, and knows nothing about specific domain actions.
- **No Proposal Dispatch:** Does not collect or execute proposals (owned by `game-choice`).

## How It Works

- **Channel Dot Products:** Combines domain-provided motivation channels (such as survival urgency, material gain, social affinity, or curiosity) with actor-specific sensitivity weights using saturating integer math.
- **Incumbent Bias:** Adds a configurable hysteresis bonus to the currently active action before comparing against challenger candidates.

## Game Integration

- **Plugin / Entry Point:** Pure library crate (`Candidate`, `Score`, `Weights`, selection helpers); requires no `game-sim` plugin registration.
- **Requests & Events:** None.
- **Callers:** Used directly by `game-choice` and domain proposal providers across the workspace.

## Dependencies & Testing

- **Dependencies:** Pure Rust crate with no workspace dependencies.
- **Testing:** Unit tests verify saturating arithmetic, channel projection, urgency thresholds, hysteresis stability, and order-independent tie-breaking.
