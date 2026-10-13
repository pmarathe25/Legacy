# game-genetics

This crate passes opaque genetic material between parents and children. Trait owners supply stable locus IDs, chromosome positions, allele bounds, and mutation settings through `HeritableCharacteristic`; they interpret the inherited values themselves.

Each parent contributes a linked chromosome copy, with configured crossover and mutation. The child’s genome and parental transmission traces are fixed at conception. Genetics validates and saves this substrate without knowing about physical traits, temperament, development, or reproductive eligibility.

`Runtime` holds the rules, locus definitions, and content-derived variant registry. `PersonGenome` and `Inheritance` are plain components on person and pregnancy stable entities in the host's shared `game-ecs::Store`. `register_components` installs the `genetics/genome` and `genetics/inheritance` columns and their intrinsic validation; typed accessors read and change only those columns. Runtime operations take an explicit store.

`prepare_conception` and `prepare_birth` return effects without changing rows or the variant registry. They stage their columns into a shared batch using the host's `ReadSet` protocol. The staged effects validate the original rows, relevant runtime metadata, and the exact writes before admission. After successful batch application, `StagedConception::finish` publishes mutation variants without further fallible work. `PreparedBirth::genome` exposes the inherited row for receiving owners to interpret before publication. `conceive` and `born` are convenience commits of the same owner entry points.

Trait owners can accept `AlleleSource` and use `runtime.allele_source(&store)` to read opaque values without depending on the runtime or storage layout.

Save the runtime and the shared store snapshot together. Restore the store through the composed schema, then call `validate(&runtime, &store, now)` to check locus bounds, parental genomes, linked crossover, mutation, and frozen transmission traces. `Runtime::validate_references` consumes factual person identities, locus definitions, and `PregnancyReference` stages to validate cross-feature joins without importing those features. Trait interpretation and acquired characteristics remain with their owners; inheritance never copies their components.
