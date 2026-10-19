# game-observations

Event adapter that translates domain simulation outcomes into personal `game-inquiry::Witness` episodes and installs food inquiry actions.

**Why it matters:** Keeping domain crates like agriculture, transport, and workshops decoupled from cognitive memory requires an adapter layer that turns raw world events into structured personal observations.

## Responsibilities

- **Domain Event Adaptation:** Listens to travel time/load (`game-transport`), crafting duration/practice (`game-workshop`, `game-skills`), crop harvests/weather (`game-agriculture`, `game-environment`), food effects (`game-vitality`), thermal harm (`game-exposure`), and storage conditions (`game-food-storage`, `game-preservation`).
- **Episode Dispatch:** Packages those outcomes into structured numerical `game-inquiry::Witness` requests for the acting or observing character.
- **Food Inquiry Wiring:** Installs and re-exports `game-food-inquiry`'s `Inspect` and `Experiment` actions into the live simulation.

## Boundaries

- **Adapter Only:** Owns no long-term cognitive memory (owned by `game-inquiry`) and no domain simulation state.
- **No Direct Inference:** Delegates all hypothesis generation to `game-inquiry` and `game-reasoning`.

## How It Works

- **Event Subscriptions:** Registers lightweight handlers on domain completion events and translates domain quantities (such as load weight, ambient temperature, skill level, and yield) into `game-reasoning` variables.
- **Perception Filtering:** Ensures only characters present and capable of perceiving the outcome receive a `Witness` episode.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers domain event adapters and installs `game-food-inquiry` handlers in `game-assembly`.
- **Requests & Events:** Consumes events from transport, workshop, agriculture, exposure, vitality, and storage crates; emits `game-inquiry::Witness` requests.
- **Callers:** Composed at the top level by `game-assembly` and scenario fixtures.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-reasoning`, `game-knowledge`, `game-choice`, `game-cargo`, `game-objects`, `game-food-storage`, `game-preservation`, `game-environment`, `game-transport`, `game-physiology`, `game-vitality`, `game-agriculture`, `game-workshop`, `game-skills`, `game-exposure`, `game-inquiry`, `game-food-inquiry`.
- **Testing:** Integration tests in `tests/` verify that traveling, crafting, harvesting, eating, and inspecting food stores generate accurate `Witness` episodes and drive empirical learning.
