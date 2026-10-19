# game-campaign

`game-campaign` configures the full autonomous feature preset, founder genotypes, and adult age distributions for generated worlds.

**Why it matters:** Generated worlds need all emergent daily-life, biological, cultural, and social decision owners enabled together with human-scale timing, while authored test fixtures keep their minimal, controlled behavior unless explicitly opted in.

## Responsibilities

- **Campaign activation (`Start`):** Registers founder human genotypes (`game-heredity`), assigns varied initial adult ages and vitality (`game-vitality`), and enables autonomous subsystems (`game-provisioning`, `game-households`, `game-parenthood`, `game-exchange`, `game-works`, `game-preservation`, `game-food-storage`, `game-maturation`, `game-equipment`, `game-shelter`, `game-exposure`, `game-wayfaring`, `game-horticulture`, `game-care`, `game-livelihoods`, `game-pastimes`, `game-communities`, `game-grievances`, `game-names`, `game-communication`, `game-botany`).
- **Calendar & cadence alignment:** Sets the campaign calendar epoch and configures half-day review cadences suited for multi-year histories.

## Boundaries

- **Preset composer only:** Owns no persistent gameplay state or alternate simulation loop; simply dispatches typed configuration/enable requests to lower feature owners.

## How It Works

- **Single transaction bootstrap:** Executing `game_campaign::Start` at world creation wires up founder biology, wild botany stands, and autonomous decision hooks before the first simulation tick advances.
- **Fixture isolation:** Scenarios that do not call `Start` leave optional autonomous decision owners inactive so isolated mechanic tests stay predictable.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`campaign`).
- **Requests & Events:** Handles `Start` (`campaign.start`).
- **Callers:** Invoked by [`game-openings`](../game-openings/README.md) when initializing `generated` worlds and `seed_world`.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-physiology`](../game-physiology/README.md), and the enabled campaign feature crates.
- **Testing:** `cargo test -p game-campaign` verifies multi-feature autonomous startup and survival without importing `game-assembly`.
