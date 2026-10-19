# game-health

`game-health` executes timed medical treatments that consume physical medicine and caregiver labor to heal an injured patient.

**Why it matters:** Healing wounds requires an actual co-located caregiver, accessible medical supplies, and uninterrupted treatment time rather than automatic regeneration.

## Responsibilities

- **Treatment rules (`TreatmentRules`):** Stores authored medicine object ID, required dose, treatment duration, and healing amount (`health.rules`) loaded from `content/health/*.toml`.
- **Timed treatment (`Treat`):** Reserves medicine in the caregiver's accessible local inventory (`game-cargo`), occupies the caregiver's canonical action slot, and restores patient health via `game-physiology::Heal` upon completion.

## Boundaries

- **Physical action vs. care motivation:** Owns the `Treat` execution rules and medicine consumption; [`game-care`](../game-care/README.md) decides when autonomous characters volunteer to treat someone or ask for help.

## How It Works

- **Completion re-verification:** When the treatment timer finishes, `game-health` rechecks that the patient is still alive and in the same area before consuming the reserved medicine and applying healing.
- **Patient freedom:** Only the caregiver's action slot is occupied during `Treat`; if the patient walks away mid-treatment, completion fails cleanly without healing from afar.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`health`, resource `health.rules`) and provides `load(&mut Store, key, &Objects)`.
- **Requests & Events:** Handles `Configure` and `Treat`; emits `Treated`.
- **Callers:** Bound to `Command::Treat` in `game-assembly` and invoked autonomously by [`game-care`](../game-care/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-content`](../game-content/README.md), [`game-objects`](../game-objects/README.md), [`game-sim`](../game-sim/README.md), [`game-physiology`](../game-physiology/README.md), [`game-cargo`](../game-cargo/README.md), [`game-property`](../game-property/README.md), [`game-transport`](../game-transport/README.md).
- **Testing:** `cargo test -p game-health` verifies medicine reservation, timed healing, patient departure failure, and cancellation.
