# Architectural headroom

The [decision concurrency evaluation](CONCURRENCY.md) records the newer exact
dependency executor and opt-in phased proposal model. It distinguishes snapshot
visibility, review frequency, active CPU occupancy and useful parallel speedup.

**Decision:** Hot component stores now use `game-packed` dense rows and nested undo journals. Shared immutable scenes in `game-facts` replace repeated per-person observation writes. Cold histories, relationships, and other sparse state retain persistent tables. Typed pending requests and one owner-checked host remain. The earlier isolated packed-state kernel and analytical intervals below are historical experiments; the composed implementation now has its own measurements.

## Production layout and tradeoffs

The transaction owns the resource map during a writable phase and checkpoints each resource lazily. Default checkpoints clone persistent cold resources. Hot owners record a row-journal mark and their small metadata; failed nested work restores both. Successful inner work retains undo until the enclosing phase commits. Queries borrow the committed map and reject mutation. Request errors, validation failures, and worker panics restore the map before returning to the caller.

Bodies, motivations, personal knowledge headers, positions, actions, and the reverse permission index use dense indexed rows. Bulk physiology traverses physical rows using the bounded worker pool. Death effects explicitly sort IDs before dispatch. Physical compaction never becomes an identity or arbitration order. Scenes share visible roads/patch payloads while keeping source/time/revision/confidence personal. Explicit reports, deliberate stock inspection, visibility, stale knowledge, and per-patch hooks retain their semantics.

The default ID index uses 256-entry pages with sparse/dense representations. It avoids both tree lookup per component access and allocation proportional to the maximum ID. `hash-index` compares a per-row hash map with ordered keys. Extremely sparse IDs can favor that alternative. `compact-storage` grows row capacity more tightly and releases bulk undo buffers; the fast default retains reusable buffers. These are storage policies under the same host, not different game engines.

Cloning a packed revision is constant-time. The first write while a complete revision is retained copies its dense rows and index. This shifts snapshot cost to the writer, and can enlarge the first committed boundary. Ordinary borrowed reads and small public observation DTOs do not hold complete revisions. The economy benchmark's `LEGACY_HOLD_READERS` mode retains six actual feature resources and checks their streaming fingerprints through advancement, births/deaths, and loading.

Storage commit/restore hooks are infallible bookkeeping, not gameplay callbacks. The row store coalesces writes within each savepoint, retains successful nested undo for outer rollback, reuses bulk buffers, and journals structural swaps. Cold resources keep their previous snapshot contract. Differential tests exercise both index implementations, gappy IDs, nested bulk/structural edits, retained readers, and injected worker panics.

The reference is `7a71e15` with benchmark-only setup for extra patches and initial historical facts. Its target directory is isolated from the working build. The comparison checks exact serialized logical state and continued execution after loading; storage policy and index slots are excluded from saves. Rich histories use shared templates; unique histories are not established by this fixture.

Reproduce the policy comparisons:

```sh
python3 tools/performance/run.py --workload economy --population 10000 20000 --workers 1
python3 tools/performance/run.py --workload economy --population 1000 --workers 1 --history 1000 --extra-patches 64
python3 tools/performance/run.py --workload economy --population 20000 --workers 1 --hold-readers
python3 tools/performance/run.py --workload economy --population 20000 --workers 1 --hash-index
python3 tools/performance/run.py --workload economy --population 20000 --workers 1 --compact-storage
python3 tools/performance/run.py --workload packed --allocation-profile
python3 tools/performance/run.py --workload packed --allocation-profile --hash-index
```

## Measured policy tradeoffs

