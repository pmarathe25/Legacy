# Mechanics

This owner stages explicit experiments on independent apparatus in parallel.
Each apparatus is an acyclic graph of distinct parts, with data-defined force
ratios, efficiency and strength. Branches divide the available input force;
integer rounding and losses never create work. An overloaded part breaks and
remains broken. Competing experiments on one apparatus have a deterministic
winner; the rest receive contention outcomes. Preparing a batch does not mutate
state, and a stale draft cannot be published.

The caller supplies actual chosen experiments and is responsible for verifying
people's locations, effort, part custody and access. Part IDs are unique within
this owner but have not yet been connected to physical item custody. This crate
is not yet integrated into the interval simulation, and does not by itself
provide character invention, skills or scientific discovery.

The ignored benchmarks compare complete state and observations across worker
counts. On one development machine, 50,000 independent apparatus took 14.4,
8.40, 6.15 and 5.18 ms with one, two, four and six workers. With 50,000
experiments competing for one apparatus, the corresponding times were 2.12,
1.21, 0.78 and 0.84 ms. These measurements cover this owner alone, not item
custody or the full simulation.
