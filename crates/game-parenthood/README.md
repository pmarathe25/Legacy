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
- **Requests & Events:** Handles `Consider` and partner-specific `ConsiderWith`; registers with [`game-choice`](../game-choice/README.md) and subscribes to `game_households::Formed`.
- **Callers:** Enabled in generated worlds via [`game-campaign`](../game-campaign/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-choice`](../game-choice/README.md), [`game-households`](../game-households/README.md), [`game-lifecycle`](../game-lifecycle/README.md), [`game-physiology`](../game-physiology/README.md), [`game-relationships`](../game-relationships/README.md), [`game-vitality`](../game-vitality/README.md), [`game-lineage`](../game-lineage/README.md), [`game-characters`](../game-characters/README.md).
- **Testing:** Exercised by `cargo test -p game-campaign` for voluntary conception, refusal under high care load or distress, and save/load rigidity.

Reproductive drive is an inherited personality/physiological disposition expressed
through current bodily health. It competes with other needs and the anticipated care
burden; fear, grievance, personal values, and either partner's refusal still matter.
Successful conception produces a drive-dependent subjective reward in vitality.

All accepted co-parent bonds are considered, rather than only the first one. Proposals
exclude incompatible, distant, infertile, and recovering pairs; the selected partner
and both partners' willingness are checked again when the action finishes. No birth
quota, minimum population, or guaranteed surviving descendant is imposed.

Subjective attraction, heard compatibility, learned kinship values, empathy, condition, anticipated care burden, and physiological drive contribute separately. Both participants must want conception. Another partner’s pregnancy is not a physiological ban on reproduction. Reproductive drive does not supply a universal courtship preference.
