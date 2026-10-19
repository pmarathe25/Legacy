# game-cli

`game-cli` provides the interactive and scripted command-line interface for running Legacy scenarios, inspecting player views, and saving or loading sessions.

**Why it matters:** Gives players and developers a lightweight text adapter over the exact same `game-api` commands and `PlayerView` observations that a future graphical renderer will use.

## Responsibilities

- **CLI argument parsing:** Handles `--list`, `--scenario <id>`, `--seed <u64>`, `--workers <n>`, `--script <path>`, `--content <dir>`, and repeated `--mod <dir>` overlays.
- **Text command parser (`parse`):** Translates human-friendly commands (`status`, `run`, `move`, `eat`, `rest`, `look`, `observe`, `observe-patch`, `tell`, `talk`, `read-work`, `show-work`, `craft`, `plant`, `pursue`, `sow`, `harvest`, `cancel`, `consent`, `dismiss`, `save`, `load`, `debug`, `quit`, and `command <JSON>`) into `Input` and `game_api::Command`.
- **Allocator selection:** Enables `mimalloc` by default for binary builds (can be disabled via `--no-default-features`).

## Boundaries

- **Presentation & parsing only:** Contains no gameplay transitions or privileged state mutations; all gameplay commands go through `Simulation::submit` and `Simulation::observe`.
- **Explicit debug mode:** `debug` is the only command that inspects `Simulation::debug_host()`; `status` strictly formats `PlayerView`.

## How It Works

- **Millisecond conversion:** `run <seconds>` multiplies integer seconds by `1,000` into `SimDuration` and advances the simulation in bounded steps.
- **Full API fallback:** `command <JSON>` deserializes any `game_api::Command` variant directly, so every API action is testable from scripts even without a dedicated shorthand verb.

## Game Integration

- **Plugin / Entry Point:** Binary (`src/main.rs`) and parser library (`Input`, `parse` in `src/lib.rs`).
- **Requests & Events:** Submits `game_api::Command` via `game_assembly::Simulation`.
- **Callers:** Invoked directly by users (`cargo run -p game-cli -- ...`) and CLI integration tests.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-content`](../game-content/README.md), [`game-api`](../game-api/README.md), [`game-assembly`](../game-assembly/README.md), [`game-sim`](../game-sim/README.md).
- **Testing:** `cargo test -p game-cli` verifies command parsing, script execution, and equivalence with direct `Simulation` calls.
