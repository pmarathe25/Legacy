# game-genetics

This crate passes opaque genetic material between parents and children. Trait owners supply stable locus IDs, chromosome positions, allele bounds, and mutation settings through `HeritableCharacteristic`; they interpret the inherited values themselves.

Each parent contributes a linked chromosome copy, with configured crossover and mutation. The child’s genome and parental transmission traces are fixed at conception. Genetics validates and saves this substrate without knowing about physical traits, temperament, development, or reproductive eligibility.
