# game-lifecycle

`game-lifecycle` manages biological conception, scheduled gestation, childbirth, newborn identity allocation, and birth provenance.

**Why it matters:** Multigenerational history requires every new person to be born from actual parents after a real gestation period in a physical location, never spawned out of thin air to maintain population numbers.

## Responsibilities

- **Lifecycle records (`Population`):** Stores each person's birth time, biological parents, simple genetic alleles, birth attendant, and active pregnancies (`lifecycle.population`).
- **Conception & gestation (`Conceive`):** Verifies that both parents are alive, mature, co-located, and within their fertile window (the first two-thirds of expressed natural lifespan), then schedules a timed birth.
- **Childbirth (`Deliver`):** Allocates a new `PersonId`, spawns the infant's body (`game-physiology`) and location (`game-transport`) at the mother's area, records a physically present birth attendant, and emits `Born`.

## Boundaries

- **Biological mechanics vs. reproductive choice:** Enforces maturity, fertility windows, and gestation timers; [`game-parenthood`](../game-parenthood/README.md) decides when consenting partners choose to conceive, and [`game-heredity`](../game-heredity/README.md) expresses full trait genotypes on `Born`.

## How It Works

- **Travel-safe delivery:** If a pregnancy reaches full term while the mother is in transit on a road, delivery remains scheduled for a settled area, retrying rather than dropping the pregnancy.
- **Physical estate neutrality:** Death leaves physical possessions in place without automatically transferring property claims. Delivery cancels a pregnancy if the mother has died; the father's death does not cancel it.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`lifecycle`, resource `lifecycle.population`).
- **Requests & Events:** Handles `Conceive` and `Birth`, schedules delivery, and subscribes to `game_physiology::Died`; emits `Conceived` and `Born`.
- **Callers:** Invoked by [`game-parenthood`](../game-parenthood/README.md) and [`game-openings`](../game-openings/README.md); `Born` is subscribed to by `game-heredity`, `game-lineage`, `game-households`, and `game-names`.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-storage`](../game-storage/README.md), [`game-sim`](../game-sim/README.md), [`game-physiology`](../game-physiology/README.md), [`game-transport`](../game-transport/README.md), [`game-cargo`](../game-cargo/README.md), [`game-genetics`](../game-genetics/README.md).
- **Testing:** `cargo test -p game-lifecycle` verifies fertility windows, gestation scheduling, deferred birth during travel, and parental lineage recording.

Successful conception produces a reproductive-drive-dependent subjective reward
through vitality. The parenthood owner evaluates voluntary intentions; lifecycle
continues to enforce biological capability rather than demographic targets.
`Family.attendant` and pregnancy/birth requests record birth contact, not a care
obligation. A dead or absent anticipated attendant is replaced in the birth
provenance by the present mother. Biological parents remain unchanged. Actual
care commitments belong to `game-households` and are chosen independently.
