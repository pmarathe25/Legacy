# game-genetics

This crate passes opaque genetic material between parents and children. Trait owners supply stable locus IDs, chromosome positions, allele bounds, and mutation settings through `HeritableCharacteristic`; they interpret the inherited values themselves.

Each parent contributes a linked chromosome copy, with configured crossover and mutation. The child’s genome and parental transmission traces are fixed at conception. Genetics validates and saves this substrate without knowing about physical traits, temperament, development, or reproductive eligibility.

`Runtime` holds the rules, locus definitions, and content-derived variant registry. `PersonGenome` and `Inheritance` are plain components on person and pregnancy stable entities in the host's shared `game-ecs::Store`. `register_components` installs the `genetics/genome` and `genetics/inheritance` columns and their intrinsic validation; typed accessors read and change only those columns. Runtime operations take an explicit store. Birth commits the child genome and pregnancy record atomically, and conception publishes mutation variants only after its inheritance component succeeds.

Trait owners can accept `AlleleSource` and use `runtime.allele_source(&store)` to read opaque values without depending on the runtime or storage layout.

Save the runtime and the shared store snapshot together. Restore the store through the composed schema, then call `validate(&runtime, &store, now)` to check locus bounds, parental genomes, linked crossover, mutation, and frozen transmission traces. Trait interpretation and acquired characteristics remain with their owners; inheritance never copies their components.
