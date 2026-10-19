# game-exposure

`game-exposure` tracks per-person body temperature equilibrium and applies fatigue, stress, and thermal injury from severe cold or heat.

**Why it matters:** Weather, wet rain, clothing insulation, shelter, and biological hardiness combine into concrete physiological consequences over 6-hour thermal updates.

## Responsibilities

- **Thermal state (`Exposure`):** Tracks each person's integer core temperature offset and wetness across 6-hour ticks (`exposure.state`).
- **Equilibrium calculation:** Integrates local area climate (`game-environment`), equipped clothing warmth/weatherproofing (`game-equipment`), indoor shelter insulation and hearth heat (`game-shelter`), and expressed body size and hardiness (`game-vitality`).
- **Harm & stress application:** Applies extra fatigue and stress under mild thermal discomfort and inflicts bodily damage (`game-physiology`) under severe hypothermia or heatstroke.

## Boundaries

- **Thermal effects only:** Computes temperature drift and calls `game-physiology` / `game-vitality` requests to apply damage or stress; does not own body health or clothing items.

## How It Works

- **Thermal inertia:** Core temperature approaches the effective ambient equilibrium gradually over 6-hour steps rather than jumping instantaneously on area changes.
- **Bidirectional hardiness buffer:** Expressed hardiness widens the safe comfort band against both cold and heat without generating heat of its own, while heavy clothing in hot weather can cause overheating.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`exposure`, resource `exposure.state`).
- **Requests & Events:** Handles `Enable` and periodic thermal tick requests; subscribes to `game_physiology::Died`.
- **Callers:** Enabled by [`game-campaign`](../game-campaign/README.md) and read by [`game-comfort`](../game-comfort/README.md) to motivate clothing and shelter choices.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-storage`](../game-storage/README.md), [`game-physiology`](../game-physiology/README.md), [`game-environment`](../game-environment/README.md), [`game-transport`](../game-transport/README.md), [`game-equipment`](../game-equipment/README.md), [`game-shelter`](../game-shelter/README.md), [`game-vitality`](../game-vitality/README.md).
- **Testing:** `cargo test -p game-exposure` verifies cold/heat drift, rain wetness penalties, clothing/shelter protection, overheating, and hypothermia injury.
