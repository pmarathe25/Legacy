# game-scenario-assets

`game-scenario-assets` loads, cross-validates, and installs all runtime content catalogs and rule profiles referenced by a `game-scenarios::Definition`.

**Why it matters:** Validating that every object, trait, crop, recipe, fauna species, and inventory mass in a scenario is legal *before* mutating the simulation host keeps [`game-openings`](../game-openings/README.md) focused on entity placement and startup flow.

## Responsibilities

- **Catalog & rule bundle (`Assets`):** Loads `Objects`, `Fauna`, `Traits`, `Crops`, `Workshop` recipes, `Livelihoods`, `Pastimes`, and the scenario's selected `Physiology`, `Characters`, `Environment` (global and per-area regional climates), `Health`, `SocialLife`, and `Invocation` rule profiles.
- **Cross-catalog validation (`Assets::load`):** Verifies that all person trait profiles are within authored bounds, starting inventory masses fit within their declared capacities, plots reference valid crops, and patches reference valid objects.
- **Batch installation (`Assets::install`):** Dispatches each feature's `Install` / `Configure` request onto `game_sim::Host`.

## Boundaries

- **Asset resolution vs. entity placement:** Resolves and installs static catalogs and rules; [`game-openings`](../game-openings/README.md) creates the actual areas, people, inventories, and live history warmup.

## How It Works

- **Fail-fast preflight:** `Assets::load` performs all content store reads and capacity/reference checks in memory before `Assets::install` touches the host.
- **Per-person inventory scaling:** Multiplies `per_person` inventory goods by the expanded scenario actor count when verifying container mass limits.

## Game Integration

- **Plugin / Entry Point:** Helper library (`Assets::load`, `Assets::install`); registers no `game-sim` plugin of its own.
- **Requests & Events:** Dispatches `Install` and `Configure` requests to `game-objects`, `game-fauna`, `game-traits`, `game-agriculture`, `game-workshop`, `game-livelihoods`, `game-pastimes`, `game-physiology`, `game-characters`, `game-environment`, `game-health`, `game-social-life`, and `game-invocation`.
- **Callers:** Called by [`game-openings`](../game-openings/README.md) during world initialization.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-content`](../game-content/README.md), [`game-scenarios`](../game-scenarios/README.md), [`game-objects`](../game-objects/README.md), [`game-fauna`](../game-fauna/README.md), [`game-traits`](../game-traits/README.md), [`game-agriculture`](../game-agriculture/README.md), [`game-workshop`](../game-workshop/README.md), [`game-physiology`](../game-physiology/README.md), [`game-characters`](../game-characters/README.md), [`game-environment`](../game-environment/README.md), [`game-health`](../game-health/README.md), [`game-invocation`](../game-invocation/README.md), [`game-social-life`](../game-social-life/README.md), [`game-livelihoods`](../game-livelihoods/README.md), [`game-pastimes`](../game-pastimes/README.md).
- **Testing:** Exercised by `cargo test -p game-openings` and `cargo test -p game-assembly --test content`.
