# game-digestion

`IngestMany` computes distinct personal stomach updates in parallel from the same
committed body state, then installs them in input order. Duplicate recipients or
scalar ingestion hooks require ordered requests instead.

`game-digestion` tracks lazy stomach fullness after swallowing food and computes satiety discomfort relative to expressed body size.

**Why it matters:** Without stomach capacity, a character seeking minor nutrient or mood benefits could binge-eat unlimited portions back-to-back; fullness adds natural, body-scaled satiety pressure without a rigid cooldown timer.

## Responsibilities

- **Stomach records (`Digestion`):** Stores each person's swallowed food volume and last-updated timestamp (`physiology.digestion`).
- **Lazy decay projection (`fullness`, `discomfort`):** Projects remaining stomach volume forward using the physiological clock, expressed energy demand, and body size from `game-vitality`.
- **Swallow updates (`Swallow`):** Increases fullness when actual portions are eaten in `game-physiology`.

## Boundaries

- **Soft discomfort, not a hard lock:** Increases effort/discomfort on further meal proposals rather than forbidding eating outright; owns no periodic per-person tick jobs.

## How It Works

- **Event-driven lazy evaluation:** Stomach volume is updated only when a person eats and projected on demand during meal evaluation, adding zero background scheduler overhead.
- **Size-relative capacity:** A portion fills an infant's stomach much faster than a large adult's, matching `game-vitality` body size.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (resource `physiology.digestion`).
- **Requests & Events:** Handles `Swallow`; subscribes to `game_physiology::Died`.
- **Callers:** Updated by [`game-physiology`](../game-physiology/README.md) during eating/feeding and queried during meal evaluation in [`game-characters`](../game-characters/README.md) and [`game-provisioning`](../game-provisioning/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-storage`](../game-storage/README.md), [`game-soma`](../game-soma/README.md), [`game-vitality`](../game-vitality/README.md).
- **Testing:** Exercised by `cargo test -p game-physiology --test satiety` and autonomous dietary tests in `game-characters`.
