# Mechanics

This owner stages explicit experiments on independent apparatus in parallel.
Each apparatus is an acyclic graph of distinct parts, with data-defined force
ratios, efficiency and strength. Branches divide the available input force;
integer rounding and losses never create work. An overloaded part breaks and
remains broken. Competing experiments on one apparatus have a deterministic
winner; the rest receive contention outcomes. Preparing a batch does not mutate
state, and a stale draft cannot be published.

The owner can also stage explicit assembly proposals, validating each proposed
graph and arbitrating competing apparatus identifiers and parts in parallel.
A proposal succeeds only if it wins every resource; it may lose resources to a
proposal that itself loses another resource. The interval host verifies that an
available actor actually carries each correctly typed identified part before it
commits. An opted-in person can also compare a generated two-part design with
other activities and idleness in the shared choice round. The host retains the
exact selected parts and revalidates before assembly. Assembly currently has no
duration or disassembly, and design search, experimentation and learning are
still rudimentary.

The ignored `assembly_scaling` benchmark stages and commits 100,000 proposals.
Median times at one, two, four and six workers were 86.1, 59.3, 44.9 and
40.3 ms for independent parts, and 57.4, 31.9, 20.7 and 17.3 ms when all
proposals competed for the same parts. Results matched across worker counts;
these timings exclude physical-custody and personal-choice processing.

The caller supplies actual chosen experiments and is responsible for verifying
people's locations, effort, part custody and access. The interval host offers an
explicit experiment API that checks actors' carried identified items and effort
limits. The interval host can offer personally willing experiment policies in
its opt-in choice round. The interval host also offers an explicit chosen-load
action using data-defined force per unit of a real good. It stages physical
access, evaluates the apparatus, and transfers the actual lot only when the
calculated travel is sufficient. Short travel, stalled or broken apparatus,
and unavailable stock do not transfer the whole load. Resistance and required
travel are still scalar inputs; partial positions, fatigue, invention and
scientific discovery are not yet modelled by this action. Optional personal
load policies participate in the interval host's shared choice round; a person
can prefer another activity or idleness, and an unavailable load can fail.

The ignored benchmarks compare complete state and observations across worker
counts. On one development machine, 50,000 independent apparatus took 14.4,
8.40, 6.15 and 5.18 ms with one, two, four and six workers. With 50,000
experiments competing for one apparatus, the corresponding times were 2.12,
1.21, 0.78 and 0.84 ms. These measurements cover this owner alone, not item
custody or the full simulation.
