# game-metabolism

`game-metabolism` derives individual energy demand, fatigue accumulation, and rest recovery rates from expressed traits, vitality, and ambient thermal stress.

**Why it matters:** A growing infant, a large muscular adult, and someone shivering in freezing weather should not burn calories or recover fatigue at identical flat rates.

## Responsibilities

- **Rate derivation (`rates`, `demand`):** Scales baseline `game-body::Rules` hunger and recovery rates using expressed body size, strength, constitution, basal metabolism, age development (`game-vitality`), and optional environmental stress (`Environment` query).
- **Environmental hook (`Environment`):** Defines the read-only query interface that [`game-exposure`](../game-exposure/README.md) serves to report thermal stress penalties.

## Boundaries

- **Pure rate calculation:** Owns no persistent resource state or decision logic; [`game-soma`](../game-soma/README.md) advances stored bodies using these rates and [`game-exposure`](../game-exposure/README.md) owns body temperature.

## How It Works

- **Size & capability maintenance:** Larger bodies and high expressed strength or constitution require more energy per tick; high basal metabolism speeds up recovery at the cost of faster hunger accumulation.
- **Childhood scaling:** Infants start with smaller absolute caloric requirements that scale upward along `game-vitality` growth curves.

## Game Integration

- **Plugin / Entry Point:** Library crate (`rates`, `demand`, `Environment` query); registers no stateful `game-sim` plugin.
- **Requests & Events:** Defines the `Environment` read-only query.
- **Callers:** Called by [`game-soma`](../game-soma/README.md), [`game-physiology`](../game-physiology/README.md), and [`game-nutrition`](../game-nutrition/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-body`](../game-body/README.md), [`game-vitality`](../game-vitality/README.md), [`game-traits`](../game-traits/README.md).
- **Testing:** `cargo test -p game-metabolism` verifies infant vs. adult demand scaling, basal metabolism trade-offs, and thermal stress modifiers.
