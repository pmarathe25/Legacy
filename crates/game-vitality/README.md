# game-vitality

`Experiences` applies distinct personal mood changes on private owner rows in the
shared worker pool and publishes them in input order. Scalar experience hooks and
duplicate recipients require the ordinary request path.

`game-vitality` models human physical and cognitive development from infancy through old age, tracks stress and satisfaction, and computes expressed capabilities from baseline traits.

**Why it matters:** An infant, a healthy adult, and an elderly or distressed elder with the same genetic potential have very different physical strength, walking ability, language readiness, and lifespan limits.

## Responsibilities

- **Vitality records (`Vitality`):** Stores each person's birth/age clock, natural lifespan, stress, satisfaction, and accumulated childhood care/distress history (`vitality.state`).
- **Developmental & aging curves:** Delegates pure expression to [`game-expression`](../game-expression/README.md), which computes age-dependent expression fractions for size, strength, endurance, locomotion, language readiness, reasoning, foresight, learning, and emotional regulation.
- **Expressed ability queries (`Assess`, `ability`, `disposition`):** Combines inherited `game-traits` baselines with age maturation, senescence (beginning halfway through natural lifespan), and childhood formative effects.

## Boundaries

- **Capability expression vs. bodily health:** Supplies expressed capabilities, natural lifespan, and psychological condition; [`game-soma`](../game-soma/README.md) and [`game-physiology`](../game-physiology/README.md) own actual wounds, hunger, fatigue, and death, while [`game-wellbeing`](../game-wellbeing/README.md) translates social/leisure events into condition updates.

## How It Works

- **Formative childhood memory:** Elapsed childhood time spent in severe hunger/injury or high stress leaves bounded permanent adjustments on adult capability, patience, risk tolerance, and sociability.
- **Milestone gating:** Infants cannot walk independently before locomotion readiness (`game-transport`) or hold adult conversations before language readiness (`game-communication`, `game-social-life`). Guardians do not age and retain full mature expression.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`vitality`, resource `vitality.state`).
- **Requests & Events:** Handles `Enable`, `Register`, `Condition`, `Assess`.
- **Callers:** Queried by almost every physical, cognitive, and social mechanic (`game-metabolism`, `game-soma`, `game-transport`, `game-communication`, `game-conflict`, `game-skills`, `game-characters`).

## Dependencies & Testing

- **Dependencies:** [`game-expression`](../game-expression/README.md), [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-storage`](../game-storage/README.md), [`game-traits`](../game-traits/README.md).
- **Testing:** `cargo test -p game-vitality` verifies childhood growth trajectories, formative distress effects, aging onset, health-fraction preservation during growth, and guardian immortality.

Sustained attention has inherited potential and follows cognitive development, stress, aging, and childhood conditions. It affects switching costs and retained observations rather than imposing a fixed activity cooldown. Empathy and compassion have inherited baselines, with expression shaped by development and upbringing; learned values and individual relationships remain separate. Deliberate manipulation and independent walking use expressed motor readiness. A newborn can be a victim without receiving adult physical capabilities.

The inherited `rigidity` disposition affects commitment switching and is modulated
by psychological development, accumulated childhood distress, and current stress.
It describes persistence, not a favored identity, belief, or political procedure.

`Life`, `Upbringing` and `maturation` remain public reexports. State ownership and
save schemas are unchanged. Expression caching now also covers the six modulated
dispositions with exact life/clock/raw inputs; the authoritative uncached formulas
remain available on `Life`.

Repeated owner queries validate weak life-table, trait-catalog and profile versions
before reusing expressed abilities, modulated dispositions and motor or conversation
readiness. This avoids resolving the same rows and raw traits for each provider.
The cache is bounded to 128 personal frames per thread and checks the exact clock.
It retains numeric results and weak tokens, never a world or resource snapshot.
Missing resources, direct edits, nested rollback and loading another world retain
their ordinary results. Unmodulated authored traits take the raw-trait path directly.

Each frame lazily shares pure developmental factors through
`game_expression::Projection`, so different attributes reuse the same upbringing
integration, condition recovery and common maturation curves. Every result still
uses the exact simulation clock and unchanged integer arithmetic.
