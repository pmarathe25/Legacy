# game-heredity

Genes express authored traits independently of learned skills, beliefs, and personal knowledge. Founders receive two alleles per trait; births sample one allele from each parent, apply bounded mutation, and map expression back to the trait's authored numeric range. Personality preferences retain their 0–1000 scale; intelligence, foresight, learning, size, strength, and hardiness have a 16000 ceiling with a 1000 baseline.

`Founder` registers an initial person's expressed profile as a genotype. The `Born` subscriber expresses inherited characteristics through `game-traits::Set`. `Heredity::generation` is one plus the shallower parent's generation: a generation-20 person has traversed twenty reproductive steps on every ancestral branch. Genomes remain research state, outside character decisions.

`maximum` and `living` maintain ancestry depth and the living genotyped mortal count incrementally. Save validation compares both against the full records. Legacy scalar alleles remain in lifecycle family records for the existing interface; expressed characteristics use this owner's per-trait loci.
