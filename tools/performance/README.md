# Performance workloads

Run release benchmarks from the repository root. Each process checks its resulting records; timings alone do not constitute a pass.

Both Python build drivers default to one Cargo build job. `--build-jobs` raises
that limit explicitly. Compiler/linker threads are separate from Cargo jobs;
use the [bounded test scope](../testing/README.md) when the shared machine needs
an enforced CPU and memory budget. Do not overlap benchmark timing with builds,
tests or background simulations.

## Profile-guided history builds

`python3 tools/performance/pgo.py --training target/history/training.json --output target/history/pgo-build`
builds an instrumented history executable, runs the supplied workloads, merges their profiles, and builds an optimized executable at `target/history/pgo-build/game-history`. Install the compiler's matching profiler tools with `rustup component add llvm-tools-preview` first. The output directory must be new. `--restart-build` reuses compiler caches from an interrupted compilation before any training data exists; trained or completed output requires a new directory. Start without `RUSTFLAGS` or `CARGO_ENCODED_RUSTFLAGS`.

The training file contains ordinary history-tool arguments. Paths are relative to that file. Include varied populations and habitats; for example, a continuation of a checkpoint at day 18,250 and a new founding population:

```json
{
  "runs": [
    {"arguments": ["--resume", "source", "--days", "18615", "--sample-days", "365000", "--checkpoint-days", "365000", "--final-save", "false"]},
    {"arguments": ["--seed", "7", "--content", "../../content", "--mod", "../../tools/history/profiles/warm-32", "--days", "1825", "--sample-days", "365000", "--checkpoint-days", "365000", "--final-save", "false"]}
  ]
}
```

Training runs write only to tool-managed subdirectories. Logs, profiles, and checkpoints remain under the selected output directory. Metadata records the compiler, source fingerprint, training arguments, profile hash, and executable hash. Source changes during training or compilation reject the build. Rebuild profiles after changing mechanics or representative content.

Training timings are not benchmark results. Compare the frozen executable against an ordinary release build with [the history replay driver](../history/README.md), using longer continuations and workloads outside the training set. Require matching world, ledger, event, and individual hashes before using an optimizer in a continuing study. Gains depend on the workload and compiler; profile-guided compilation changes no simulation rules.

`python3 -m unittest discover -s tools/performance/tests -p 'test_*.py'` checks training-plan paths, output ownership, and source fingerprints.

```sh
python3 tools/performance/run.py
cargo bench -p game-performance --bench ledger --locked --offline
cargo bench -p game-performance --bench storage --locked --offline
cargo bench -p game-performance --bench perception --locked --offline
cargo bench -p game-performance --bench traits --locked --offline
cargo bench -p game-performance --bench architecture --locked --offline
LEGACY_POPULATION=10000 LEGACY_WORKERS=1 cargo bench -p game-performance --bench economy --locked --offline
LEGACY_POPULATION=10000 LEGACY_WORKERS=4 LEGACY_DISTRIBUTED=true cargo bench -p game-performance --bench economy --locked --offline
LEGACY_POPULATION=10000 LEGACY_SOCIAL=true cargo bench -p game-performance --bench economy --locked --offline
LEGACY_POPULATION=10000 cargo bench -p game-performance --bench latency --locked --offline
cargo bench -p game-performance --features allocation-profile --bench economy --locked --offline
```

Use `LEGACY_POPULATION` values 15, 1000, 10000, and 20000, and worker counts 1, 2, 4, and the available bounded budget. Repeat each economy and latency run at least three times. `LEGACY_MINUTES` defaults to 120. The storage bench runs this population/worker matrix with three repeats and distributed/contested stock automatically.

