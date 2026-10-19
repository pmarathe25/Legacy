# game-genetics

`game-genetics` provides pure functions and data types for seeded diploid inheritance, bounded allele mutation, and quantitative trait expression.

**Why it matters:** Both human heredity (`game-heredity`) and diploid plant breeding (`game-cultivars`) share the same deterministic Mendelian segregation and mutation math without duplicating genetic code or coupling to `game-sim`.

## Responsibilities

- **Diploid genomes (`Genome`, `Allele`, `Locus`):** Represents named diploid loci with pairs of integer alleles and supports thread-local deduplicated deserialization (`deserialize_genome`).
- **Mendelian inheritance (`inherit`):** Samples one allele from each parent per locus using `game-core` deterministic RNG and applies bounded mutation steps.
- **Phenotypic expression (`express`):** Maps diploid allele pairs into expressed trait values within caller-supplied numeric bounds.

## Boundaries

- **Pure mathematical utility:** Owns no simulation resources, species catalogs, or social meanings; [`game-heredity`](../game-heredity/README.md) and [`game-cultivars`](../game-cultivars/README.md) own human and plant genotypes respectively.

## How It Works

- **Deduplicated genome arcs:** `deserialize_genome` interns identical `Arc<Genome>` values through a bounded 4,096-entry thread-local weak cache so thousands of related seeds or plants share immutable genome allocations.
- **Copy-on-write mutation:** Mutating a shared genome via `Arc::make_mut` clones only the modified instance.

## Game Integration

- **Plugin / Entry Point:** Pure library crate (`Genome`, `Locus`, `Allele`, `inherit`, `express`, `deserialize_genome`); registers no `game-sim` plugin.
- **Requests & Events:** None.
- **Callers:** Used by [`game-lifecycle`](../game-lifecycle/README.md), [`game-heredity`](../game-heredity/README.md), and [`game-cultivars`](../game-cultivars/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md).
- **Testing:** `cargo test -p game-genetics` verifies deterministic allele segregation, mutation bounds, expression scaling, and genome interning.

Plant `Genome` keys use immutable `GeneName` values shared across distinct genomes. They retain string ordering, string lookup, and the existing string-keyed save format. The weak name cache is bounded and bypasses long extension names; it retains no genome or living entity. Construct locus keys with `.into()` when using the Rust API.
