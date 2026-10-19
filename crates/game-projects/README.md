# game-projects

`game-projects` manages construction proposals, physical material contributions, timed building labor, and completed operating sites.

**Why it matters:** Building structures requires staged, in-kind material deliveries and timed labor from one or more contributors rather than instant resource deduction.

## Responsibilities

- **Projects resource (`Projects`):** Stores proposed, funded, in-progress, and completed construction projects and their explicit contributor records (`projects.state`).
- **Material contributions (`Contribute`):** Transfers required building materials from a contributor's local inventory into the project site via `game-cargo`.
- **Timed construction (`Build`):** Occupies the builder's canonical action slot for the project's labor duration, consumes staged materials on completion, and credits construction skill via `game-skills`.

## Boundaries

- **Explicit contribution records:** Tracks who contributed what materials and labor without inventing automatic corporate ownership; [`game-property`](../game-property/README.md) and [`game-shelter`](../game-shelter/README.md) handle site claims and indoor shelter behavior.

## How It Works

- **Staged lifecycle:** A project transitions from `Proposed` -> `Funded` (once all required `GoodId` quantities are physically contributed) -> `Building` (during timed `Build` labor) -> `Completed`.
- **Interruption safety:** Cancelling `Build` stops construction labor cleanly without destroying contributed materials.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`projects`, resource `projects.state`).
- **Requests & Events:** Handles `Propose`, `Contribute`, `Build`; emits `Completed`.
- **Callers:** Bound to `Command::Build` in [`game-assembly`](../game-assembly/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-storage`](../game-storage/README.md), [`game-sim`](../game-sim/README.md), [`game-cargo`](../game-cargo/README.md), [`game-property`](../game-property/README.md), [`game-transport`](../game-transport/README.md), [`game-physiology`](../game-physiology/README.md), [`game-skills`](../game-skills/README.md).
- **Testing:** `cargo test -p game-projects` verifies partial material contributions, timed construction, skill attribution, cancellation, and save/load state.
