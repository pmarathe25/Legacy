# game-households

Adult partnerships, dependent caregiver records, fostering (`Foster`), and household care reviews.

**Why it matters:** Children cannot survive alone, and adults form enduring domestic bonds. `game-households` manages consenting adult partnerships, assigns caregivers to newborns, and coordinates fostering when a parent dies.

## Responsibilities

- **Consenting Partnerships (`Form`):** Forms partnerships between co-located adults when both express sufficient affection and trust without fear or grievance (`game-relationships`), excluding parent–child and sibling pairs.
- **Caregiver Records & Fostering (`Tend`, `Foster`):** Tracks primary caregivers for dependents, transfers care to a surviving partner on bereavement, and validates local `Foster` requests when willing adults adopt orphaned children.
- **Dependent Transport & Reunion:** Coordinates carrying young dependents on a caregiver's journeys (`game-transport`) and remembers where a separated child was last seen so caregivers can travel to reunite.
- **Autonomy Enrollment:** Enrolls children into ordinary decision autonomy at age six (`game-characters`) while continuing caregiver provisioning support until age eighteen.

## Boundaries

- **No Biological Gestation:** Biological eligibility, pregnancy, and birth events are owned by `game-lifecycle` and `game-parenthood`.
- **No Copied Ideology:** Children start with a blank slate of beliefs rather than inheriting their parents' opinions.

## How It Works

- **Local-First Care Priority:** Care reviews prioritize feeding and tending dependents already present with the caregiver before initiating reunion journeys for distant dependents, preventing travel loops from starving local infants.
- **Event & Demand Sync:** Updates care and food-demand indices when children are born, fostered, or reach adulthood, emitting `Fostered` events for observers.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers household partnership tables, caregiver assignments, and `Form` / `Tend` / `Foster` handlers in `game-assembly`.
- **Requests & Events:** Exposes `Form`, `Tend`, and `Foster` requests; emits partnership and `Fostered` events.
- **Callers:** Used by `game-family-life`, `game-parenthood`, `game-nurture`, `game-care`, and `game-provisioning`.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-objects`, `game-cargo`, `game-property`, `game-transport`, `game-physiology`, `game-vitality`, `game-lifecycle`, `game-parenthood`, `game-nurture`, `game-care`, `game-relationships`, `game-social-life`, `game-names`, `game-characters`, `game-provisioning`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-households`) cover partnership consent, incest exclusion, newborn initialization, dependent carrying, bereavement transfer, and fostering.
