# Plant genetics and physical seed provenance

Plant selection requires physical reproductive material to retain its history.
The earlier lot identity consisted of a good, expiration and optional unique
item; an item is a single nonperishable object and cannot identify a divisible
seed batch. Lots now optionally preserve opaque divisible batch identities
through generic movements, but movements cannot yet select a particular
batch. Planting still uses compact consumption receipts, so a successful field
cannot determine which actual seed was consumed. Generic harvest outputs have no
associated biological state. These are missing capabilities, not data that a
farmer should be allowed to infer from an aggregate stock balance.

Introduce an optional opaque batch identity for divisible physical lots. Batch
identity participates in cohort merge keys and survives splitting, transfer,
consumption, expiration, serialization and rollback. A batch may have the same
genome as another batch without being the same batch: provenance and an
individual's observations can differ. The lot owner stores no genetic meaning.
Ordinary goods without batch identities retain compact paths. Exact-batch
selection must coexist with FEFO selection and deterministic shared-stock
arbitration; targeted and generic claims cannot reserve the same quantities.
No genetic state or batch identifier is committed until the physical draft and
its containing host operation commit.

An agriculture-owned biological state can then map batches to immutable,
diploid genotypes and keep counted living cohorts per field. Planting derives
its living cohorts from the exact accepted consumption receipts. Growth and
reproduction use environmental circumstances and stable, event-keyed random
choices; mutation and inheritance produce variation rather than choosing a
universal improvement target. Determine offspring at maturation, or use a stable
maturation identity, so a failed harvest cannot reroll its offspring. Seeds
created by a harvest are physical output and compete with food and other uses.

Growers may choose seed or parents from reachable physical batches using their
own remembered observations of yield, timing, survival and context. They do not
inspect alleles, global fitness, or results that they did not observe. Different
people may value or interpret the same evidence differently, decline to select,
or have no relevant knowledge. No role or institution forces planting or
compliance. Wild stands, seed saving and crossing require their own actual
physical opportunities and personal decisions.

Implementation checkpoints:

1. Carry and optionally select opaque divisible batch identities through every
   lot allocation path. Test generic and targeted contention, expiry, splits,
   merge, conservation, atomic rejection, save/load and worker determinism;
   benchmark ordinary unbatched goods as well as concentrated mixed claims.
2. Plant from detailed accepted receipts and produce traceable reproductive
   outputs atomically with agriculture. Check that rejected planting and harvest
   cannot create or lose genetics or consume identifiers.
3. Add environmental survival, reproduction, inheritance and bounded mutation
   for counted cohorts. Test multiple generations and worker determinism.
4. Add personal, fallible observations and optional seed retention, selection
   and crossing. Demonstrate different choices from different experiences with
   identical reachable stock and no hidden-genome access.
5. Measure independent fields and contested seed stores separately, including
   cohort count, memory, allocation, staging and publication costs. A growing
   number of distinct batches is a real workload cost and must not be silently
   truncated or presented as linear whole-world scaling.
