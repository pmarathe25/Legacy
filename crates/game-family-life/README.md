# game-family-life

Voluntary family bonds, co-parent and companion ties, newborn social initialization, and capability-based independence.

**Why it matters:** Family structures extend beyond a single primary household record. `game-family-life` manages broader co-parent and family bonds, initializes newborn social identities at birth, and transitions growing children toward independence based on actual capability.

## Responsibilities

- **Extended Family & Co-Parent Bonds:** Tracks voluntary family, companion, and co-parent relationships alongside primary `game-households` records.
- **Newborn Initialization:** Initializes a newborn's personal inventory pack (`game-objects`, `game-cargo`), social identity (`game-social-life`), name (`game-names`), lineage links (`game-lineage`), and decision state (`game-characters`) at birth.
- **Passenger Dependent Travel:** Carries willing or helpless dependents during caregiver departures within physical load limits (`game-transport`), while allowing carried passengers to rest and emit distress signals.
- **Capability-Based Independence:** Adjusts shared care and separation as dependents develop physical and cognitive capabilities (`game-vitality`).

## Boundaries

- **No Biological Reproduction:** Gestation, genetics, and developmental biology remain in `game-lifecycle`, `game-heredity`, and `game-vitality`.
- **Backwards-Compatible Household State:** Preserves primary household records in `game-households` for existing consumers.

## How It Works

- **Birth Event Hooks:** Subscribes to `game-lifecycle` birth events to wire up packs, names, lineage, and caregiver links in a single atomic transaction.
- **Choice Integration:** Proposes family care, reunion, and accompanied travel actions through `game-choice`.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers family bond storage, birth initialization hooks, and `game-choice` providers in `game-assembly`.
- **Requests & Events:** Consumes birth, movement, and mortality events; dispatches household, transport, and naming requests.
- **Callers:** Composed alongside `game-households`, `game-lifecycle`, and `game-transport` in `game-assembly` and `game-campaign`.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-diagnostics`, `game-choice`, `game-households`, `game-names`, `game-vitality`, `game-food-memory`, `game-objects`, `game-provisioning`, `game-characters`, `game-lifecycle`, `game-physiology`, `game-social-life`, `game-relationships`, `game-traits`, `game-cargo`, `game-property`, `game-transport`, `game-knowledge`, `game-lineage`.
- **Testing:** Tested via `cargo test -p game-campaign -p game-transport` covering newborn setup, passenger travel load limits, and capability-driven independence.

Existing companions can negotiate a different kind of bond. The planner excludes
only the current kind, rather than freezing every existing relationship forever.
Each participant must prefer a proposed change enough to outweigh their expressed
rigidity; the other participant can refuse. Reproductive drive contributes to
co-parent courtship as well as later reproductive choices. Moving from an exclusive
bond to a nonexclusive one clears the corresponding primary-household partnership
through the household owner's request, while retaining the agreed family bond.

Care uses the household owner’s many-to-many commitments. `ShareCare` can add or withdraw help without displacing another caregiver. Birth creates kinship, not authority or a care assignment. Dead partners cease to block new voluntary relationships. Romantic attraction is supplied by `game-attraction`; companionship and care do not use romantic physical tastes.

Bond assent checks apply cheap eligibility predicates before subjective appraisal:
living adulthood, exclusivity conflicts, and close kinship can reject an impossible
choice immediately. Failing the desire threshold also avoids calculating the old
bond's appeal. These calculations are pure; provider order, observation effects,
offer hooks, acceptance rules, and action timing remain unchanged. Predicates are
not hoisted across offer requests. See `tools/performance/REVIEWS.md`.