The [full production measurements](README.md#production-measurements) compare the exact same logical simulation. Paged dense rows and shared scenes improve ordinary 10,000/20,000/100,000-person advancement by about 2.3–2.4×. Richer worlds with 64 extra patches improve about 20×; adding 1,000 shared historical facts per added character yields about 22× at 1,000 people. Canonical saves match the reference, including after structural changes and loading.

At 100,000 people, three-run one-worker medians:

| Policy | Simulated minutes/s | Active RSS MiB | Requested live heap MiB |
| --- | ---: | ---: | ---: |
| Previous persistent hot state | 17.5 | 167.7 | 125.5 |
| Paged index, reusable buffers | 41.4 | 169.3 | 140.9 |
| Per-row hash index | 24.1 | 214.1 | 160.1 |
| Paged index, compact buffers/capacity | 39.1 | 170.5 | 124.9 |

Heap counts use separate instrumented runs; throughput/RSS use ordinary builds. Compact storage demonstrates lower requested heap, not a guaranteed lower resident footprint. The default remains paged indexing with reusable buffers. At smaller ordinary populations this raises RSS about 35% for more than twice the speed; the richer 10,000-person fixture improves both speed and RSS substantially.

The [production index benchmark](benches/packed.rs) uses 100,000 `u64` rows, one million deterministic random lookups, and ten canonical scans. Physical insertion order is reversed. Three-run uninstrumented medians:

| ID distribution | Index | Random reads ms | Canonical scans ms | Requested heap MiB |
| --- | --- | ---: | ---: | ---: |
| Consecutive | Paged | 24.79 | 3.65 | 2.79 |
| Consecutive | Hash | 30.28 | 19.87 | 6.00 |
| One ID every 1,024 | Paged | 95.39 | 54.04 | 14.10 |
| One ID every 1,024 | Hash | 31.95 | 19.48 | 6.00 |
| One ID every trillion | Paged | 97.13 | 43.85 | 14.10 |
| One ID every trillion | Hash | 29.23 | 19.26 | 6.00 |

Paged indexes fit the measured worlds' clustered IDs. Hashing is about three times faster and uses less than half the heap for extremely sparse IDs. Both scale with occupied data, never the maximum ID. Future campaign ID distributions should determine the policy; one isolated dense result is insufficient. These index-only figures exclude the complete game and use requested heap from instrumented construction.

A retained complete revision shifts its copying cost to the first writer. At 100,000 people the measured six-resource snapshot changes active RSS from 169.3 to 227.5 MiB, with 41.4 to 40.9 minutes/s throughput. Creation is about 0.003 ms and the maximum measured advancement boundary stays below 9.0 ms. Those readers start at fixture initialization, before mature per-character heap payloads accumulate. Frequent new global snapshots are a different workload; use borrowed reads or focused public DTOs for ordinary queries.

The reference setup patch is retained at [reference/7a71e15-fixtures.patch](reference/7a71e15-fixtures.patch). Apply it to a detached `7a71e15` worktree to add the same rich inputs and retained-reader checks. Use a separate Cargo target directory: reusing one across old/new worktrees can reuse stale dependency artifacts.

## Where time remains

Before the typed-request migration, a sampled 20,000-person production run attributed about 76% of advancement samples to character reviews, including 28.5% in observation. Physiology ticks accounted for about 7.5%. These are inclusive samples, not additive phase timers. A follow-up profile after the production changes below, with 1,248 advancement samples, attributes about 81% to reviews, 32% to observation, 19% to remembering facts, and 8% to physiology. Position lookup accounts for about 8% of flat samples; permission lookup and knowledge-tree copying remain visible. Relative shares are not absolute phase timings. Whole-game throughput barely changes with additional workers.

Those earlier samples motivated the production changes above. A new 100,000-person profile has 3,064 advancement samples: reviews account for about 67%, observation 24%, and physiology 19% inclusively. Journalled knowledge-row editing accounts for about 11% of flat samples; schedule maintenance remains visible. Next experiments should reduce repeated row resolution/undo preparation, evaluate interval forecasts against their now-larger physiology share, and integrate independent preparation batches only with an explicit read/effect contract. A faster arithmetic kernel alone leaves most current work intact.

## Packed state and transactional undo

Run the checked comparison:

```sh
cargo bench -p game-performance --bench architecture --locked --offline
LEGACY_ARCH_POPULATION=100000 cargo bench -p game-performance --bench architecture --locked --offline
LEGACY_ARCH_HISTORY=0 cargo bench -p game-performance --bench architecture --locked --offline
cargo bench -p game-performance --bench architecture --features allocation-profile --locked --offline
```

The [shared kernel](benches/architecture.rs) uses the actual `Body`, `Fact`, physiology update, and utility-selection rules. It compares:

- **Persistent tables:** Ordered hot tables, personal fact trees, and nested immutable checkpoints, using the current storage implementation.
- **Packed journal:** Dense body and motivation arrays; separate immutable fact payloads and mutable numeric provenance; a stable-ID/row index; reusable body buffers; and bounded row-level undo. Cold history stays shared and immutable.

Every logical review phase processes at most 128 stable IDs. The workload includes selective reconsideration, scarce stock, nested failed attempts after writes, an injected phase panic, structural removal and insertion with ID rollback, a retained committed reader, a checkpoint, and continued execution. Structural changes occur both before and after loading. Packed physical order is reversed initially; deletion compacts slots, and loading rebuilds the ID index. Outcomes and event hashes match across layouts, repeats, and one/four workers. No storage slot becomes a game identity.

Original `f7c4cb5` three-run medians, one worker, 120 kernel steps, same laptop and release settings as the production benchmarks:

| People | History per person | Reconsidering per step | Layout of stock | Persistent ms | Packed journal ms | Ratio |
| --- | ---: | ---: | --- | ---: | ---: | ---: |
| 20,000 | 0 | 25% | 32 areas | 1,195.5 | 366.9 | 3.3× |
| 20,000 | 1,000 | 25% | 32 areas | 5,308.6 | 365.1 | 14.5× |
| 20,000 | 1,000 | 100% | 32 areas | 18,366.5 | 953.8 | 19.3× |
| 20,000 | 1,000 | 25% | One store | 5,331.1 | 385.4 | 13.8× |
| 100,000 | 1,000 | 25% | 32 areas | 28,103.5 | 2,358.9 | 11.9× |

The ratio covers advancement, validation, rollback, and structural changes. Snapshot creation and save/load have separate timings. Packed full-reader creation costs about 2.7 ms at 20,000 people and 20.3 ms at 100,000; persistent root snapshots are effectively constant-time. Thus an in-place journal needs explicit snapshot handling. Borrowed read views suffice during a joined worker phase; background readers need copied hot buffers or chunk sharing that copies only while a reader holds an older revision.

In allocation-instrumented 20,000-person runs with 1,000 histories and 25% reconsideration, requested live heap with a retained reader was 174.2 MiB for persistent tables and 16.2 MiB for packed storage; peak through checkpointing was 316.6/101.5 MiB. Without the histories those live values were 20.0/16.1 MiB. These are kernel heaps, including the retained reader, not whole-game RSS. The large-history fixture shares cold reports across people. It does not establish memory use for unique histories.

**Why it helps:** A timestamp update currently copies a personal tree path and a leaf containing immutable payload references and sometimes unrelated history. A nested attempt can copy neighbouring body or motivation rows again. The prototype changes only the relevant numeric columns and records old values for rollback. It performs fewer allocations and avoids copying unrelated references.

**Scope:** This is an infrastructure comparison, not another game engine. It omits the host's full dispatch, permissions, routing, timed actions, death effects, and other feature graphs. Motivation and stock interactions are fixture rules around the real numeric functions. Both adapters use the same compact checkpoint format, including the shared cold template once. Kernel ratios do not predict whole-game ratios. Both still execute review arbitration serially; four workers do not materially improve the complete kernel.

## Runtime requests and wire data

```sh
LEGACY_ARCH_MESSAGES_ONLY=true cargo bench -p game-performance --bench architecture --locked --offline
LEGACY_ARCH_MESSAGES_ONLY=true cargo bench -p game-performance --bench architecture --features allocation-profile --locked --offline
```

The [message probe](benches/architecture/messages.rs) compares the real host `Envelope` layout with a typed, reference-counted payload behind an extensible interface. It uses 20,000 cancellation-shaped requests. Both forms produce identical serialized bytes and results; loading checks each person's and job's identity. Typed construction also validates serialization to a sink. No global enum of feature requests is required.

Three-run medians:

| Operation | JSON runtime payload ms | Typed runtime payload ms |
| --- | ---: | ---: |
| Construct pending requests | 2.97 | 0.53 |
| Clone and invoke | 3.65 | 0.33 |
| Encode | 4.17 | 4.73 |
| Decode and bind | 8.32 | 11.05 |

Instrumented construction retains 13.7 MiB with JSON and 1.22 MiB with typed payloads. Allocation counts fall from 80,001 to 20,001. Invocation omits host owner checks and handler lookup in both cases; it isolates payload representation. The simple typed encoder materializes JSON on save, and the decoder binds after parsing, so this probe trades some serialization time for lower runtime cost. Streaming erased serialization is a further option, not measured here.

The production host now retains typed pending jobs and action cancellations in a two-word shared handle. Invocation clones the immutable request and enters the existing owner-checked executor; hooks and signals continue borrowing typed inputs. Each feature registers its decoder. JSON remains at save/client boundaries; loading binds raw payload JSON before exposing the host. Construction checks serializability without retaining JSON. The table above records the original isolated probe; composed measurements are in the [performance results](README.md#production-measurements).

Transaction-private jobs, cancellations, and notices now use append-only vectors with saved lengths for nested rollback. Cancelling an existing staged job records its vector index; rollback restores cancellation-log length, and commit removes cancelled rows while retaining order. Resource writes still use persistent snapshots. Owners use registry-local numeric IDs for runtime checks and retain names in saves; tests load identical state under reversed plugin registration order.

## Parallel preparation on a committed view

The [perception workload](benches/perception.rs) now compares serial and parallel calls to the actual provisioning owner on one immutable transaction view. It uses the host's bounded Rayon pool, alternates serial/parallel measurement order, preserves person/result order, compares every returned plan, and verifies that the complete saved state stays unchanged. The runner checks 60 cases: five knowledge/ecology layouts, one/two/four/eight workers, and three repeats.

```sh
python3 tools/performance/run.py --workload perception
LEGACY_PATCHES=64 LEGACY_LOCAL_PATCHES=64 LEGACY_WORKERS=4 cargo bench -p game-performance --bench perception --locked --offline
```

Three-run medians for 10,000 preparations, 1,000 people, fixed batches of at most 128 people:

| Knowledge | Food choices/person | Workers | Serial ms | Parallel ms | Ratio |
| --- | ---: | ---: | ---: | ---: | ---: |
| Small | 2 | 4 | 6.88 | 8.49 | 0.81× |
| Small | 65 | 4 | 125.63 | 49.99 | 2.51× |
| Small | 65 | 8 | 125.13 | 40.85 | 3.06× |
| 1,000 accounts + 1,000 maps | 65 | 4 | 197.65 | 70.32 | 2.81× |
| 1,000 accounts + 1,000 maps | 65 | 8 | 199.97 | 51.74 | 3.86× |

The small two-choice workload loses to serial at one, two, and four workers in 128-person batches. Single-worker parallel dispatch offers no consistent gain even for rich choices. A 512-person experiment reaches 4.43× with rich history and eight workers, but exceeds the current logical 128-person review boundary; it is not a substitute for that boundary.

Timing includes owner query dispatch, actual immutable knowledge/object/position reads, and result construction. Person IDs are already batched before timing. It excludes observation refresh, arbitration, action effects, and GPU conversion. Histories are shared templates, as in the ordinary perception fixture.

**Decision:** Retain serial preparation for tiny workloads. A further production phase pilot should prepare sufficiently rich independent queries in bounded workers, then apply receiver effects in stable order. It needs an explicit read-view contract or revalidation when earlier effects alter a prepared query's inputs. The experiment establishes preparation headroom, not whole-game parallelism; it is not wired into character reviews yet.

When parallel listeners are enabled, the character proposal path uses ordered
provider groups rather than speculating on an entire collection plus food query.
Independent providers draft in parallel; observation-producing providers run as
serial barriers. An unexpected effect retries only that provider and its
speculative suffix. Food planning runs
after all provider effects. This preserves feature ownership and the staged world
seen by later providers. It removes duplicated preparation when ordinary care
observations would otherwise force a complete serial retry. Tiny groups and shared
derived-cache contention still limit whole-history scaling; measure one worker as
well as the parallel path before choosing a worker budget.

## Analytical intervals

The [temporal probe](benches/architecture/temporal.rs) replaces repeated linear need updates with exact saturating arithmetic for an uninterrupted interval. It checks 5,000 configurations against `advance_body`, including zero/extreme rates, both kinds of body, rest, age limits, and clock overflow. It also verifies the first predicted natural-death boundary against the tick reference.

For 20,000 passive bodies over one day, the pure fold took about 89–96 ms and exact projection 0.27–0.40 ms. Harm is disabled in that timing fixture so no death interrupts the interval. At 100,000 bodies the values were 438.9/1.32 ms. This is a several-hundred-fold arithmetic gain on an uninterrupted interval; it does not skip intervening actions or events in a running game.

Physiology was about 8% before packed storage and is about 19% in the latest 100,000-person sample. Removing it entirely would give at most about 23% in that fixture, before replacement costs. An actual lazy implementation must measure the cost of projection on reads and maintaining event forecasts. Wider gains require event forecasts and materialization at every rate change, action, injury, meal, recovery, and death boundary. Passive queries must read the owning physiology phase's logical tick; another request reaching the same host timestamp must not advance needs early. Skipping character reviews without preserving their observations and provenance would change gameplay.

## Why a single history scales poorly

The measured baseline split parallel work into **providers for one character**,
while the scheduler and character loop remained ordered. The integrated runtime
policy below changes that decomposition; the committing actor loop stays ordered:

- `Host::advance` selects one due job, and `Host::run` completes its transaction
  before selecting the next. An owner batch combines adjacent requests into one
  transaction but still invokes them sequentially.
- `characters.review` can review up to 128 due people, in person order, inside one
  transaction. Each review refreshes evidence, gathers proposals, chooses, and
  executes before the next person reviews. Event-driven `characters.resume` calls
  also execute through the serial host.
- Providers accept `&mut Transaction`, not a pure decision input. Care proposals
  can call `Notice`, update perceived hunger, and discard obsolete commitments.
  They are not merely independent numeric scores. Immediate event subscribers
  can notify other owners and schedule earlier-priority work at the same time.
- Whole resources are the mutation boundary, even when the owner changes only one
  person. `Offers.active` is a singleton temporary collection. Two reviews cannot
  safely share it, or independently mutate the same resource instance.
- Action/job identities and notice sequences use global counters. Evidence stores
  the host transaction revision. These values survive saves, and identities feed
  deterministic ordering and randomness. Giving workers arbitrary ID ranges or
  splitting existing atomic batches changes observable state.

Some ordering is physical: two caregivers can feed the same person, characters
compete for a store or a conversation partner, and later observers can see an
earlier action's effects. Some ordering comes from the implementation: disjoint
people still share resource containers, counters, and the proposal buffer. Resource
ownership is a useful module boundary; it is not proof that the whole resource
must be one indivisible parallel task.

In the 96-founder seed-7 opening sample, reviews account for about 88% of advance
time: 47,704 reviews in ten days, averaging about 0.44 ms each inclusively. An
individual provider has a much smaller work budget. Forking scratch transactions,
collecting drafts, merging, cache accesses, and worker handoffs compete with that
budget. Four workers improved after ordered effect barriers removed duplicate
preparation, but remained slower than one. That is evidence against the present
task decomposition, not evidence that all character reasoning is inherently
sequential. Derived food/work caches also have global mutexes; thread-local caches
can duplicate preparation across workers. Their contribution requires measurement;
the presence of a lock alone does not establish the bottleneck.

The `parallelism_world` diagnostic records running queue widths, reviews per
transaction, and conservative resource accesses. A checkpoint alone is insufficient
to measure opportunity: the day-10 queue has 524 jobs in 522 distinct time/priority
slots, but executing jobs can create bursts of new same-time work. Resource access
sets deliberately distinguish neither individual rows nor physical locality. They
cannot authorize parallel commits, or estimate the final scaling limit.
An action completion and the review it schedules share a timestamp but form a
causal chain. Counting all recorded events at that timestamp as independent work
would overstate parallelism. Inspect the actually queued frontier and the number
of reviews inside each atomic transaction as separate quantities.

The running diagnostic confirms that newly created bursts matter:

| Ordinary continuation | Atomic boundaries | Review calls | Largest queued time/priority group | Largest review batch in a transaction |
| --- | ---: | ---: | ---: | ---: |
| Mixed habitats, day 10 → 12 | 15,805 | 11,158 | 26 | 3 |
| `warm-32`, day 300 → 330 | 11,876 | 7,243 | 10 | 1 |

Nearly every review runs as an individual `characters.resume` transaction rather
than a populated 128-person review loop. Yet more than one job is already queued
at the current time/priority in 54.0% of opening boundaries and 30.3% of older
boundaries. Equal-time preparation therefore has real opportunities; the initial
checkpoint's mostly distinct future timestamps do not rule it out. Whether those
jobs are independent still needs owner-level dependency validation.
For `characters.resume` specifically, another job is already queued at the same
time/priority in 68.0% of opening calls and 46.0% of older calls.

At resource granularity, 12,214 of 12,230 adjacent same-time pairs conflict in the
opening trace; the older values are 7,649 of 7,879. Almost every review writes
`characters.minds`, `choice.offers`, `knowledge.facts`, and `signals.experience`.
That granularity would serialize almost all work, including disjoint people. These
are conservative container conflicts, not measured physical conflicts between
individual people. The diagnostic's final complete canonical worlds match ordinary
replays for both continuations. Its `test`-profile
instrumented durations are not the optimized timings below.
The current cloud host has a four-vCPU quota; its worker measurements cannot
establish scaling to tens of cores.

On that Xeon Platinum 8573C host, a frozen ordinary-policy profiling build resumed the
same seed-7 day-10 checkpoint to day 12 in three rotated-order repetitions:

| Workers | Median wall seconds | Median CPU seconds | Median peak RSS KiB |
| --- | ---: | ---: | ---: |
| 1 | 6.573 | 6.528 | 86,288 |
| 2 | 7.038 | 8.418 | 96,716 |
| 4 | 8.446 | 13.672 | 105,296 |

The optimized `simulation` build uses Rust 1.97.1 and bundled mimalloc. Timing
includes loading, advancement, final world/history artifacts, and process exit;
canonical hashing is outside timing. All nine runs match the original executable's
complete canonical world and exact ledger, event, and individual hashes. Four
workers spend about twice the CPU time and take 28.5% longer than one. This is a
failure to amortize the current parallel path, not just idle unused cores. It does
not identify how much of the overhead comes from dispatch, caches, or allocation;
the sampled profile and conservative dependency trace address different parts of
that question.
Three final alternating baseline/candidate pairs with one worker gave
7.048/7.055 seconds respectively, with 6.962/6.953 CPU seconds and matching complete
replay hashes. This is effectively unchanged, with visible run-to-run variation;
there is no demonstrated improvement to the fastest worker configuration. The
parallel-path optimization and opt-in model experiments do not yet establish a
drastic ordinary-policy speedup.

A GNU gprofng sample of the same four-worker continuation attributes 16.2% of
exclusive CPU samples to `sched_yield` and 16.9% to `syscall`. Work stealing,
crossbeam epoch bookkeeping, and allocator maintenance also appear among the flat
hotspots. This supports larger task granularity and fewer synchronization rounds.
The inclusive `WorkerThread::wait_until_cold` stack also contains productive child
work; its large inclusive percentage must not be labeled idle time. These are
sampled attributions, not a precise partition of elapsed time, and do not establish
that a particular derived-cache mutex is the dominant bottleneck.
The caller view attributes about 80% of the `syscall` samples to Rayon's worker
wakeup path; the rest mostly comes through condition-variable waits and latch
notification. All sampled `sched_yield` calls come through the worker wait loop.
This is direct evidence of executor synchronization costs, rather than an
inference from the presence of cache mutexes.

### Architectural alternatives

| Change | Useful parallel unit | Behavior contract and limiting factor |
| --- | --- | --- |
| Persistent executor placement and larger provider chunks | Successive ordered phases and groups of providers | Keep the serial host on a pool worker instead of repeatedly crossing into the pool for tiny groups. Preserve callback order and effects. This can reduce handoffs but does not create new logical independence; benchmark the complete path. |
| Incremental preparation by owner | Changed inputs and affected proposal families | Reuse structural alternatives and recompute only affected motives. Complete dependencies must include observation effects, time-sensitive inputs, missing records, and competitors. A full serial fallback preserves correctness; acceptance and invalidation costs need measurement. |
| Larger batches of pure decision preparation | Several actors, preferably in independent places | Preserve event order by validating owner-produced input versions before applying each result. Invalid results recompute on the current world. Input preparation, validation, and retries must be included in timing. |
| Owner state partitioned by person/place | Disjoint rows or local scenes | Keep each feature as the writer of its state. Track row and collection versions, including negative lookups and iteration membership; resource-level conflicts are too coarse. Interactions still need stable arbitration. |
| Region event queues | Independent local timelines | Cross-region reads, messages, travel, institutions, and shared stores need explicit dependencies. Current APIs provide no guaranteed minimum communication delay, so conservative simulation may have zero lookahead. |
| Optimistic event execution with rollback | Future jobs on independent state | Requires read/write tracking, reversible queued effects, stable ID handling, and cancellation across speculative events. Existing nested rollback protects one transaction; it is not a distributed rollback protocol. Dense care/social interactions may cause repeated invalidation. |
| Fixed decision frames and delayed messages | A whole population per frame | A larger, simpler parallel batch, but actors see a shared earlier world. Competition, evidence, and response timing change. The five-minute review experiment demonstrates the need to compare survival and behavior as well as elapsed time. |
| Independent history processes | Separate seeds or scenarios | Already independent and preserves each world's rules. Improves ensemble throughput, not the elapsed time of one history. Memory and CPU quotas bound useful concurrency. |

Region partitioning alone is bounded by the number of independently busy regions
and by the busiest region. A world with a few crowded places still needs parallel
work within those places to use tens of cores. Assigning one thread to each feature
owner also does not remove synchronous cross-feature dependencies.

### Recommended first pilot

Separate owner-produced **read inputs**, **pure proposal preparation**, and
**ordered application**. Give each actor a private proposal buffer. Owners publish
versioned decision inputs and validate them through their own interfaces; the
kernel should not learn food, household, or relationship schemas. Retain the
current transaction boundaries, timestamps, provider barriers, canonical person
order, cancellation semantics, and ID allocation in the application stage.
Keep dependencies and prepared results per provider/query family: one changed care
observation should not force every unrelated alternative for that actor to be
prepared again. Personal knowledge can remain unchanged even when a shared physical
store changes; preserve that epistemic distinction when choosing validation tokens.

Prepare bounded actor batches speculatively, then accept each result only if its
complete dependencies still match the world at its existing execution position.
Validation must cover negative queries, collection membership, time, and any
earlier provider's observations, not just versions of returned rows. Newly
scheduled earlier-priority work ends a captured batch. An input mismatch discards
only affected preparation and recomputes through the serial owner path. Immutable
input DTOs should retain the needed rows/scenes rather than entire mutable resource
snapshots: retaining a dense whole-world revision can force expensive copy-on-write
on the next mutation.

If the running trace exposes few actors at equal times, equal-time batching alone
will not produce broad scaling. Evaluate preparation ahead of known future
decision opportunities, with each request's own logical time and complete input
validation; intervening physiology/environment phases can invalidate that work.
Many reviews are discovered only when an action finishes, so lookahead also needs
owner forecasts or reusable query preparation, not just a wider queue iterator.
Preserve distinct timestamps and canonical execution positions. Treating nearby
events as simultaneous is the separate fixed-frame model option.

Start with one expensive query family and measure acceptance rates on actual
histories before migrating all providers. Compare one/two/four workers on identical
checkpoints, including observation, input construction, validation, retries,
arbitration, application, and save costs. Check complete canonical worlds and exact
ledgers, event journals, and individual snapshots across workers, checkpoint
continuation, and advancement budgets. Use crowded shared-resource and dispersed
worlds: success on disjoint actors does not establish contested-world scaling.

Make the new preparation contract opt-in per owner, alongside the existing serial
path. Do not copy expensive reasoning into the serial input-building stage. If
coarse versions reject too much work, first distinguish disjoint rows/local scenes;
an owner may later supply a narrower validity certificate, such as an unchanged
query result under a stock threshold, where it can prove that motives and offers
are identical. A successful execution precondition alone is insufficient: a
different alternative might now score better. These boundaries can also support
exact incremental reuse with one worker.

This pilot creates reusable modular boundaries for a later region scheduler or
optimistic engine. It can preserve the existing model. A fixed-frame model remains
a separate option to evaluate explicitly if exact preparation cannot expose enough
independence; it should not be introduced as a scheduler optimization.
The five-minute policy still uses the serial execution engine. Its measured gain
does not establish the potential or limits of a properly batched fixed-frame
architecture.

### Queue-preparation prototype

These measurements describe the initial prototype, before the integrated runtime
changes in the following section.

The opt-in history pilot now exercises this contract for **livelihood recipe
search**, rather than speculating an entire character transaction:

1. `game-anticipation::Window` captures a bounded equal-time, equal-priority queue
   frontier and inspects typed `characters.resume` payloads. It never consumes
   jobs. The history driver invokes it between the existing committed boundaries.
2. `Host::query_at` provides a read-only current world with the due logical clock.
   The livelihood owner selects personal inventories and goals. Work planning
   captures private perceived supplies, patches, numerical attributes and catalog
   references, then dispatches cold independent searches across the worker pool.
   The pure search receives no transaction or handler-dispatch capability.
3. All workers join before the query releases its borrowed world. Only bounded
   plans and weak dependency versions survive into canonical execution; no dense
   whole-world revision is retained across a mutation.
4. Ordinary observation, provider barriers, scoring, execution and commits retain
   their original positions. Work planning recaptures all search dependencies at
   use. Matching results are accepted; stale results follow the serial search.
   Physical stock changes alone do not update a character's remembered supplies.

`--prepare-window 32` enables the pilot; zero is the default. Windows are bounded
by 128 queued jobs, including jobs of other types. Duplicate actor wakeups are
prepared once. A changed phase or a new queue frontier can trigger another batch.
Inserted earlier-priority jobs, cancellations, failures and save/load use the
existing dispatcher. Advisory query errors and panics do not suppress dispatch.
The new crate owns orchestration contracts; livelihoods owns query selection;
work planning owns capture, pure search, validation and its bounded result cache.
There is no new saved resource or plugin manifest entry.

This first family is deliberately narrow. Existing work-plan caching already
removes many searches: an earlier instrumented opening run had 36,168 planning
queries, 23,039 cache hits and 13,129 searches, with about 0.52 seconds inclusively
inside search. Broad single-history scaling requires migrating substantially more
expensive decision families and measuring how much preparation survives actual
observations. A queue's actor count is an opportunity bound, not the number of
cold searches available to workers.

Run profiling separately with `LEGACY_PROFILE=1`: `work preparation already
cached`, `work prepared search`, `work preparation parallel batch`, `work
preparation reused` and `work preparation stale` distinguish avoidable speculative
work, cold searches, concurrent opportunities, accepted results and invalidation.
Profile timing includes diagnostic locking and is not a benchmark.

The enabled four-worker diagnostic replays match the baseline's complete canonical
world and exact ledger, journal and individual snapshot hashes. Owner counters:

| Replay | Preview batches / maximum actors | Cold searches | Accepted / stale | Batches with multiple cold searches on workers |
| --- | --- | --- | --- | --- |
| Seed 7, days 10–12 | 3,678 / 26 | 1,190 | 1,028 / 161 | 73 |
| Seed 11, days 300–330 | 4,026 / 9 | 365 | 331 / 34 | 28 |

Already cached queries skip speculative search (7,086 opening and 6,118 older
queries). Some prepared results need not be used before the replay ends. These
counts demonstrate acceptance across entities, but only about 2.0% and 0.7% of
preview batches respectively contain parallel cold work for this family. Broader
coverage and cheaper capture/validation are necessary for a substantial speedup.

Frozen optimized binaries were replayed in three alternating rounds, with profiling
disabled and no concurrent builds or simulations. The machine exposes five CPU
affinity slots with a four-CPU quota. All 36 replays (previous binary, new binary
with preparation off, new binary with window 32; one and four workers; both
checkpoints) match complete canonical world, ledger, journal and individual
snapshot hashes.

| Replay / workers | Previous binary median | New, disabled | New, window 32 |
| --- | --- | --- | --- |
| Opening / 1 | 7.300 s | 7.860 s | 7.331 s |
| Opening / 4 | 14.674 s | 14.330 s | 13.807 s |
| Older / 1 | 2.624 s | 2.609 s | 2.694 s |
| Older / 4 | 3.677 s | 3.624 s | 4.286 s |

Run variation is substantial. For example, opening one-worker disabled replays
range from 6.913 to 8.553 s, and enabled replays from 7.166 to 8.663 s. Older
four-worker disabled replays range from 3.617 to 4.051 s, and enabled from 3.696 to
4.457 s. The overlapping ranges do not establish a reliable opening speedup or a
default performance improvement. The older enabled median regresses, and four
workers remain slower than one. **Keep the prototype disabled by default.** This
vertical slice validates preparation across actors and exact replay, not a claim
that one history is now massively parallel. Expanding capture to expensive actor
queries and observation projections, with fine-grained validation, is the next
implementation target.

Scratch reports: `target/history/optimization/prototype-opening/comparison.json`,
`prototype-older/comparison.json`, `preparation-profile-opening/summary.json`,
`preparation-profile-older/summary.json`, and `region-{opening,older}/report.json`.
`prototype-bench.py` in the same scratch directory records frozen binary and
checkpoint hashes and checks every replay before accepting timings.

### Integrated advancement and expression reuse

Entity preparation now belongs to the main `Simulation::advance` path through
`game-anticipation::Window::advance`. Disabled preparation delegates directly to
the host. Enabled preparation runs between each existing committed boundary,
including calls with multi-boundary budgets. Stop checks, progress, revision and
notice behavior are preserved. `set_preparation_window` configures the main API;
the history runner no longer runs its own preparation loop. Runtime settings and
statistics are not saved and can be reapplied after loading.

The kernel now runs proposal subscribers in canonical order by default, avoiding
per-character tiny-task fan-out. Large owner operations and entity preparations
still use the bounded pool. `set_parallel_listeners(true)` (history flag
`--parallel-listeners true`) restores the earlier isolated parallel drafting for
controlled comparisons and sufficiently rich custom providers. The ordinary
one-worker execution path remains the behavioral reference.

Developmental arithmetic now lives in the pure `game-expression` module.
`game-vitality` still owns state and reexports `Life`, `Upbringing` and maturation;
the wire representation is unchanged. The existing bounded per-thread expression
frame now also reuses the six modulated dispositions. Keys include the complete
life value, exact clock, person, attribute and raw inherited potential. It retains
no world snapshot or resource alias, and handles changes within transactions,
rollback and different worlds through exact inputs.

Validation includes 124 selected regression/assembly tests, including direct
main-API preparation, budgets, stops, save/load, formula equivalence, both callback
policies, aggregate dispatch limits and rollback. Source budgets, dependency
constraints, Clippy and formatting pass without relaxing architecture limits.

The integrated build is compared against the frozen queue-preparation prototype
above, in a new timing session. Both use Rust 1.97.1's `simulation` profile. Three
alternating-order rounds run sequentially, with profiling disabled and no builds
or other simulations running alongside them. Times include checkpoint loading,
advancement and output, excluding prefix construction and subsequent hash checks.
The host has a four-CPU quota; these results do not establish scaling beyond that.

The opening and older inputs are the same unchanged checkpoints used above. A
third input starts with 1,000 founders in 32 areas (only those world-generation
settings differ from default content), seed 23, and replays days 1–2 from an
untimed one-worker prefix with 995 survivors. It checks a larger population rather
than a mature 1,000-person society.

All **54 replays match** complete canonical world and exact ledger, journal and
individual-snapshot hashes: seven configurations at each smaller checkpoint and
four at the large checkpoint, each repeated three times.

| Replay / workers | Previous binary wall median | Integrated default wall median | Wall reduction | CPU reduction |
| --- | ---: | ---: | ---: | ---: |
| Opening / 1 | 6.268 s | 6.278 s | −0.2% | −0.1% |
| Opening / 4 | 8.471 s | 6.072 s | 28.3% | 55.3% |
| Older / 1 | 3.475 s | 2.854 s | 17.9% | 17.1% |
| Older / 4 | 5.356 s | 2.822 s | 47.3% | 63.8% |
| 1,000 founders / 1 | 45.500 s | 46.308 s | −1.8% | −1.8% |
| 1,000 founders / 4 | 61.787 s | 45.773 s | 25.9% | 48.8% |

Absolute timings vary, particularly for the older and large worlds. Four-worker
previous/integrated ranges are 8.315–8.497 / 6.072–6.093 s (opening),
4.639–8.193 / 2.705–3.714 s (older), and 60.014–72.974 / 45.573–53.737 s (large).
Each four-worker comparison improves in all three rounds. The older one-worker
result is promising but variable; opening and large one-worker results do not
establish a consistent improvement. This change primarily removes the penalty
for selecting four workers, rather than dramatically accelerating the previously
fastest single-worker configuration.

The same integrated binary also runs with the former parallel-listener policy
restored. This isolates callback scheduling from the module/cache changes. On the
opening replay, four-worker medians are 8.181 s with parallel listeners versus
6.072 s with ordered listeners; CPU time falls from 13.164 s to 6.035 s. Small
per-provider tasks were increasing overhead rather than supplying useful parallel
work. Removing that overhead does not make a whole history execute concurrently.

The arithmetic reuse has no established standalone throughput gain: opening
one-worker medians are effectively unchanged. Entity preparation at window 32 is
also inconclusive: four-worker opening/older medians are 6.332/2.756 s versus
6.072/2.822 s with preparation disabled. Keep it opt-in. Broader pure preparation
coverage and owner-produced dependency contracts remain the larger architectural
opportunity; do not add finer callback tasks to pursue it.

The measured executables are `target/history/optimization/candidate-preparation`
and `candidate-integrated`. The sequential matrix driver is `integrated-bench.py`
in that directory. It records executable/checkpoint SHA-256 values and uses
`tools/history/state_hashes.py` to check every complete world, ledger, event journal
and individual snapshot. The integrated policy flags are `--prepare-window 0`
and `--parallel-listeners false`; policy-ablation runs set the latter to `true`,
and entity-preparation runs set the former to `32`. Reports are
`integrated-{opening,older,scale}/comparison.json`. Earlier-round duplicate worlds
are removed only after hashing; the last round is retained for inspection.

### Checked capability projections and bounded offer updates

A follow-up single-worker sample of the integrated 1,000-founder continuation
still spends most advancement work inside character reviews. Capability and
readiness queries, trait projections, allocation and collection sorting appear
among the hot paths. Pure arithmetic memoization alone leaves repeated owner-row
resolution and baseline-trait lookup in front of every cached answer.

`game-vitality::projection` now validates the life table, trait definitions and
personal profile sources before reusing an expressed capability or readiness
result. Entries also check person and exact logical time. All tokens are weak;
the bounded 128-frame thread-local cache retains numeric results rather than
world roots or mutable aliases. Direct definition/profile/life edits, nested
rollback, loading, clock changes and another world invalidate the relevant frame.
Unmodulated traits bypass the life lookup. `game-expression` owns the formula
attribute slots and continues to supply the authoritative pure arithmetic.

Offer retention now compares a full collection's incoming candidate with its
worst entry before changing the heap. A retained candidate replaces the root
directly; an uncompetitive candidate avoids insertion and immediate removal.
Duplicate checks, changing weights, continuation preference, full-key ties, hooks
and dispatch counts retain their existing meaning. The independent scan oracle
continues to check saturation, collisions, changing weights and returning keys.

The compiler experiment uses the existing `tools/performance/pgo.py` workflow,
portable CPU targeting, release ThinLTO and one codegen unit. Training uses fresh
seeds 17 (`warm-32`), 29 (default content) and 43 (1,000 founders / 32 areas).
Validation checkpoints use seeds 7, 11 and 23, outside that training set. Compare
the ordinary `simulation` build separately to isolate source changes; comparisons
with the profile-guided release executable combine PGO, ThinLTO and codegen-unit
changes. An additional ordinary release executable, with the same source,
ThinLTO and codegen-unit settings, isolates PGO's contribution and checks whether
it improves on the existing release configuration.

All **60 timed replays match** complete canonical worlds and exact ledgers,
event journals and individual snapshots. Three alternating-order rounds run
sequentially after all compilation and training finish, with profiling disabled.
The smaller checkpoints compare all four executables with one and four workers;
the 1,000-founder checkpoint compares all four with one worker. Four additional
untimed checks verify the ordinary and PGO executables on the smaller checkpoints
while the release control builds; their durations are excluded from these results.

| Replay / workers | Previous `simulation` | Updated `simulation` | Updated release | Updated PGO release |
| --- | ---: | ---: | ---: | ---: |
| Opening / 1 | 6.145 s | 5.755 s | 5.128 s | 4.102 s |
| Opening / 4 | 6.144 s | 5.763 s | 5.253 s | 4.241 s |
| Older / 1 | 2.654 s | 2.499 s | 2.339 s | 1.713 s |
| Older / 4 | 2.660 s | 2.575 s | 2.274 s | 1.680 s |
| 1,000 founders / 1 | 43.580 s | 42.127 s | 36.038 s | 28.795 s |

Values are end-to-end wall medians, including load, advance, output and exit;
prefix construction and subsequent hashing are excluded. One-worker source
changes reduce these medians by 6.4%, 5.8% and 3.3% respectively. The larger-world
source effect varies: its three pairs improve by 3.3%, 9.6% and 0.1%, so its median
is an estimate rather than a promise of a fixed gain. The source change remains
enabled; its modules preserve exact input validation without adding world roots,
locks or new saved state.

PGO reduces one-worker time by **20.0%, 26.7% and 20.1% versus ordinary release**.
Relative to the previous research executable, the best one-worker results reduce
time by **33.2%, 35.4% and 33.9%**. One-worker PGO ranges are 4.094–4.178 s,
1.633–1.747 s and 28.674–29.122 s, versus ordinary release ranges of
5.038–5.146 s, 2.279–2.365 s and 36.010–37.834 s. The PGO benefit survives the
ordinary-release control on each held-out workload. Additional workers still
provide no consistent whole-history scaling benefit.

All 131 selected regression/assembly tests pass, along with Clippy,
formatting, source budgets and dependency constraints. New projection tests cover
staged life and trait edits, catalog changes, nested rollback, exact clocks,
colliding cache slots, missing resources/people, restored worlds and another thread.
No source-budget exceptions or simulation-rule changes are introduced.

Frozen artifacts and reports live under `target/history/optimization/`:
`candidate-integrated`, `candidate-projection`, `candidate-release`,
`projection-pgo/game-history`, `projection-{opening,older,scale}/comparison.json`
and `projection-bench.py`. `projection-pgo-training.json` records the training
arguments; `projection-pgo/metadata.json` records compiler, source/input/profile
and binary hashes. Both PGO compilation stages finish without warnings. Rebuild
training profiles after source or representative workload changes using the
[profile-guided build workflow](README.md#profile-guided-history-builds).

### Entity and region execution paths

Parallelism across entities is feasible: the queue-preparation pilot already
searches independently for several people. Its limited query coverage should not
be confused with the available scope of entity parallelism. A complete review
currently interleaves observations, choice providers, care and action effects;
those effects need an owner-produced dependency/delta contract before entire
reviews can safely execute concurrently.

Region partitions should preserve feature ownership. A feature can own many
physical shards without moving its rules into the scheduler. Candidate boundaries
are personal body/mind/knowledge rows, area-owned stores and patches, observer-owned
relationship rows, and immutable shared catalogs. Travel transfers authority;
cross-region requests carry explicit effects. Global queues, revisions, notices
and IDs still need deterministic arbitration if existing histories must replay.

Three implementations deserve separate evaluation:

| Approach | Behavior contract | Main work |
| --- | --- | --- |
| Private entity/region preparation with ordered application | Preserve current observations and event order through validation | Expand query coverage; publish complete row/scene dependencies and retain reusable private results. Works within a crowded area. |
| Speculative region transactions with ordered commits | Preserve behavior if read/write sets, IDs and queued effects validate | Partition owner storage and journals; stage effects; validate cross-region reads; retry conflicting work. Separate regions reduce conflicts but do not prove independence. |
| Independent regional epochs with delivered boundary messages | Can change observation and response timing | Give cross-region communication a positive delay or define snapshot epochs. Measure throughput, response delays and demographic outcomes together as an explicit model experiment. |

Current APIs do not guarantee positive cross-region lookahead. For example,
`game-testimony::Report` accepts arbitrary recipients and zero delay (the higher
`Tell` facade checks contact), and `game-property::Permit` changes the recipient's
permission index without checking location. Family, community and institutional
owners call permission requests. These are concrete contracts to route or validate;
they do not establish how often autonomous runs actually produce remote effects.

The new `region_world` probe measures real review locality while running the
ordinary engine. It reports area-attributed Resume reviews, distinct areas in the
ready queue and executed time/priority cohorts, population concentration, historical
relationship edges, and remembered remote stores. The last two are potential links,
not actual dependency traces. Co-located people can still conflict, and distant
people can share a global request. Per-row read/write tracing remains necessary
before claiming independent region execution.

A static check already shows why the two strategies should be combined: the
opening checkpoint has 89 living people: 87 in ten areas and two
travelling, while the older checkpoint has ten living people, nine in one area.
Geographic sharding alone is bounded by both active region count and the busiest
region. Fine-grained preparation within a region can supply additional concurrency.

The live spatial probe also preserves the complete baseline world in both replays:

| Replay | Located Resume boundaries with another ready area | Largest area's share of attributed full reviews | Executed time/priority cohorts spanning multiple areas |
| --- | --- | --- | --- |
| Seed 7, days 10–12 | 4 / 11,150 | 35.7% | 3 / 3,493 |
| Seed 11, days 300–330 | 0 / 7,242 | 96.2% | 0 / 3,828 |

Nine opening full reviews and one older review are not attributed to a located
Resume root. Equal-time queues contain many actors but almost always one area.
Geographic concurrency therefore needs preparation/execution across **different
logical timestamps**, rather than merely grouping the existing equal-time queue.
That motivates region-local timelines with dependency validation or guaranteed
communication lookahead, alongside entity preparation within busy regions. The
older replay's concentrated load limits geographic scaling even with independent
regions. These are workload measurements, not benchmarks of a regional engine.

### Deliberation policy evaluation

The opt-in `routines-30` policy repeats only selected short, receiver-validated
undertakings. It rarely finds eligible work in the measured opening: care plans
and raw mechanics experiments keep their original path. The broader `reviews-5`
policy additionally coalesces nonurgent full reviews, including care and new
information, while urgent hunger, injury, and arrival bring a review forward.
Actions still complete normally; choosing subsequent work can wait. Neither policy
is enabled in ordinary content.

On the cloud Xeon Platinum 8573C host (four-vCPU quota, Rust 1.97.1, `simulation`
profile, bundled mimalloc), three alternating-order repetitions gave these complete
one-worker continuation medians. Both variants load the same original prefix;
the experimental input changes only the deliberation rule. Timing includes load,
advancement, final artifacts, and process exit; prefix construction is excluded.

| Prefix and measured continuation | Ordinary seconds | Five-minute policy seconds | Wall reduction |
| --- | ---: | ---: | ---: |
| Seed 7, 96 founders, mixed habitats, day 10 → 20 | 25.171 | 21.249 | 15.6% |
| Seed 11, `warm-32`, day 300 → 330 | 2.707 | 2.332 | 13.8% |

These are different models: complete world/journal/individual hashes differ from
ordinary policy. Repetitions of each policy match, and a four-worker replay of the
five-minute policy matches its one-worker result. At day 20 the ordinary/five-minute
runs have 86/84 living people and 4/3 hungry people; at day 330 both have nine living
people, but ordinary/five-minute runs have 1/2 hungry people and 1/0 pregnancies.
These outcomes do not establish a general survival effect. Elapsed gains include
the different activity patterns and populations; they cannot be attributed solely
to cheaper review scheduling. The older prefix has only one recorded birth and
ten living people at its start, so it does not establish large-population or
multigeneration scaling. Keep the five-minute variant as an explicit behavioral
experiment, disabled in ordinary content.

## Migration sequence

1. **Typed pending messages — complete:** One request executor and the existing wire shape remain. Loading/binding, unregistered types, failed-job retries, cancellation, receipts, and retained snapshots are checked. Composed survival and memory measurements justify the production representation.
2. **Hot row journal — in production:** Bodies and other hot stores now use dense rows and undo; shared scenes separate observation payloads from personal evidence. Continue measuring further component owners. Feature-owned edit guards record old rows before returning mutable access. Nested savepoints retain inner undo until the enclosing phase commits; panic or validation failure reverses all writes and metadata. Cold records keep persistent snapshots. Avoid two authoritative copies of a component.
3. **Feature-owned phases:** Collect due IDs once, borrow packed read views, prepare independent observations/candidates in bounded workers, then resolve claims and receiver-owned effects in stable order. Keep logical phase boundaries fixed across CPU/GPU backends and worker counts. Content remains immutable runtime catalogs referenced by IDs.
4. **Forecast passive intervals:** Adopt analytical updates where the owning mechanic can enumerate every invalidating event. Keep cadence and knowledge semantics explicit. Keep inert/dead records out of hot sweeps. Region partitioning can follow declared cross-region reads and effects; it is not a substitute for those declarations.

Long-lived command receipts and skill-attribution records also need explicit archival or compaction rules that preserve retry and credit semantics. Packed storage does not bound an ever-growing history.

A wholesale ECS framework swap is not the first move. The previous archetype comparisons did not establish a whole-game advantage. Packed access, transaction cost, and phase dependencies are the concrete problems to solve; an ECS could supply part of that implementation if a composed pilot demonstrates a benefit.

GPU calibration remains [optional](../compute/README.md#backend-calibration). Calibrate actual phase sizes and include preparing inputs and reading results. Backend choice is runtime configuration, not gameplay state.

## Bounded memory of considered options

The next behavioral experiment retains a character's recently considered
non-survival options through `Rules.option_memory`. It is independent of the
routine and review-floor policies, and disabled in ordinary content. The
`options-5` overlay selects a five-minute lifetime in a new history. The history
policy comparison tool forks only the character rules in an existing checkpoint;
its reports explicitly reject equivalence with the parent model.

`game-autonomy-policy` owns the policy value and immutable considered-offer storage.
`game-characters` owns the saved mind field, invalidation and commitment. No new
resource, plugin, global cache, mutex or world snapshot is added. Immutable offers
are shared across transactional versions; a small set records consumed keys.
Continuation and failure scoring also live in the deliberation owner, keeping
character source below its existing 650-line budget.

Every ordinary review still observes surroundings and carried stores, searches
for food, scores hunger and fatigue, applies current trait weights and failure
costs, and preserves current action IDs and completion times. Injury, arrival and
abandonment clear the remembered options; urgent hunger bypasses them. A failed
reused start or a reused list selecting wait immediately collects fresh options.
Failed timed undertakings clear the memory through the existing resolved-outcome
handler. A successfully selected new option is consumed from the shortlist until
refresh. Current action continuations are reconstructed separately.

This remains a model change. Unchosen opportunities, their motives, and provider
observations or other collection effects can be stale until expiry. A consumed
opportunity can wait until refresh before appearing again. Receiver validation
checks execution, rather than whether an old motive would still win a fresh
comparison. A short delay can change action competition, learning, injuries and
later demographics. Daily snapshots and aggregate counts do not establish that
these histories are behaviorally equivalent or acceptable for every study.

A discarded first prototype repeatedly reused completed options. In the opening
checkpoint's two-day diagnostic, character-review calls rose from 11,158 to
32,991, while option collections rose from 11,157 to 11,603. Signals rose from
935 to 4,990. Consuming used options reduced the diagnostic to 16,029 review calls,
8,438 option collections and 1,237 signals. These instrumented runs overlapped
compilation; only their counts are attribution evidence, not their durations.
The diagnostic source and summary are retained under
`target/history/optimization/repeating-prototype-source` and
`options-smoke-profile`; the revised diagnostic is `shortlist-smoke-profile`.

The maintained `tools/history/decisions.py` compares a frozen executable's ordinary
policy with selected option-memory and review-floor intervals. It records source
checkpoint and ledger hashes, the executable and driver hashes, all policy
patches, independent wall/CPU/RSS measurements, checkpoint size, event-count
increments, and explicitly labeled daily sampled state. It alternates three
rounds, requires exact complete replay hashes within each policy, and separately
checks four workers, optional mid-run reload, and instrumented replays. It fixes
the horizon after extinction. Different-policy evidence cannot authorize a
behavior-preserving runtime upgrade. Timings include the ordinary history driver,
loading and final export; hashing occurs outside the measured process.

The full 256-option shortlist failed screening: it was slower on both small
checkpoints and took 58.45 seconds versus 39.65 seconds in the first 1,000-entity
five-minute run. That large screening was stopped after the ordinary, one-minute
and five-minute runs; its remaining rounds and replay verification are incomplete.
The complete small reports are `shortlist-opening` and `shortlist-older`; the
incomplete screen is `shortlist-scale`. Its binary and source fingerprint are in
`shortlist-runtime.json`. The next revision limits retained offers using current
trait weights, while every fresh review still considers the ordinary full list.
The 16/64-option opening screen is `capped-opening`; `capped-older` was stopped
after timing to enforce an explicit expiry wake during long actions. Those
versions expired memory only at the next review, so they did not bound the actual
response delay during a quiet long action. They are preliminary experiments.

The final policy schedules one guarded expiry wake upon first reuse, cancels it
on fresh collection, and ignores obsolete callbacks. Unused memory does not add
periodic full reviews. The character owner avoids a duplicate expiry response
when a same-time wake is already pending. The existing `Resume` wire key and
payload remain unchanged through a reexport from the deliberation protocol.
Routine/review-floor and option-memory policies cannot be combined, avoiding
conflicting deadline contracts. Regression tests cover the quiet-action bound,
checkpoint continuation with a pending expiry, injury invalidation, and expiry
coinciding with action completion.

Final comparisons also use a fresh measurement helper. Direct fork measurements
in the preliminary reports can include the Python parent's decoded-world RSS;
those preliminary RSS values are unsuitable for memory comparisons. Wall and CPU
measurements remain screening evidence. The helper's regression test allocates a
large parent heap and verifies that it does not inflate the measured child peak.

Both review-floor variants use the existing narrow routine kinds (`experiment`,
`reason`, `inspect-food`, `express-need`, `exchange-information`), with a 30-minute
minimum and two-hour maximum attention window. Their nonurgent wake response is
coalesced to the declared floor; an eligible routine without an invalidating
event can repeat for its longer attention window. These are explicitly different
policies, rather than a guarantee that every voluntary switch moves by only the
floor interval. Each report records the complete rule patch.

The final bounded-memory implementation is a negative result on the two small
checkpoints. On the Xeon Platinum 8573C container (four-CPU quota), Rust 1.97.1,
portable release ThinLTO / one codegen unit / no PGO, three alternating rounds
with one simulation worker gave:

| Continuation | Ordinary | Five-minute memory, 1 option | Five-minute memory, 16 options | Five-minute review policy |
| --- | ---: | ---: | ---: | ---: |
| 96-founder opening, day 10–12 | 6.161 s | 6.829 s | 6.891 s | 4.501 s |
| Older small population, day 300–330 | 2.292 s | 2.401 s | 2.624 s | 1.975 s |
| 1,000 founders / 32 areas, day 1–2 | 40.890 s | 44.251 s | 48.138 s | 9.774 s |

The 16-option opening policy executes 18,780 full reviews versus 11,157, while
fresh option collections fall only from 11,157 to 10,695. The older checkpoint's
16-option policy leaves fresh collections unchanged at 7,243 while adding 2,390
reuse reviews. Retaining proposals and guaranteeing bounded freshness does not
necessarily avoid the underlying work. Keep this overlay experimental and off
by default; prefer exact indexed query reuse for the next optimization.

The final binary and compiler/source fingerprints are in
`target/history/optimization/attention-runtime.json`; complete small reports are
`attention-opening` and `attention-older`. Complete within-policy replay checks
cover different worker counts, mid-run save/load and separate instrumented runs.
The default-policy control reproduces all four earlier replay hashes in six runs.
Its previous/candidate median is 5.791/5.466 seconds, with overlapping ranges;
this round establishes default replay compatibility, not a new default speedup.

The large report is `attention-scale`. Its ordinary timing range is
36.167–49.604 seconds and the five-minute review range is 9.529–13.214 seconds;
these are three-run medians on a variable cloud host, not confidence intervals.
CPU medians are 40.791 and 9.696 seconds. The review policy is 4.18 times faster
in elapsed time, but final living counts change from 977 to 993, production
completions from 1,554 to 616, new conceptions from 72 to 34, inquiry learning
events from 654 to 347, and treatment completions from 16 to 0. This is a young
large cohort, not evidence about a mature thousand-person society.

An execution-environment restart interrupted the second large round. Five fully
completed timing records were retained, incomplete outputs were discarded, and
the remaining rounds resumed with the same source, binary, driver and input
fingerprints. The report's `recovery` field records the recovery-script hash and
retained samples. All repeated, four-worker and instrumented output hashes match
within each of its four policies. Interrupted work contributes no accepted timing.

The [relational and linear algebra experiments](QUERY_PLANS.md) evaluate a
different route: express read-only preparation as selective joins and segmented
numeric reductions, then update those views from actual fact deltas. They do not
require retaining stale choices or delaying reviews.

Held-out screens compare only ordinary and review coalescing; the unsuccessful
option-memory policies were not extended. `attention-seed-19` continues a
different ten-person checkpoint from day 300 to 480. Ordinary/review-policy wall
times are 130.440/68.374 seconds in one pair, final living counts 10/8, births
3/3 (one new birth each), and production completions 843/184. Daily sampled hungry
person-days are 67/99; these are snapshot approximations rather than exact hunger
durations. `attention-seed-37` runs a seventeen-person checkpoint from day 300 to
330. It takes 16.336/16.954 seconds: no useful speed gain in that pair. Both end
with 17 living people, while injured counts are 4/6. Both screens reproduce all
four output hashes with four workers and a mid-run reload. Single timing pairs
are outcome screens, not precise speed estimates or multigeneration validation.
The large and longer histories show different survival effects, so these results
do not support enabling the policy as an ordinary runtime optimization.

Shorter review screens use one timing pair per checkpoint, plus exact four-worker
replays (and mid-run reloads on the small checkpoints):

| Screen | Ordinary | Changed policy | Final living | Production completions |
| --- | ---: | ---: | ---: | ---: |
| Opening, one-minute floor | 5.754 s | 5.264 s | 88 / 88 | 45 / 49 |
| Older, one-minute floor | 2.754 s | 1.844 s | 9 / 9 | 29 / 34 |
| Large cohort, one-minute floor | 43.651 s | 12.217 s | 977 / 992 | 1,554 / 590 |
| Opening, one-millisecond floor | 5.074 s | 4.315 s | 88 / 88 | 45 / 49 |
| Large cohort, one-millisecond floor | 36.625 s | 12.281 s | 977 / 992 | 1,554 / 686 |

Reports are `attention-review1-{opening,older,scale}` and
`attention-reviewms-{opening,scale}`. The millisecond floor is an integer
simulation tick, exposed by `decisions.py --review-millis 1`; the other routine
parameters remain declared in each patch. The original driver is archived as
`attention-driver-before-millis.py`; the later reports record the extended
driver's own hash. Ordinary-policy hashes match the three-round controls across
all shorter screens.

The millisecond policy reduces opening full reviews from 11,157 to 9,132. Its
large timing pair is about 2.98 times faster, but new conceptions change from
72 to 30 and treatment completions from 16 to 0. Small time displacements and
suppressing review feedback can still produce large history changes. Equal-horizon
policy timings include different amounts of activity; these are model tradeoffs,
not equivalent-work engine speedups. This supports investigating incremental
query invalidation and review causality before choosing a new ordinary policy.

Final validation covers 96 complete four-artifact replay checks across these
controls and screens, 94 selected Rust tests and 47 Python tests. Selected Clippy,
Rust formatting and architecture checks pass. The measured production source
and binary fingerprints remain unchanged. The SQL and arithmetic probes have
their own immutable driver/input fingerprints and exact-result checks.

### Review causality and predicate pushdown

The next investigation is recorded in [REVIEWS.md](REVIEWS.md). Optional generic
request/event/listener scopes produce bounded causal review-attempt telemetry;
ordinary builds compile out the backend. Saved minds live in `game-character-state`
while `game-characters` retains registration and writes. Wire state remains equal.

A bounded incremental appraisal-view prototype passes exact replay checks but
has mixed timings and remains opt-in. The default change is database-style
predicate pushdown in family assent: reject inadmissible bonds before expensive
read-only scoring. This removes 35–47% of visible physical appraisal queries while
old/new raw causal traces remain byte-identical on all three fixtures.

Portable-release three-pair medians improve 2.81% on the opening suffix and 1.47%
on the 1000-founder suffix. An initial older-history regression of 3.79% prompted
a five-pair CPU-pinned follow-up, which improves 1.73%; a separate component check
improves 4.71%. These are modest and timing-sensitive gains. Full artifact,
worker/reload, campaign, owner, architecture, Clippy and formatting checks pass.

Causal traces identify new-evidence notifications as 83–97% of accepted wakes,
with `care.food-review` and `nurture.heard-need` prominent in their chains. Numeric
proposal fingerprints repeat much less often than chosen keys; neither observed
state nor a fingerprint includes every decision input. Further exact reuse should
target partial provider computations with explicit dependencies. The optional
behavioral cadence policies remain separate from these engine optimizations.

### Food-care evidence and decision invalidation

[CARE_EVIDENCE.md](CARE_EVIDENCE.md) follows the wake-causality measurements with a
field-specific invalidation experiment. The new `game-care-perception` module
separates personal assessment from delivery. An opt-in rule records heard cues
but avoids new decision wakes for non-food cues or repeated food estimates,
except during an existing delivery. This changes which observations can interrupt
an actor, so it is a model option with measured history outcomes. It is not an
exact query cache. Both this option and the independent completion-preservation
trial remain absent from ordinary content; the kernel and arithmetic stay intact.

## Exact replay results and policy tradeoffs

[EXACT_REPLAY.md](EXACT_REPLAY.md) records the shared developmental-factor and
incremental information-validation pass, complete replay equivalence, fresh
comparisons with both ordinary release and the previous PGO executable, and the
source-only regressions. It also audits why the opt-in decision policies alter
histories and explains the next exact direction: owner-defined incremental query
views with unchanged observation effects.
