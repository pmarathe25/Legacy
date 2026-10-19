# game-parenthood

`game-parenthood` evaluates voluntary reproductive decisions between household partners based on mutual attachment, current condition, and existing child-care demands.

**Why it matters:** Birth rates in long simulations emerge from partners' actual well-being, affection, kinship values, and current caregiving load rather than demographic targets or forced replacement rules.

## Responsibilities

- **Mutual desire evaluation (`Consider`):** Checks whether co-located household partners both desire a child given their affection, trust, learned kinship value, hunger, fatigue, stress, fear, grievance, and current dependent children.
- **Conception trigger:** Submits a reproductive proposal via [`game-choice`](../game-choice/README.md) (or invokes [`game-lifecycle::Conceive`](../game-lifecycle/README.md)) when both partners consent.

## Boundaries

- **Voluntary intention only:** Owns no persistent population target or fertility state; [`game-households`](../game-households/README.md) owns partnerships and dependent rosters, and [`game-lifecycle`](../game-lifecycle/README.md) enforces biological fertility and gestation.

## How It Works

- **Dependent load sensitivity:** Existing dependent children impose a caregiving load that reduces desire for another child until older children mature into independent adults.
- **Bilateral consent:** Both partners must independently score positive willingness at the same place and time; partnership alone never implies automatic conception.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`parenthood`).
- **Requests & Events:** Handles `Consider`; registers with [`game-choice`](../game-choice/README.md) and subscribes to `game_households::Partnered`.
- **Callers:** Enabled in generated worlds via [`game-campaign`](../game-campaign/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-choice`](../game-choice/README.md), [`game-households`](../game-households/README.md), [`game-lifecycle`](../game-lifecycle/README.md), [`game-physiology`](../game-physiology/README.md), [`game-relationships`](../game-relationships/README.md), [`game-vitality`](../game-vitality/README.md), [`game-lineage`](../game-lineage/README.md), [`game-characters`](../game-characters/README.md).
- **Testing:** Exercised by `cargo test -p game-campaign` for voluntary conception, refusal under high care load or distress, and save/load continuity.
