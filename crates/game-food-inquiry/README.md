# game-food-inquiry

`game-food-inquiry` manages personal food-condition sampling, store inspection progress, and deliberate small-batch food preservation experiments.

**Why it matters:** Characters do not start with textbook knowledge of shelf life or cellar preservation; they learn how foods keep or spoil by inspecting real stores and running finite storage experiments.

## Responsibilities

- **Sampling & experiment state:** Tracks per-person food condition samples, rotating inspection cursors, and active storage experiment batches (`observations.food`).
- **Store inspection (`Inspect`):** Observes present edible goods in accessible local inventories (ignoring zero-quantity historical entries) to record freshness and spoilage evidence into [`game-inquiry`](../game-inquiry/README.md) and [`game-reasoning`](../game-reasoning/README.md).
- **Storage experiments (`Experiment`):** Splits a finite food sample into a target storage environment via [`game-food-storage`](../game-food-storage/README.md) so the character can observe relative decay rates.

## Boundaries

- **Food-specific observation & experimentation:** Owns food-condition sampling and storage trials; general meal, travel, production, and thermal observations live in [`game-observations`](../game-observations/README.md), and hypothesis evaluation lives in [`game-inquiry`](../game-inquiry/README.md) and [`game-reasoning`](../game-reasoning/README.md).

## How It Works

- **Subjective negative evidence:** Travelling to inspect a remembered store that has been moved or emptied disproves the old memory locally without revealing where the container went.
- **Diminishing novelty:** Completed inspections reduce expected information gain for that food/condition pair without permanently locking out future checks.

## Game Integration

- **Plugin / Entry Point:** Registers handlers and state through [`game-observations`](../game-observations/README.md) (`plugin(&mut Builder)`).
- **Requests & Events:** Handles `Inspect` and `Experiment`; submits proposals via [`game-choice`](../game-choice/README.md).
- **Callers:** Re-exported and composed by [`game-observations`](../game-observations/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-storage`](../game-storage/README.md), [`game-inquiry`](../game-inquiry/README.md), [`game-reasoning`](../game-reasoning/README.md), [`game-knowledge`](../game-knowledge/README.md), [`game-choice`](../game-choice/README.md), [`game-cargo`](../game-cargo/README.md), [`game-objects`](../game-objects/README.md), [`game-food-storage`](../game-food-storage/README.md), [`game-preservation`](../game-preservation/README.md), [`game-transport`](../game-transport/README.md), [`game-physiology`](../game-physiology/README.md), [`game-vitality`](../game-vitality/README.md).
- **Testing:** Exercised by `cargo test -p game-observations` for sampling, negative evidence, storage experiments, and save/load replay.
