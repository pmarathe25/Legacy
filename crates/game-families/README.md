# family owner

> Historical note: references below to removed experimental hosts and their fixtures describe the previous implementation. See the [current runner](../game-economy-cli/README.md) for supported scenarios and commands.

This owner supplies a deterministic life-history phase to the economic simulation.
`content/scenarios/families.toml` defines its parameters and initial people. All ages,
gestation, recovery spacing and lifespan values are simulation ticks; the example
deliberately accelerates generations and does not define a real-world calendar.

Each person retains an individual demographic record, including recorded birth,
age at death, parents, reproductive opt-in, partner, pregnancy and four diploid
quantitative loci plus a separate diploid mate-desire locus. Founders can predate the epoch. Alleles range from 0 to 1000.
Fertility and longevity affect conception probability and natural lifespan;
vigor and sociability are inherited but have no behavioral effect yet. When subjective mating is enabled, the separate mate-desire locus contributes to the individual desire to seek a reproductive encounter. Each child
inherits one independently selected allele from each parent at each locus, with
data-defined bounded mutation. Dead people and their ancestry remain saved.

Workers first advance independent personal rows. Region-local matching proposes
eligible pairs in parallel, then resolves exclusive partner claims deterministically.
Hosts may additionally supply physical contact partitions for both sides of the
boundary; matching and conception require equal partitions as well as a shared region.
The unpartitioned default preserves ordinary regional behavior.
For large batches, proposals are grouped by the prospective father and resolved in parallel; the earliest female priority wins, as in the serial path. Each proposal considers at most 32 ranked candidates, avoiding an exhaustive
quadratic search; an unsuccessful person can try again on the next tick. Eligible
people in the legacy path are healthy, fed, local adults below the fertility limit who have opted in.
Both reproduction paths pair a female with a male and exclude shared
ancestors through grandparents. Existing partnerships persist across travel and
end on death. This is an explicit policy model without a complete account of courtship, coercion, marriage law, sex, or gender.

Pregnancy continues after paternal death. Maternal death ends it. Birth identity
allocation follows ascending maternal ID, independent of region layout and worker
count; inheritance computation then runs in parallel. The population owner applies
newborn and death effects at a barrier. A child born in transit remains on the
mother's route. Birth adds a person, never food. Existing personal needs apply to
children too. Hunger can suppress conception and starvation can kill people;
nursing, childcare, custody, asset inheritance, pregnancy costs and obstetric
risks are not implemented.

The adapter stages physical regions in parallel and publishes only after all
fallible family work succeeds. Overflow leaves the entire tick unchanged. Saves
contain pregnancies, genetics, identity allocation and cumulative counters. The
`events` field contains only the latest tick's canonical birth, death, adulthood,
partnership, conception and pregnancy-loss receipts; stream them for a full history.
Known parent IDs must precede a child's ID, which makes the saved pedigree acyclic.

Parallel aging, matching preparation and inheritance are production work, not a
synthetic numerical workload. Identity allocation, state indexing and installation
still have serial costs; no near-linear scaling claim has been measured for this
extended model.

Optional `min_work_age` in family rules gates manual jobs, automatic work, and farm labor. A value of zero preserves unrestricted work; the comparison hosts reject nonzero limits rather than silently omitting them. `content/scenarios/work_age.toml` demonstrates a child becoming eligible on the tick after reaching the configured age.

With `subjective_mating = true`, people form partnerships only when both personally seek and accept one another; either may leave when their own assessment falls below their threshold. Reproductive encounters are separate from any existing partnership. A partnership can form between adults even when they are infertile or do not opt into reproduction. Both adults independently seek an encounter based on their own `mate_preference` and inherited `mate_desire` alleles, then independently evaluate the other person using their own directed affinity and stable individual attraction. Hunger and the number of living minor children can each reduce or increase desire according to the individual’s signed weights; children do not impose an obligation. Each chooser considers at most 32 local, physically reachable candidates and ranks mutually acceptable candidates by their own assessment; the same person may be selected by more than one prospective mother. Partnership matching can include same-sex pairs, and individuals can configure signed attraction adjustments for each modeled sex. Biological conception still uses the current female–male model. This is not a complete courtship or relationship model. The defaults are desire 500, genetic weight 250, affinity weight 10, attraction weight 100, hunger weight -2, dependent weight -100, and minimum score 0. Existing genomes without `mate_desire` load with neutral alleles `[500, 500]`. The new mode is opt-in; legacy scenarios retain their earlier automatic exclusive matching. All chance parameters apply per simulation tick and must be calibrated when changing tick duration.
