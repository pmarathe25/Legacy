# game-heredity

`game-heredity` stores human diploid genotypes, tracks generational depth, and expresses inherited trait profiles when children are born.

**Why it matters:** Multigenerational simulations require children to inherit physical and personality traits from their actual parents with bounded variation, while tracking true reproductive generation depth for cohort studies.

## Responsibilities

- **Human genotypes (`Heredity`):** Stores per-person diploid genotypes, generation numbers, maximum ancestry depth, and living genotyped mortal counts (`heredity.state`).
- **Founder registration (`Founder`):** Encodes a founder's initial trait profile into two diploid alleles per authored trait.
- **Birth expression:** Subscribes to `game_lifecycle::Born`, combines parental alleles via [`game-genetics`](../game-genetics/README.md), and writes the newborn's expressed trait profile via `game_traits::Set`.

## Boundaries

- **Biological traits only:** Expresses inherited capacities and dispositions into [`game-traits`](../game-traits/README.md); never encodes learned skills, vocabulary, social beliefs, or memories in genes.

## How It Works

- **Conservative generation counter:** A child's `generation` is `1 + min(parent_a.generation, parent_b.generation)`, so reaching generation $N$ requires $N$ complete reproductive steps across every ancestral branch.
- **Incremental population counters:** Maintains `maximum` generation depth and `living` genotyped counts incrementally on birth and death, cross-checking both during save validation.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`heredity`, resource `heredity.state`).
- **Requests & Events:** Handles `Founder`; subscribes to `game_lifecycle::Born` and `game_physiology::Died`.
- **Callers:** Bootstrapped by [`game-campaign`](../game-campaign/README.md) and [`game-openings`](../game-openings/README.md); triggered automatically on births from [`game-lifecycle`](../game-lifecycle/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-storage`](../game-storage/README.md), [`game-genetics`](../game-genetics/README.md), [`game-traits`](../game-traits/README.md), [`game-lifecycle`](../game-lifecycle/README.md), [`game-physiology`](../game-physiology/README.md).
- **Testing:** `cargo test -p game-heredity` verifies founder encoding, parental allele inheritance, generation depth math, and save validation.
