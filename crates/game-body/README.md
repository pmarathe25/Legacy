# game-body

`game-body` is a pure library defining bodily state values (`Body`, `BodyView`), physiological rule profiles (`Rules`), and constant-time hunger, fatigue, and recovery projections.

**Why it matters:** Separating pure bodily arithmetic from hosted storage (`game-soma`) and eating/resting actions (`game-physiology`) lets decision planners and dead-row serializers project body state forward in $O(1)$ time without mutating host state.

## Responsibilities

- **Body representation (`Body`, `BodyView`):** Tracks hunger, fatigue, health, maximum health, mortality (`Mortal` vs. `Immortal`), awake/resting state, and fractional sub-tick accumulators.
- **Rule schema (`Rules`, `load`):** Parses physiological rate profiles from `content/physiology/*.toml` (hunger gain, fatigue gain, rest recovery, starvation damage, and immortal recovery thresholds).
- **Constant-time evolution (`advance`, `forecast`):** Projects a `Body` across elapsed `SimDuration` steps and computes exact deadlines until hunger or fatigue crosses an interruption threshold.

## Boundaries

- **Owner-controlled storage:** Owns no plugin or resource storage; [`game-soma`](../game-soma/README.md) stores authoritative bodies and registers the optional generic nourishment batch under its own owner. [`game-physiology`](../game-physiology/README.md) orchestrates actions and ticks.

## How It Works

- **Fractional remainder preservation:** Carries sub-unit hunger and recovery remainders across arbitrary step sizes so irregular advance intervals produce the exact same trajectory as single-step ticks.
- **Deadline forecasting:** Computes when an active person's hunger will hit urgent thresholds so `game-characters` can sleep until that exact timestamp instead of polling.

## Game Integration

- **Plugin / Entry Point:** Pure library crate (`Body`, `BodyView`, `Rules`, `load`); re-exported by [`game-physiology`](../game-physiology/README.md).
- **Requests & Events:** None.
- **Callers:** Used by [`game-soma`](../game-soma/README.md), [`game-metabolism`](../game-metabolism/README.md), [`game-physiology`](../game-physiology/README.md), and [`game-characters`](../game-characters/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-content`](../game-content/README.md), [`game-sim`](../game-sim/README.md).
- **Testing:** `cargo test -p game-body` (and `cargo test -p game-physiology`) verifies constant-time projection equivalence and rule validation.