The CLI and performance fixtures default to bundled [mimalloc](https://github.com/purpleprotocol/mimalloc_rust); building them needs a C compiler. Feature libraries select no global allocator. Use `run.py --system-allocator` or Cargo's `--no-default-features` for a controlled system-allocator comparison. Metadata identifies the allocator. Optional allocation instrumentation wraps the selected allocator, preserving that choice.

**Storage:** The shared protocol fixture includes needs, progress, scoring from four known choices, two sparse relationship edges per person, stable transfer arbitration, arrivals, membership changes, structural replacement, observation extraction, and a checkpoint followed by continued execution. It compares canonical records, conserved quantities, and event hashes across workers and reversed evaluation output. This is a storage workload; its progress and transfers are fixture rules. It does not run the complete economy.

**Economy:** The composed fixture runs actual character, knowledge, physiology, cargo, and transport features with a shared warehouse or 32 settlements. It checks live bodies and actual food consumption, saves and reloads, then compares continued canonical state with different advancement budgets. A separate measured phase creates actual births and deaths, reserves capacity, and starts deliveries. The checkpoint retains those in-flight shipments and checks their eventual arrival and quantity conservation. Output includes fixture counts, throughput, committed-boundary latency, save/load time and size, and a state hash. Pending choices and invocation semantics remain covered by feature and assembly tests.

`LEGACY_SOCIAL=true` enrolls the same population in ordinary social life with varied sociability and empathy. The economy benchmark additionally checks completed encounters and prints participation counts. Searches use bounded, rotating local neighborhoods; timed conversations share the ordinary action clock with survival and travel. The replay test includes a crowded 257-person social world with different worker counts and advance budgets. Existing production measurements below predate this option.

**Ledger:** Actual cargo requests fill disjoint inventories or one shared reservation book. The benchmark rejects an over-capacity request without consuming its identity, reloads pending reservations, then releases half and fulfills half. It checks final quantities, capacity accounting, and canonical state at 15, 1,000, 10,000, and 20,000 reservations.

**Latency:** A real `Session` receives 100 pauses and public look commands during catch-up in the shared-store fixture. It reports acknowledgement p50/p95/p99 and maximum latency. The session services input after each committed request. A single request still determines the longest possible wait.

**Perception:** The runner uses 1,000 people with either no historical facts, or 1,000 shared historical accounts and 1,000 obsolete maps per person, plus one or 10,000 world patches. It also measures 64 local patches with and without histories. `LEGACY_LOCAL_PATCHES` defaults to one and cannot exceed `LEGACY_PATCHES`. Ten rounds inspect surroundings, generate actual provisioning options, and route along a known road. It checks exact choices, quantities, fact counts, and canonical hashes across workers. The development comparison also checked complete logical state against a temporary frozen checkout. Shared cold histories test bounded updates and namespace indexes; they do not establish memory use for unique personal histories.

**Traits:** The actual trait owner installs 1,000 definitions and 1,000 profiles with every attribute populated. Ten rounds read the effective weights, compare them with an independent integer reference, then verify rebuilding after save/load. Profile construction is outside the timed read phase.

Every benchmark prints hardware, OS, compiler, build mode, and worker metadata. Linux process RSS includes allocator overhead and thread stacks. Optional allocation instrumentation reports allocation counts, requested live heap bytes, and peak heap bytes, including working buffers and observations. It adds atomic accounting overhead; compare throughput with the default build. Save/load temporarily retains a second simulation and serialized snapshots.

## Storage choice

Production hot components now use [game-packed](../../crates/game-packed/README.md). [game-storage](../../crates/game-storage/README.md) supplies persistent cold records. The following framework comparison records the earlier storage decision before row journals and shared scenes. Both integrate with owner-checked transactions, incremental validation, and nested rollback. Content catalogs remain definitions, independent of component types.

Standalone Bevy ECS 0.20.0 and Legion 0.4.0 were compared with the same packed protocol, pure physiology rules, Rayon worker budget, stable IDs, operation order, and canonical output. Both ECS adapters used cached queries, batch creation, and a stable-ID/entity map. Each run retained the same canonical reference output. The host retained phase scheduling; this comparison does not rank their native schedulers. Bevy committed the packed workload faster. Dense storage retained less memory and integrates atomic snapshots without an additional record journal or ECS ownership layer. This integration and memory tradeoff determined the choice. Losing adapters and dependencies are absent from the workspace. APIs were checked against the [Bevy ECS documentation](https://docs.rs/bevy_ecs/0.20.0/bevy_ecs/) and [Legion documentation](https://docs.rs/legion/0.4.0/legion/).

On the measured laptop (i7-8550U, 4 cores/8 threads, 7.15 GiB RAM, Linux x86_64, Rust 1.98.1, release optimization), the three-run medians for 10,000 people, four workers, distributed stock, and 60 protocol steps were:

| Candidate | Total ms | Retained heap MiB | Peak heap MiB |
| --- | ---: | ---: | ---: |
| Dense | 198.0 | 5.30 | 12.64 |
| Bevy ECS | 144.3 | 6.07 | 14.45 |
| Legion | 155.8 | 5.83 | 13.65 |

These comparison runs instrumented allocations equally; their elapsed times include that overhead. The protocol retains cold records and a canonical reference, and includes packing, evaluation, stable arbitration, commit, structural changes, and a checkpoint. One-worker dense time was 361.5 ms, so four workers improved this parallel workload by 1.83×. Dense trades some packed-loop speed for a smaller footprint and direct immutable transaction snapshots. The earlier physiology path updated staged leaves directly. Current production physiology uses indexed dense rows with reusable undo buffers.

## Production measurements

Measured on the i7-8550U laptop, 4 cores/8 threads, 7.15 GiB RAM, Linux, Rust 1.98.1. Release builds use optimization level 3, thin LTO, one codegen unit, portable CPU defaults, and bundled mimalloc. These are three-run medians for 120 simulated minutes, one worker, default paged indexing and reusable buffers.

| People | Layout | Simulated minutes/s | Active RSS MiB | Peak RSS through save/load MiB |
| --- | --- | ---: | ---: | ---: |
| 1,000 | Shared store | 5,126.3 | 21.5 | 49.7 |
| 1,000 | 32 settlements | 4,925.8 | 21.4 | 39.6 |
| 10,000 | Shared store | 512.3 | 39.6 | 143.7 |
| 10,000 | 32 settlements | 473.2 | 39.6 | 131.7 |
| 20,000 | Shared store | 240.7 | 63.4 | 210.9 |
| 20,000 | 32 settlements | 235.8 | 63.5 | 198.8 |
| 100,000 | Shared store | 41.4 | 169.3 | 815.4 |

Against `7a71e15`, ordinary shared-store throughput rises from 225.6 to 512.3 minutes/s at 10,000 people, 103.3 to 240.7 at 20,000, and 17.5 to 41.4 at 100,000: about 2.3–2.4×. Active RSS rises from 29.2 to 39.6 MiB and 47.2 to 63.4 MiB at the first two sizes. At 100,000 it stays near 168–169 MiB. Requested heap is higher in the ordinary fixture: the fast path retains row capacity, indexes, and reusable bulk undo buffers. These are deliberate speed/memory tradeoffs.

**Richer composed worlds:** The same complete simulation, with 64 extra local patches per area, runs at 323.5 versus 15.8 minutes/s for 10,000 people, while active RSS falls from 87.2 to 39.3 MiB. At 1,000 people with those patches and 1,000 historical facts per added person, the values are 3,107.3 versus 139.4 minutes/s, with 27.3 versus 25.2 MiB RSS. Thus the scene representation gives roughly 20–22× advancement gains in these richer fixtures. All cases retain real food consumption, actions, permissions, births/deaths, shipments, and continued save/load checks. Histories are shared templates; they do not measure unique histories. The opening's first 15 people keep their authored knowledge.

**Changes:** Bodies, minds, knowledge headers, positions, actions, and reverse permissions use indexed dense rows with nested undo. Cold records keep persistent snapshots. Immutable visible scenes share payloads and key layouts; an unchanged observation updates one personal evidence stamp. Reports, individual inspection, stale stock, visibility, per-patch hooks, transaction boundaries, and stable-ID ordering retain their tested behavior. A writable transaction owns one resource map rather than maintaining a second lookup layer for staged resources.

**Storage policies:** At 100,000 people, the default measures 41.4 minutes/s and 169.3 MiB RSS; per-row hashing measures 24.1 and 214.1 MiB, and compact storage 39.1 and 170.5 MiB. Compact storage lowers requested live heap from 140.9 to 124.9 MiB, but allocator retention prevents a reliable RSS reduction here. It also increases initialization time. The default therefore favors buffer reuse. Extremely sparse IDs favor the hash alternative; see the [index comparison](ARCHITECTURE.md#measured-policy-tradeoffs). Storage policies do not change saves or gameplay.

**Retained snapshots:** Holding six actual feature-resource revisions at 100,000 people raises default active RSS from 169.3 to 227.5 MiB. Throughput remains 40.9 versus 41.4 minutes/s without readers. Creating the shared revision takes about 0.003 ms; the first writer pays the copy cost. Maximum current advancement boundaries are 8.2 ms without readers and 9.0 ms with them. Readers are captured immediately after initialization; mature heap-heavy rows and frequent new whole-world snapshots remain unmeasured. Borrowed reads and public observation DTOs do not retain whole revisions.

**Checks:** The ordinary matrix includes 96 default runs across 1/2/4/8 workers, plus reference/hash/compact one-worker comparisons. Further runs cover 100,000 people, richer worlds, retained readers, actual structural effects, and continued execution with different advancement budgets. Every corresponding save hash matches exactly. The largest default advancement boundary through 20,000 people was 4.8 ms. Twelve session runs at 10,000/20,000 people and one/four workers, with 100 samples per operation, kept pause p99 below 1.1 ms and ordinary look-command p99 below 0.22 ms. Maximum pause latency was 3.4 ms. Session latency at 100,000 people remains unmeasured.

Shared-store save/load medians are 41/112 ms at 10,000 people, 85/235 ms at 20,000, and 499/1,241 ms at 100,000. Expanded logical JSON remains expensive for rich histories; the 1,000-person rich checkpoint is about 128 MB and loads in roughly one second. Scene sharing improves runtime representation rather than compressing that wire format. Loading binds fresh action payloads in place instead of cloning their whole resource.

**Allocation accounting:** Instrumented 20,000-person advancement falls from 3.07 million allocations to 1.02 million; requested live heap rises from 25.2 to 31.0 MiB. Compact storage uses 24.8 MiB. At 100,000, allocations fall from 15.62 to 5.09 million and default live heap rises from 125.5 to 140.9 MiB. In the 1,000-person rich fixture, allocations fall from 349,457 to 62,262 and live heap falls from 8.0 to 4.4 MiB. Instrumentation adds atomic overhead; throughput above uses uninstrumented builds.

### Headroom for richer records

Earlier three-run medians at `f7c4cb5` against commit `9fc5f07`, with the same fixture and integer rules:

| Workload | Before ms | After ms |
| --- | ---: | ---: |
| 10,000 local observations with 1,000 historical accounts, 1,000 maps, 10,000 world patches | 2,840.3 | 79.9 |
| 10,000 provisioning queries with those histories | 465.7 | 17.0 |
| 10,000 known-road routes with those histories | 4,127.8 | 6.5 |
| 10,000 effective-weight reads from fully populated 1,000-trait profiles | 2,128.3 | 0.41 |

Routing looks up reached records instead of reconstructing the whole known graph. Decision preparation seeks relevant fact namespaces. Repeated trait reads use compiled numeric effects. These gains remove work proportional to unrelated history and definition count. The reference comparison also checked complete logical perception state; physical JSON field order is not gameplay state.

### CPU and GPU layouts

The [numeric decision experiment](../compute/README.md) compares packed records, tiled columns, runtime AVX2, and actual Intel UHD 620 Vulkan compute. Every candidate uses the production selection rule, including saturation, urgency, stable-ID ties, and hysteresis. The GPU receives no private world knowledge. Packed production candidates occupy 56 bytes; experimental columns use 49 bytes per candidate before policy/output buffers.

See the compute tool for measured arithmetic, packing, GPU transfers, resident dispatches, and a component-level Bevy comparison. Packed numeric inputs are in production. SIMD columns and GPU execution remain experiments: their conversion costs must be included before changing the simulation backend.

## Fundamental architecture evaluation

The [architecture experiments](ARCHITECTURE.md) compare persistent hot tables with packed columns and row-level undo, exact analytical physiology, and compact typed pending messages. They retain stable IDs and test rollback, structural changes, retained readers, and continued execution after a checkpoint. Typed requests, dense hot components with journals, and shared observation scenes now run in production. Earlier kernel ratios are historical comparisons. Analytical intervals and parallel decision preparation remain experiments.

**Limits:** Whole-game worker scaling remains weak in the ordinary survival fixture; faster single-worker storage is the principal gain. Shared-store default medians at 10,000 people are 512.3, 493.1, 482.8, 472.8 minutes/s at 1/2/4/8 workers. Full late-game production, tactical load, unique long histories, campaign catch-up, mature full-world snapshots, and mass-death latency remain unmeasured. These fixtures establish their asserted behavior and budgets, not the complete game design.

## Raw-world diagnostic replay

`cargo run -p game-performance --profile simulation --example resume_world -- SAVE [DAYS] [OUTPUT]` resumes a raw world checkpoint with bounded advancement and rejects sustained failure of clock progress. Set `LEGACY_PROFILE=1` for feature spans. Run timing measurements without concurrent builds or other simulations, using a frozen executable and the same input. This tool does not reconstruct ancestry or history ledgers; use the history driver for multigeneration evidence.

## Sampling complete histories

On Linux with GNU gprofng installed, `gprofng collect app -p on -a off -o target/history/profile.er target/simulation/game-history --resume CHECKPOINT --days END_DAY --output target/history/profile-run` samples call stacks without requiring kernel performance counters. Read inclusive call paths with `gprofng display text -sort i.%totalcpu -functions target/history/profile.er`. Keep profiles under `target/`. Use samples to identify costs; use uninstrumented alternating replay pairs to establish time and peak RSS changes.

For a runtime specific to the build machine, add `--target-cpu native`. `CARGO_PROFILE_RELEASE_LTO=fat` selects whole-program optimization for both compilation phases. Metadata records the CPU target and release-profile overrides. Compare the resulting executable on workloads beyond the training windows, and keep the ordinary portable build for other machines.

## Repeated language interpretation

`cargo bench -p game-performance --bench language` compares the actual decoder
with the prior complete scan on identical personal vocabularies. Each optimized
batch starts with a fresh vocabulary version, including reverse-index preparation
in its timing. Three alternating pairs verify interpretation, confidence,
rendering, checksums, and unchanged saved vocabulary. `LEGACY_WORDS` and
`LEGACY_DECODES` select scale. This measures a language query workload, not a
whole-world speedup.

## World memory phases

`cargo run -p game-performance --profile simulation --features allocation-profile
--example memory_world -- CHECKPOINT.json 1` reports requested live/peak heap,
allocation counts, and process RSS after loading, advancing, streaming a save to a
sink, and releasing a raw world. Omit the final day count to isolate loading and
saving. Instrumentation changes timing; use frozen, uninstrumented history replays
for speed comparisons. This probe does not reconstruct the history ledger.

With `LEGACY_PROFILE=1`, the allocation-profile memory probe also attributes
inclusive allocation counts to the existing feature spans. Nested spans overlap;
do not add their counts. The default simulation build includes no allocation
counter. Use this instrumentation to locate churn, then verify optimizers with
uninstrumented matching replays.

## Sampled allocation stacks

Build `memory_world` with `--features allocation-stacks` and set
`LEGACY_ALLOCATION_STACKS=target/history/allocation-stacks.json`. It samples roughly
one in 8,192 advancement allocations, capped at 8,192 stacks. Sampling is guarded
against recursive allocator calls. Stack grouping/symbol resolution happens after
advancement. The feature is absent from normal game builds. Use samples only to
attribute churn: unwinding and symbol resolution add allocations and retain debug
data, so its heap/RSS/timing numbers are not benchmark evidence.
