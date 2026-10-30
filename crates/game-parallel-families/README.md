# Parallel family owner

This owner supplies a deterministic life-history phase to `game-parallel`.
`content/parallel/families.toml` defines its parameters and initial people. All ages,
gestation, recovery spacing and lifespan values are simulation ticks; the example
deliberately accelerates generations and does not define a real-world calendar.

Each person retains an individual demographic record, including recorded birth,
age at death, parents, reproductive opt-in, partner, pregnancy and four diploid
quantitative loci. Founders can predate the epoch. Alleles range from 0 to 1000.
Fertility and longevity affect conception probability and natural lifespan;
vigor and sociability are inherited but have no behavioral effect yet. Each child
inherits one independently selected allele from each parent at each locus, with
data-defined bounded mutation. Dead people and their ancestry remain saved.

Workers first advance independent personal rows. Region-local matching proposes
eligible pairs in parallel, then resolves exclusive partner claims deterministically.
For large batches, proposals are grouped by the prospective father and resolved in parallel; the earliest female priority wins, as in the serial path. Each proposal considers at most 32 ranked candidates, avoiding an exhaustive
quadratic search; an unsuccessful person can try again on the next tick. Eligible
people are healthy, fed, local adults below the fertility limit who have opted in.
The current reproductive model pairs a female with a male and excludes shared
ancestors through grandparents. Existing partnerships persist across travel and
end on death. This is an explicit policy model, without courtship, attraction,
coercion, marriage law, or a complete account of sex and gender.

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

Optional `min_work_age` in family rules gates manual jobs, automatic work, and farm labor. A value of zero preserves unrestricted work; the comparison hosts reject nonzero limits rather than silently omitting them. `content/parallel/work_age.toml` demonstrates a child becoming eligible on the tick after reaching the configured age.
