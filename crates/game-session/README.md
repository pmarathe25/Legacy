# game-session

`game-session` runs a `game-assembly::Simulation` on a dedicated background thread, processing player commands between bounded simulation advances and coalescing routine progress updates.

**Why it matters:** A real-time or interactive client needs responsive command submission and view inspection without blocking on long simulation ticks or flooding the UI queue with intermediate progress frames.

## Responsibilities

- **Dedicated host thread (`Session`):** Spawns a worker thread owning `Simulation` and communicates via typed request/response channels.
- **Interleaved advancement:** Advances the simulation in bounded steps (`Advance`), checking for pending player commands (`Submit`), `Observe`, or `Save` requests between steps.
- **Progress coalescing:** Overwrites unread routine progress notifications in a single slot while preserving explicit command results, terminal stops, views, and saves in order.

## Boundaries

- **Threading policy only:** Owns concurrency and message coalescing; [`game-assembly`](../game-assembly/README.md) and [`game-sim`](../game-sim/README.md) own command execution and simulation state.
- **Deterministic advancement:** Never mutates simulation state outside the single host thread.

## How It Works

- **Bounded slice loop:** Long advance targets are broken into small step budgets so incoming player commands execute immediately after the current atomic phase commits.
- **Stop condition handling:** Halts advancement early if the host reports a player-relevant stop (such as tactical battle input or ruler loss) and emits a terminal progress event.

## Game Integration

- **Plugin / Entry Point:** Application threading adapter (`Session`); registers no `game-sim` plugin.
- **Requests & Events:** Processes session messages (`Submit`, `Advance`, `Observe`, `Save`, `Shutdown`).
- **Callers:** Designed for interactive frontends and async/threaded client runners.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-api`](../game-api/README.md), [`game-assembly`](../game-assembly/README.md), [`game-sim`](../game-sim/README.md).
- **Testing:** `cargo test -p game-session` verifies command interleaving, progress coalescing, and clean thread shutdown.
