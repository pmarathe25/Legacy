# Reusing database and linear algebra optimizations

The expensive autonomous decision pipeline contains several different problems:

| Work | Familiar formulation | Suitable optimization |
| --- | --- | --- |
| Find personally remembered, locally usable supplies | Selection and join | Covering indexes, predicate pushdown, materialized views |
| Match recipes, objects, stores and actors | Sparse joins with shared catalogs | Shared catalog indexes and grouped probes |
| Score already prepared actor-specific candidates | Segmented reduction / batched GEMV | Direct numeric views, SIMD, stable top-K |
| Choose compatible contested actions | Ordered arbitration | Deterministic commit after independent reads |
| Update views after observations and actions | Incremental view maintenance | Payload deltas and owner-local revision tokens |

## Database formulation

The concrete prototype uses the care provider's supply lookup. Its logical input
relations are `remembered_location(person, inventory, area, carrier)` and
`remembered_stock(person, inventory, good, quantity)`. It selects stocks for the
requested good and quantity, joins locations, then keeps the actor's carried
stores or remembered stores in the current area. Output is deduplicated and
ordered by inventory ID. Only that actor's personal evidence participates;
substituting actual cargo contents would teach information the actor does not
have. Noncanonical store aliases use canonical stock/location evidence, and an
absence report suppresses the old location.

`relational.py` compares four SQLite plans on the same decoded checkpoint:

1. Scan this person's store index and probe each stock map for the good.
2. Find qualifying rows in a covering person/good/quantity index, then join
   locations. This pushes the selective predicate before other work.
3. Query a materialized stock/location join with a person/good index, avoiding
   repeated location probes.
4. Join a whole batch of actor/query parameters against that view in one query,
   avoiding per-actor statement dispatch. Loading batch parameters is reported
   separately. A changing batch must pay that cost again. The fresh-batch estimate
   adds the median query time to one parameter-loading sample.

Both baseline and alternatives use SQLite. Their ratios compare query algorithms
within that engine; they do not compare SQL with production Rust. The reference
uses separately implemented store and stock maps. Construction costs and logical
database bytes are explicit. A positive stock fact is zeroed transactionally,
every query plan is checked against the changed reference, and rollback must
restore the original outputs. Update timing is a single diagnostic sample,
rather than an amortized throughput result.

Each stationary living actor queries the treatment good and staple good 1. The
latter is a denser control for the same material lookup, not a reproduction of
food choice: food appraisal also has personal recall windows, dietary scoring,
travel and other rules. This experiment requires positive requested quantities.

Several of these ideas already exist in production: namespace-indexed personal
facts, incrementally maintained remembered-store views, compiled trait weights,
and revision-checked recipe preparation. A possible exact extension is an
inverted good index within the existing remembered-store owner. It could serve
care and other material queries without scanning every store, and would update
from the same stock/location payload deltas. A native integration must include
its maintenance, memory, alias semantics, rollback and whole-history costs.
The measured fixtures below have very short store lists and do not justify this
extra index. Installing SQLite as another authoritative world is unnecessary.

A broader architectural change would declare each provider's pure candidate
query separately from its observations, requests and other effects. Pure query
results could then have indexed dependencies and incremental maintenance. A
database optimizer can reorder pure joins; current ordered providers can also
write observations and affect later providers. Reordering those effects would
change histories. Snapshot batches must retain the established ordered commit
path and invalidate read views using all actual dependencies, including time.

Late materialization is another transferable technique: retain compact candidate
IDs, numeric terms and an owner/template reference during filtering and ranking,
then build the executable payload only for retained candidates. Currently
`Proposal::new` allocates a typed pending payload before the collection can reject
it at the 256-offer limit. Some owners already reuse immutable `Templates`.
Extending that idea requires preserving key validation, duplicate detection,
continuation precedence, before-hooks and the ordinary request path. A closure
allocation per candidate would merely move the allocation rather than remove it.
The native prototype should measure complete preparation, not only numeric ranking.

A single lookup cannot deliver the full speedup suggested by its kernel ratio.
Use the complete-history profile to identify how much time its callers consume,
then apply the query representation to the expensive providers. The SQL probe
does not justify a claim that all opportunity generation can be accelerated by
its measured lookup ratio.

## Matrix formulation

Each actor has five utility weights, and each candidate has five motive terms.
With actor-specific candidate sets, the needed operation is
`score[a,n] = reduce(terms[a,n,:], weights[a,:])`: a ragged batch of tiny matrix-vector
products, followed by stable selection. It is not naturally one large dense GEMM.
Multiplying all actor weights by all candidate terms scores every other actor's
ineligible candidates too. With A actors this computes A times the needed number
of scores and creates an A-by-total-candidates output.

Arithmetic also matters. Production saturates each signed 64-bit product, divides
each channel by 100 with truncation toward zero, then adds/subtracts channels with
saturation in a fixed order. A floating dot product divides after summing, and
can change ties even when values are small. For example, two terms of 1 weighted
by 50 each contribute 0 individually; their floating dot product is 1. Float
precision and saturation add further differences at large values. An ordinary
BLAS call therefore needs an explicitly different scoring contract or an exact
correction stage.

