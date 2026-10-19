# game-invocation

Temporary channeling of past-life skills and memories, escalating physiological strain, and loss-of-control thresholds.

**Why it matters:** Calling upon the skills and memories of past incarnations (`game-lineage`) gives a mortal ruler extraordinary power—at the price of mounting physiological strain and the risk of losing conscious control.

## Responsibilities

- **Temporary Past-Life Recall:** Grants the current ruler temporary access to selected skills and memories from completed past lives in `game-lineage`.
- **Consent Checkpoints & Cost Thresholds:** Pauses simulation advancement at authored strain thresholds so the player must explicitly consent before pushing an invocation deeper.
- **Loss of Control:** Blocks player input and prevents voluntary dismissal once an accepted invocation crosses into loss of control, while continuing to apply ongoing physiological strain (`game-physiology`).
- **Combat Control Hook:** Serves `game-conflict`'s control query and prompts combat to resume automatic turns when the player ruler loses control mid-battle.

## Boundaries

- **Temporary Recall Only:** Channeled past-life memories and skills revert when the invocation ends; they do not permanently overwrite the ruler's personal learning table.
- **No Combat Resolution Rules:** Contains no tactical combat math; only answers control queries for `game-conflict`.

## How It Works

- **Interval Strain Accumulation:** Tracks active invocation duration and committed cost intervals, applying strain to `game-physiology` and triggering consent warnings at configured thresholds.
- **Save-Safe Warnings:** Persists active invocation state, issued threshold warnings, and control-loss flags across save/load checkpoints.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers invocation state, strain timers, consent stops, and combat control hooks in `game-assembly`.
- **Requests & Events:** Exposes invocation start, threshold consent continuation, and dismissal requests; serves `game-conflict` control queries.
- **Callers:** Invoked via `game-api` / `game-cli` player commands; queried by `game-conflict`, `game-skills`, and `game-knowledge`.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-lineage`, `game-physiology`, `game-conflict`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-invocation`) cover temporary recall, threshold consent stops, loss-of-control input blocking, automatic combat turn resumption, and save/load consistency.
