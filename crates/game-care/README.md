# game-care

`game-care` drives voluntary medical treatment of injured neighbors, personal appeals for medical help, orphan fostering, and dependent feeding coordination.

**Why it matters:** Injured people and helpless children survive only when capable neighbors or family members notice their need and choose to spend their own time, medicine, and food to help.

## Responsibilities

- **Care assignments & appeals (`Care`):** Tracks active caregiver–patient assignments (preventing duplicate treatment), received medical help appeals, and rotating neighbor inspection cursors (`care.state`).
- **Voluntary medical treatment:** Evaluates self-treatment and up to 64 co-located injured neighbors using empathy, altruism, learned care value, affection, and grievance, invoking [`game-health::Treat`](../game-health/README.md).
- **Medical help appeals:** Lets an injured person without medicine approach a perceived local helper to request aid; the recipient records the appeal and weighs it against their own concern and competing needs.
- **Orphan fostering & nurture composition:** Detects locally witnessed caregiver deaths to propose `game_households::Foster`, and installs [`game-nurture`](../game-nurture/README.md) requests for dependent feeding.

## Boundaries

- **Decision & coordination owner:** Orchestrates who chooses to help whom; [`game-health`](../game-health/README.md) owns physical treatment, [`game-nurture`](../game-nurture/README.md) plans dependent food errands, and [`game-households`](../game-households/README.md) owns ward memberships.

## How It Works

- **Gratitude & learning:** When `game_health::Treated` completes between two people, the patient's trust and affection toward the caregiver increase and the caregiver's learned concept value for care strengthens.
- **Strictly local awareness:** Fostering and care consider only co-located people and locally witnessed deaths—never an omniscient global orphan or patient list.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`care`, resource `care.state`), including [`game-nurture`](../game-nurture/README.md) handlers.
- **Requests & Events:** Handles `Enable`, `Consider`, `Appeal`, and nurture `Assist` requests; registers with [`game-choice`](../game-choice/README.md); subscribes to `game_health::Treated` and `game_physiology::Died`.
- **Callers:** Enabled by [`game-campaign`](../game-campaign/README.md) and [`game-openings`](../game-openings/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-storage`](../game-storage/README.md), [`game-choice`](../game-choice/README.md), [`game-nurture`](../game-nurture/README.md), [`game-households`](../game-households/README.md), [`game-health`](../game-health/README.md), [`game-physiology`](../game-physiology/README.md), [`game-vitality`](../game-vitality/README.md), [`game-traits`](../game-traits/README.md), [`game-relationships`](../game-relationships/README.md), [`game-social-life`](../game-social-life/README.md), [`game-knowledge`](../game-knowledge/README.md), [`game-food-memory`](../game-food-memory/README.md), [`game-cargo`](../game-cargo/README.md), [`game-property`](../game-property/README.md), [`game-objects`](../game-objects/README.md), [`game-transport`](../game-transport/README.md), [`game-characters`](../game-characters/README.md), [`game-lineage`](../game-lineage/README.md), [`game-diagnostics`](../game-diagnostics/README.md).
- **Testing:** `cargo test -p game-care` verifies voluntary treatment, grievance refusal, help appeals, gratitude updates, orphan fostering, and save/load consistency.

Household care commitments are voluntary and many-to-many. The `Care.ongoing` table reserves one currently performed medical procedure per patient; it does not appoint a permanent caregiver or grant authority.
