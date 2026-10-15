# Architectural headroom

**Decision:** Pursue compact typed pending requests first, then packed hot component stores with row-level undo. Keep persistent sparse records for histories, relationships, and other cold data. The experiments below support this direction; the production engine still uses its current storage and scheduling contracts.

## Where time remains

A sampled 20,000-person production run attributes about 76% of advancement samples to character reviews, including 28.5% in observation. Physiology ticks account for about 7.5%. These are inclusive samples, not additive phase timers. Whole-game throughput barely changes with additional workers.

The next architectural gain therefore needs to remove copying, lookups, and dispatch from observation and decisions, and expose independent preparation batches. A faster arithmetic kernel alone leaves most current work intact.

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

Three-run medians, one worker, 120 kernel steps, same laptop and release settings as the production benchmarks:

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

The production host already borrows typed internal hooks and signals. Pending jobs and action cancellation still retain JSON envelopes. Moving those to typed runtime payloads is a smaller migration than replacing all transaction storage, and targets repeated allocations and retained memory directly. Keep JSON at persistence and client boundaries, retain decoder registration per feature, and use the same owner-checked request executor.

## Analytical intervals

The [temporal probe](benches/architecture/temporal.rs) replaces repeated linear need updates with exact saturating arithmetic for an uninterrupted interval. It checks 5,000 configurations against `advance_body`, including zero/extreme rates, both kinds of body, rest, age limits, and clock overflow. It also verifies the first predicted natural-death boundary against the tick reference.

For 20,000 passive bodies over one day, the pure fold took about 89–96 ms and exact projection 0.27–0.40 ms. Harm is disabled in that timing fixture so no death interrupts the interval. At 100,000 bodies the values were 438.9/1.32 ms. This is a several-hundred-fold arithmetic gain on an uninterrupted interval; it does not skip intervening actions or events in a running game.

Physiology is only about 7.5% of the current advancement profile. Removing that cost entirely would improve this fixture by at most about 8%, before replacement overhead. Wider gains require event forecasts and materialization at every rate change, action, injury, meal, recovery, and death boundary. Passive queries must read the owning physiology phase's logical tick; another request reaching the same host timestamp must not advance needs early. Skipping character reviews without preserving their observations and provenance would change gameplay.

## Migration sequence

1. **Typed pending messages:** Preserve one request executor and the existing wire shape. Validate loading/binding, unregistered types, atomic failures, cancellation, receipts, and retained snapshots. Measure composed survival and memory before choosing the production representation.
2. **Hot row journal:** Pilot body rows and observation provenance. Feature-owned edit guards record old rows before returning mutable access. Nested savepoints retain inner undo until the enclosing phase commits; panic or validation failure reverses all writes and metadata. Cold records keep persistent snapshots. Avoid two authoritative copies of a component.
3. **Feature-owned phases:** Collect due IDs once, borrow packed read views, prepare independent observations/candidates in bounded workers, then resolve claims and receiver-owned effects in stable order. Keep logical phase boundaries fixed across CPU/GPU backends and worker counts. Content remains immutable runtime catalogs referenced by IDs.
4. **Forecast passive intervals:** Adopt analytical updates where the owning mechanic can enumerate every invalidating event. Keep cadence and knowledge semantics explicit. Keep inert/dead records out of hot sweeps. Region partitioning can follow declared cross-region reads and effects; it is not a substitute for those declarations.

Long-lived command receipts and skill-attribution records also need explicit archival or compaction rules that preserve retry and credit semantics. Packed storage does not bound an ever-growing history.

A wholesale ECS framework swap is not the first move. The previous archetype comparisons did not establish a whole-game advantage. Packed access, transaction cost, and phase dependencies are the concrete problems to solve; an ECS could supply part of that implementation if a composed pilot demonstrates a benefit.

GPU calibration remains [optional](../compute/README.md#backend-calibration). Calibrate actual phase sizes and include preparing inputs and reading results. Backend choice is runtime configuration, not gameplay state.
