# Architectural headroom

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

## Analytical intervals

The [temporal probe](benches/architecture/temporal.rs) replaces repeated linear need updates with exact saturating arithmetic for an uninterrupted interval. It checks 5,000 configurations against `advance_body`, including zero/extreme rates, both kinds of body, rest, age limits, and clock overflow. It also verifies the first predicted natural-death boundary against the tick reference.

For 20,000 passive bodies over one day, the pure fold took about 89–96 ms and exact projection 0.27–0.40 ms. Harm is disabled in that timing fixture so no death interrupts the interval. At 100,000 bodies the values were 438.9/1.32 ms. This is a several-hundred-fold arithmetic gain on an uninterrupted interval; it does not skip intervening actions or events in a running game.

Physiology was about 8% before packed storage and is about 19% in the latest 100,000-person sample. Removing it entirely would give at most about 23% in that fixture, before replacement costs. An actual lazy implementation must measure the cost of projection on reads and maintaining event forecasts. Wider gains require event forecasts and materialization at every rate change, action, injury, meal, recovery, and death boundary. Passive queries must read the owning physiology phase's logical tick; another request reaching the same host timestamp must not advance needs early. Skipping character reviews without preserving their observations and provenance would change gameplay.

## Migration sequence

1. **Typed pending messages — complete:** One request executor and the existing wire shape remain. Loading/binding, unregistered types, failed-job retries, cancellation, receipts, and retained snapshots are checked. Composed survival and memory measurements justify the production representation.
2. **Hot row journal — in production:** Bodies and other hot stores now use dense rows and undo; shared scenes separate observation payloads from personal evidence. Continue measuring further component owners. Feature-owned edit guards record old rows before returning mutable access. Nested savepoints retain inner undo until the enclosing phase commits; panic or validation failure reverses all writes and metadata. Cold records keep persistent snapshots. Avoid two authoritative copies of a component.
3. **Feature-owned phases:** Collect due IDs once, borrow packed read views, prepare independent observations/candidates in bounded workers, then resolve claims and receiver-owned effects in stable order. Keep logical phase boundaries fixed across CPU/GPU backends and worker counts. Content remains immutable runtime catalogs referenced by IDs.
4. **Forecast passive intervals:** Adopt analytical updates where the owning mechanic can enumerate every invalidating event. Keep cadence and knowledge semantics explicit. Keep inert/dead records out of hot sweeps. Region partitioning can follow declared cross-region reads and effects; it is not a substitute for those declarations.

Long-lived command receipts and skill-attribution records also need explicit archival or compaction rules that preserve retry and credit semantics. Packed storage does not bound an ever-growing history.

A wholesale ECS framework swap is not the first move. The previous archetype comparisons did not establish a whole-game advantage. Packed access, transaction cost, and phase dependencies are the concrete problems to solve; an ECS could supply part of that implementation if a composed pilot demonstrates a benefit.

GPU calibration remains [optional](../compute/README.md#backend-calibration). Calibrate actual phase sizes and include preparing inputs and reading results. Backend choice is runtime configuration, not gameplay state.
