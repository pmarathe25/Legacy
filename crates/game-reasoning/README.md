# game-reasoning

Pure bounded empirical inference over numerical observations, single-factor contrasts, and conditional deduction.

**Why it matters:** For characters to learn cause and effect without hardcoded scripts, they need a lightweight inference engine that extracts rules from noisy observations and generalizes cautiously across contexts.

## Responsibilities

- **Single-Factor Contrasts:** Compares pairs of numerical observations that differ along a single controlled or varied factor to isolate causal effects.
- **Experiment Deduplication:** Tracks shared experiment IDs so repeated observations of the same trial do not artificially inflate confidence.
- **Tentative Abstraction:** Generalizes consistent causal findings across similar settings while tracking exceptions.
- **Confidence-Decaying Deduction:** Chains inferred rules into multi-step conditional predictions with decaying confidence per hop.

## Boundaries

- **Pure Inference Engine:** Owns no ECS state, registers no simulation plugin, and has zero `game-*` workspace dependencies.
- **No Social or Action Logic:** Does not decide when characters reflect or how they act on conclusions (owned by `game-inquiry` and `game-food-inquiry`).

## How It Works

- **Bounded Pairwise Comparison:** Scans a bounded window of structured observations, identifies single-variable deltas that correlate with outcome shifts, and updates confidence-weighted hypotheses.
- **Hierarchical Generalization:** Promotes specific rules to broader category abstractions when multiple distinct subjects exhibit the same response pattern.

## Game Integration

- **Plugin / Entry Point:** Pure library crate; requires no `game-sim` plugin registration.
- **Requests & Events:** None. Operates on serializable observation and hypothesis value types.
- **Callers:** Used by `game-inquiry`, `game-food-inquiry`, and `game-observations`.

## Dependencies & Testing

- **Dependencies:** `serde` only (no workspace dependencies).
- **Testing:** Unit tests cover single-factor isolation, experiment ID deduplication, cross-setting abstraction, noise rejection, and deductive confidence decay.
