# game-lifecycle

`game-lifecycle` manages biological conception, scheduled gestation, childbirth, newborn identity allocation, and initial caregiver placement.

**Why it matters:** Multigenerational history requires every new person to be born from actual parents after a real gestation period in a physical location, never spawned out of thin air to maintain population numbers.

## Responsibilities

- **Lifecycle records (`Lifecycle`):** Stores each person's birth time, biological parents, simple genetic alleles, caregiver assignment, and active pregnancies (`lifecycle.state`).
- **Conception & gestation (`Conceive`):** Verifies that both parents are alive, mature, co-located, and within their fertile window (the first two-thirds of expressed natural lifespan), then schedules a timed birth.
- **Childbirth (`Deliver`):** Allocates a new `PersonId`, spawns the infant's body (`game-physiology`) and location (`game-transport`) at the mother's area, assigns the mother as initial caregiver, and emits `Born`.

## Boundaries

- **Biological mechanics vs. reproductive choice:** Enforces maturity, fertility windows, and gestation timers; [`game-parenthood`](../game-parenthood/README.md) decides when consenting partners choose to conceive, and [`game-heredity`](../game-heredity/README.md) expresses full trait genotypes on `Born`.

## How It Works

- **Travel-safe delivery:** If a pregnancy reaches full term while the mother is in transit on a road, delivery remains pending and triggers as soon as she arrives at an area (`game_transport::Arrived`).
- **Physical estate neutrality:** Mortal death clears active pregnancies and leaves physical possessions in place without automatically transferring property claims.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`lifecycle`, resource `lifecycle.state`).
- **Requests & Events:** Handles `Register`, `Conceive`, `AssignCaregiver`; subscribes to `game_transport::Arrived` and `game_physiology::Died`; emits `Born`.
- **Callers:** Invoked by [`game-parenthood`](../game-parenthood/README.md) and [`game-openings`](../game-openings/README.md); `Born` is subscribed to by `game-heredity`, `game-lineage`, `game-households`, and `game-names`.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-storage`](../game-storage/README.md), [`game-sim`](../game-sim/README.md), [`game-physiology`](../game-physiology/README.md), [`game-transport`](../game-transport/README.md), [`game-cargo`](../game-cargo/README.md), [`game-genetics`](../game-genetics/README.md).
- **Testing:** `cargo test -p game-lifecycle` verifies fertility windows, gestation scheduling, deferred birth during travel, and parental lineage recording.
