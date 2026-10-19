# game-wellbeing

`game-wellbeing` translates lived experiences—meals, rest, injuries, pastimes, and conversations—into stress, satisfaction, and childhood belief adjustments.

**Why it matters:** Keeps [`game-vitality`](../game-vitality/README.md) focused on developmental and aging curves while higher-level social and leisure events shape a person's ongoing emotional condition and formative attitudes.

## Responsibilities

- **Experience interpretation:** Subscribes to physiological, social, and pastime events and dispatches `game_vitality::Condition` updates for stress relief, satisfaction, or trauma.
- **Childhood attitude shaping:** Adjusts a child's `sharing`, `authority`, and `honesty` beliefs in [`game-relationships`](../game-relationships/README.md) gradually after conversations with trusted, affectionate adults.

## Boundaries

- **Event translator only:** Owns no persistent resource of its own; [`game-vitality`](../game-vitality/README.md) stores stress/satisfaction and [`game-relationships`](../game-relationships/README.md) stores social beliefs. Never alters genetic potential.

## How It Works

- **Asymmetric conversation feelings:** A completed conversation can calm one participant while stressing the other, depending on each person's own directed trust, affection, fear, and grievance.
- **Relationship-gated childhood learning:** Parents do not copy beliefs into children at birth; children shift attitudes only through actual conversations weighted by their empathy and trust in the speaker.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`wellbeing`).
- **Requests & Events:** Subscribes to `game_physiology::Ate`, `game_physiology::Fed`, `game_physiology::Damaged`, `game_social_life::Conversed`, and `game_pastimes::Completed`.
- **Callers:** Installed by [`game-assembly`](../game-assembly/README.md) and triggered automatically by subscribed events.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-vitality`](../game-vitality/README.md), [`game-physiology`](../game-physiology/README.md), [`game-social-life`](../game-social-life/README.md), [`game-relationships`](../game-relationships/README.md), [`game-pastimes`](../game-pastimes/README.md).
- **Testing:** `cargo test -p game-wellbeing` verifies conversation stress/relief asymmetry, pastime satisfaction, and childhood belief influence.