`linear.py` uses actual saved shortlist terms and compiled trait weights. It
counts different scores and stable unsigned-ID winners, reports numeric packing
separately, and times a row-wise integer implementation against floating batched
dot and GEMM. It excludes fresh opportunity generation, food plans, failure costs,
current-action hysteresis and execution. Its integer kernel rejects ranges that
could overflow; the production-compatible full-range SIMD and GPU experiments
remain in `tools/compute`.

The saved lists are arithmetic fixtures: they can contain consumed or expired
options. The probe does not claim that its winning row is the character's next
action. All candidate IDs stay unsigned Python integers during tie-breaking.

Dense GEMM becomes more plausible if many actors share the same candidate
templates, a much wider feature dimension, and long-lived numeric views. Grouping
identical templates can provide reuse without a dense eligibility mask. Changing
scoring to permit float arithmetic is a separate model experiment; it does not
remove the costs of discovering or executing candidates.

## Measured query plans

Five alternating rounds, ten passes per round, SQLite on the Xeon Platinum 8573C
container. These are milliseconds for a pass over stationary living actors,
querying treatment material and staple good 1 for each actor:

| Checkpoint | Actors / queries | Store scan | Good index | Materialized join | Batched join | Batch including parameter-load estimate |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Opening day 10 | 87 / 174 | 0.380 | 0.422 | 0.323 | 0.075 | 0.193 |
| Older day 300 | 10 / 20 | 0.040 | 0.047 | 0.040 | 0.008 | 0.038 |
| Large cohort day 1 | 983 / 1,966 | 4.335 | 4.873 | 3.748 | 0.934 | 2.155 |

All ordered rows match the independent reference before and after an update and
rollback. The large projection has 1,073 remembered stores and 7,361 stock rows
across all retained people; the queried actors account for only 1,052 stores.
Most actors remember about one store, so an additional good index has very little
work to eliminate. Materializing the join helps modestly. Batched execution saves
statement dispatch and Python/SQLite boundary calls; production Rust already
uses in-process indexes, so its speedup cannot be inferred from these ratios.

Projecting decoded checkpoint objects takes 190.9 ms in Python; JSON decoding is
reported separately. Building all SQLite
views and planner statistics takes another 11.55 ms; batch parameters take
1.22 ms. Logical storage is 151,552 bytes for normalized tables, 262,144 with the
good index and 421,888 with both that index and the materialized join. These are
logical page sizes, not process RSS. The views need reuse and incremental updates
to recover construction costs. Production's existing remembered-store projection
already provides incremental reuse. The whole `care.help` provider accounts for
only about 3% of the ordinary large-history advance profile, which also limits
the possible benefit from optimizing just this lookup.

Reports are `target/history/optimization/relational-{opening,older,scale}.json`.
Each includes checkpoint and driver hashes, query plans, every timing round,
construction costs, candidate counts and update/rollback checks.

## Measured arithmetic

Five alternating rounds, 100 passes per round, NumPy with one BLAS thread. GEMM
reuses a preallocated output buffer; initialization is reported separately.
These are arithmetic fixtures from saved raw shortlists, not complete character
decisions or native Rust preparation timings.

| Saved shortlist | Actors / rows | Integer rows | Floating batched dot | Dense floating GEMM | Winners differing from integer rows |
| --- | ---: | ---: | ---: | ---: | ---: |
| Opening | 88 / 1,397 | 0.075 ms | 0.027 ms | 0.061 ms | 0 / 88 for either float kernel |
| Large cohort | 978 / 15,636 | 0.802 ms | 0.276 ms | 12.280 ms | 12 / 978 for either float kernel |

Large GEMM computes 15,292,008 scores to obtain the required 15,636: 978 times
the needed score count, with a 122,336,064-byte output buffer. It is about 15.3 times slower
than the integer-row prototype. Floating batched dot avoids that waste but changes
15,330 row scores and 12 winners. Dense GEMM changes 15,346 scores and the same
number of winners. Integer rows match every reference score and winner.
Projection from saved Python objects takes 48.4/291.9 ms for the two fixtures,
including compiling trait weights; production already caches compiled weights.
These costs are separate from kernel timing. Native full-range SIMD and complete
preparation measurements in `tools/compute` remain the relevant production
comparison. Dense GEMM is unsuitable for the present actor-specific five-channel
shape; indexed/batched discovery and compact exact reductions are the stronger
direction.

The next native prototype should focus on expensive provider preparation:
separate pure candidate queries from observation effects, share actor/catalog
read views, and materialize executable payloads after filtering and ranking.
Feature owners keep their rules and writes; pure batches retain the ordinary
dependency validation and ordered commit path. Measure complete histories and
require all replay hashes to match before making this an ordinary optimization.

Reports are `target/history/optimization/linear-{opening,scale}.json`, including
the NumPy/BLAS configuration, input hashes, packing/buffer costs and all rounds.
