# game-nurture

`game-nurture` plans and executes voluntary food acquisition, travel, and feeding errands for dependent children and incapacitated wards.

**Why it matters:** Infants and young children cannot forage or walk roads on their own; caregivers must perceive their hunger cues, fetch or harvest food without eating it themselves, carry it back, and serve them.

## Responsibilities

- **Dependent food errands (`Assist`):** Evaluates food needs for both assigned household wards and co-located dependent biological children (even if another parent is the primary assigned caregiver).
- **Multi-stage acquisition & delivery:** Acquires food via [`game-provisioning`](../game-provisioning/README.md) (comparing ready stores against harvest/travel time), carries it along known roads to the beneficiary's remembered location, and invokes `game_physiology::Serve`.
- **Perceived need estimation:** Uses [`game-communication`](../game-communication/README.md) infant cries, nonverbal cues, and elapsed-time anticipation rather than reading the child's private hunger integer.

## Boundaries

- **Stateless policy module:** Owns no independent `game-sim` plugin or resource; registers its request handlers inside [`game-care`](../game-care/README.md) and persists delivery intentions in [`game-households`](../game-households/README.md).

## How It Works

- **Imperfect caregiver perception:** When communication is enabled, a successful serving reduces the caregiver's perceived need estimate by expected relief, while a declined offer gives observable feedback—allowing caregivers to help without an omniscient hunger oracle.
- **Abandonment on missing ward:** If a returning caregiver arrives and finds the beneficiary is no longer at the remembered area, or if the caregiver's own urgent needs supervene, the delivery intention ends cleanly.

## Game Integration

- **Plugin / Entry Point:** Registered by [`game-care::plugin`](../game-care/README.md) (`register(&mut Builder)`).
- **Requests & Events:** Handles `Assist` and step-continuation callbacks; submits proposals via [`game-choice`](../game-choice/README.md).
- **Callers:** Invoked through [`game-care`](../game-care/README.md) and [`game-households`](../game-households/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-choice`](../game-choice/README.md), [`game-communication`](../game-communication/README.md), [`game-nutrition`](../game-nutrition/README.md), [`game-households`](../game-households/README.md), [`game-provisioning`](../game-provisioning/README.md), [`game-characters`](../game-characters/README.md), [`game-physiology`](../game-physiology/README.md), [`game-vitality`](../game-vitality/README.md), [`game-traits`](../game-traits/README.md), [`game-relationships`](../game-relationships/README.md), [`game-cargo`](../game-cargo/README.md), [`game-objects`](../game-objects/README.md), [`game-property`](../game-property/README.md), [`game-knowledge`](../game-knowledge/README.md), [`game-transport`](../game-transport/README.md), [`game-lineage`](../game-lineage/README.md).
- **Testing:** Exercised by `cargo test -p game-care`, `cargo test -p game-households`, and `cargo test -p game-campaign`.

Food-specific evidence now survives competing pain or rest cues. Observed feeding and hunger intervals refine a personal appetite estimate. Caregivers can restart an interrupted delivery toward the last observed location, including after saving, without learning a remote child's true location. Nourishment can satisfy an ongoing need signal; acceptance weighs the recipient's appetite, preferences, and competing commitment.

Care urgency now includes anticipated consequences of personally perceived need, with reasoning and foresight controlling that anticipation. This allows a concerned caregiver to interrupt an absorbing interest before a dependent is harmed. Altruism, compassion, relationships, learned duty, personal needs, and mistaken observations still affect the choice; an indifferent caregiver can decline care.
