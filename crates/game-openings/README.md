# game-openings

`game-openings` initializes a `game_sim::Host` from either a generated world or an authored scenario fixture and runs pre-control history warmup.

**Why it matters:** Bridges declarative scenario definitions (`game-scenarios`, `game-worldgen`) and live simulation state by dispatching feature-owned creation requests in a deterministic sequence and recording provenance in saves.

## Responsibilities

- **Opening resource (`Opening`):** Persists the scenario ID, seed, content fingerprints, resolved `game_scenarios::Definition`, symbolic-to-runtime ID map (`Binding`), and player handoff timestamp under `openings.state`.
- **World installation (`initialize`, `seed_world`):** Loads [`game-scenario-assets`](../game-scenario-assets/README.md), creates areas, roads, inventories, items, patches, plots, people, traits, relationships, agreements, institutions, and guardians via each owner's typed requests.
- **Live history warmup:** For `generated` starts, enables [`game-campaign`](../game-campaign/README.md) autonomy, advances the host for the configured warmup duration with all mortals autonomous, and then assigns a surviving mortal as the initial player ruler.

## Boundaries

- **Startup orchestration only:** Mutates only `openings.state` directly; all entity creation goes through the owning feature crates' requests.
- **Uncontrolled research path (`seed_world`):** Constructs the generated world at day 0 without warmup or player ruler assignment for [`tools/history`](../../tools/history/README.md) cohort studies.

## How It Works

- **Symbolic binding resolution:** Maps symbolic scenario keys (areas, people, inventories, plots, agreements, institutions) to allocated `game-core` IDs so cross-references wire up cleanly.
- **Unprotected warmup:** During generated history warmup, no character is immortalized or shielded; if a candidate dies, player selection chooses from actual survivors at handoff time.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`openings`, resource `openings.state`) and exposes `initialize`, `seed_world`, `scenarios`, and `default_scenario`.
- **Requests & Events:** Handles internal opening state setup and dispatches creation/configuration requests across all initialized feature owners.
- **Callers:** Called by [`game-assembly`](../game-assembly/README.md) during `Simulation::new`, `Simulation::with_content`, and `Simulation::uncontrolled`.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-content`](../game-content/README.md), [`game-scenarios`](../game-scenarios/README.md), [`game-scenario-assets`](../game-scenario-assets/README.md), [`game-worldgen`](../game-worldgen/README.md), [`game-campaign`](../game-campaign/README.md), and initialized feature crates.
- **Testing:** `cargo test -p game-openings` verifies fixture initialization, generated warmup handoff, symbolic binding, and save/load persistence.
