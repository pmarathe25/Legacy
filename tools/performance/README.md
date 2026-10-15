# Performance workloads

Run release benchmarks from the repository root. Each process checks its resulting records; timings alone do not constitute a pass.

```sh
python3 tools/performance/run.py
cargo bench -p game-performance --bench ledger --locked --offline
cargo bench -p game-performance --bench storage --locked --offline
cargo bench -p game-performance --bench perception --locked --offline
cargo bench -p game-performance --bench traits --locked --offline
cargo bench -p game-performance --bench architecture --locked --offline
LEGACY_POPULATION=10000 LEGACY_WORKERS=1 cargo bench -p game-performance --bench economy --locked --offline
LEGACY_POPULATION=10000 LEGACY_WORKERS=4 LEGACY_DISTRIBUTED=true cargo bench -p game-performance --bench economy --locked --offline
LEGACY_POPULATION=10000 cargo bench -p game-performance --bench latency --locked --offline
cargo bench -p game-performance --features allocation-profile --bench economy --locked --offline
```

Use `LEGACY_POPULATION` values 15, 1000, 10000, and 20000, and worker counts 1, 2, 4, and the available bounded budget. Repeat each economy and latency run at least three times. `LEGACY_MINUTES` defaults to 120. The storage bench runs this population/worker matrix with three repeats and distributed/contested stock automatically.

The CLI and performance fixtures default to bundled [mimalloc](https://github.com/purpleprotocol/mimalloc_rust); building them needs a C compiler. Feature libraries select no global allocator. Use `run.py --system-allocator` or Cargo's `--no-default-features` for a controlled system-allocator comparison. Metadata identifies the allocator. Optional allocation instrumentation wraps the selected allocator, preserving that choice.

**Storage:** The shared protocol fixture includes needs, progress, scoring from four known choices, two sparse relationship edges per person, stable transfer arbitration, arrivals, membership changes, structural replacement, observation extraction, and a checkpoint followed by continued execution. It compares canonical records, conserved quantities, and event hashes across workers and reversed evaluation output. This is a storage workload; its progress and transfers are fixture rules. It does not run the complete economy.

**Economy:** The composed fixture runs actual character, knowledge, physiology, cargo, and transport features with a shared warehouse or 32 settlements. It checks live bodies and actual food consumption, saves and reloads, then compares continued canonical state with different advancement budgets. A separate measured phase creates actual births and deaths, reserves capacity, and starts deliveries. The checkpoint retains those in-flight shipments and checks their eventual arrival and quantity conservation. Output includes fixture counts, throughput, committed-boundary latency, save/load time and size, and a state hash. Pending choices and invocation semantics remain covered by feature and assembly tests.

**Ledger:** Actual cargo requests fill disjoint inventories or one shared reservation book. The benchmark rejects an over-capacity request without consuming its identity, reloads pending reservations, then releases half and fulfills half. It checks final quantities, capacity accounting, and canonical state at 15, 1,000, 10,000, and 20,000 reservations.

**Latency:** A real `Session` receives 100 pauses and public look commands during catch-up in the shared-store fixture. It reports acknowledgement p50/p95/p99 and maximum latency. The session services input after each committed request. A single request still determines the longest possible wait.

**Perception:** The runner uses 1,000 people with either no historical facts, or 1,000 shared historical accounts and 1,000 obsolete maps per person, plus one or 10,000 world patches. One patch is local. Ten rounds inspect surroundings, generate actual provisioning options, and route along a known road. It checks exact choices, quantities, fact counts, and canonical hashes across workers. The development comparison also checked complete logical state against a temporary frozen checkout. Shared cold histories test bounded updates and namespace indexes; they do not establish memory use for unique personal histories.

**Traits:** The actual trait owner installs 1,000 definitions and 1,000 profiles with every attribute populated. Ten rounds read the effective weights, compare them with an independent integer reference, then verify rebuilding after save/load. Profile construction is outside the timed read phase.

Every benchmark prints hardware, OS, compiler, build mode, and worker metadata. Linux process RSS includes allocator overhead and thread stacks. Optional allocation instrumentation reports allocation counts, requested live heap bytes, and peak heap bytes, including working buffers and observations. It adds atomic accounting overhead; compare throughput with the default build. Save/load temporarily retains a second simulation and serialized snapshots.

## Storage choice

The production implementation is [game-storage](../../crates/game-storage/README.md): ordered dense leaves for hot numeric rows and individually shared sparse records. It integrates with owner-checked transactions, incremental validation, and nested rollback through immutable snapshots. Content catalogs remain definitions, independent of component types.

Standalone Bevy ECS 0.20.0 and Legion 0.4.0 were compared with the same packed protocol, pure physiology rules, Rayon worker budget, stable IDs, operation order, and canonical output. Both ECS adapters used cached queries, batch creation, and a stable-ID/entity map. Each run retained the same canonical reference output. The host retained phase scheduling; this comparison does not rank their native schedulers. Bevy committed the packed workload faster. Dense storage retained less memory and integrates atomic snapshots without an additional record journal or ECS ownership layer. This integration and memory tradeoff determined the choice. Losing adapters and dependencies are absent from the workspace. APIs were checked against the [Bevy ECS documentation](https://docs.rs/bevy_ecs/0.20.0/bevy_ecs/) and [Legion documentation](https://docs.rs/legion/0.4.0/legion/).

On the measured laptop (i7-8550U, 4 cores/8 threads, 7.15 GiB RAM, Linux x86_64, Rust 1.98.1, release optimization), the three-run medians for 10,000 people, four workers, distributed stock, and 60 protocol steps were:

| Candidate | Total ms | Retained heap MiB | Peak heap MiB |
| --- | ---: | ---: | ---: |
| Dense | 198.0 | 5.30 | 12.64 |
| Bevy ECS | 144.3 | 6.07 | 14.45 |
| Legion | 155.8 | 5.83 | 13.65 |

These comparison runs instrumented allocations equally; their elapsed times include that overhead. The protocol retains cold records and a canonical reference, and includes packing, evaluation, stable arbitration, commit, structural changes, and a checkpoint. One-worker dense time was 361.5 ms, so four workers improved this parallel workload by 1.83×. Dense trades some packed-loop speed for a smaller footprint and direct immutable transaction snapshots. The production physiology path updates staged leaves directly and avoids the protocol's packing buffers.

## Production measurements

Measured on the i7-8550U laptop, 4 cores/8 threads, 7.15 GiB RAM, Linux, Rust 1.98.1. Release builds use optimization level 3, thin LTO, one codegen unit, portable CPU defaults, and bundled mimalloc. This machine is below the provisional six-core/16-GiB reference. The composed survival workload runs 120 minutes; values are three-run medians with one worker.

| People | Layout | Simulated minutes/s | Active RSS MiB | Peak RSS through save/load MiB |
| --- | --- | ---: | ---: | ---: |
| 1,000 | Shared store | 2,570.6 | 16.9 | 43.0 |
| 1,000 | 32 settlements | 2,439.3 | 16.9 | 35.1 |
| 10,000 | Shared store | 201.3 | 44.9 | 144.9 |
| 10,000 | 32 settlements | 196.9 | 44.9 | 143.7 |
| 20,000 | Shared store | 93.1 | 72.4 | 261.3 |
| 20,000 | 32 settlements | 90.9 | 72.4 | 268.1 |
| 100,000 | Shared store | 15.7 | 301.3 | 1,114.3 |

Commit `9d2d042` measured 137.1 minutes/s at 10,000 people and 62.2 at 20,000. This pass improves ordinary survival throughput by roughly 50%. A frozen executable also checked the 100,000-person case: 11.1 → 15.7 minutes/s, with identical saved state and continued execution. The 100,000-person run is a stress case, not the 10,000-person design target.

Empty persistent tables allocate nothing; aligned validation pairs avoid repeated tree searches. Observation refreshes update provenance without replacing unchanged payloads. Completed container discovery uses weak permission-version tokens. Provisioning reads locations once per query and reuses routes to a destination. Typed internal hooks and signals avoid JSON conversion and temporary subscriber lists. Content, observation cadence, stale knowledge, transaction boundaries, and arbitration order retain their tested behavior.

A same-source allocator comparison measured approximately 153/69 minutes/s with the system allocator and 200/94 with mimalloc at 10,000/20,000 people. Thus the allocator accounts for much of the improvement. A separate dynamic-library probe found TCMalloc faster than mimalloc on this machine, with greater retained and peak RSS. Mimalloc is the bundled portable default; the system allocator remains a build option. At 20,000 people the default raises active RSS by about 3 MiB and save/load peak by about 45 MiB compared with the preceding commit. At 100,000, active RSS falls from 307.5 to 301.3 MiB. These are workload-dependent allocator tradeoffs.

The matrix checked 96 economy runs at 1, 2, 4, and 8 workers, exact meals and live bodies, actual births/deaths, in-flight shipments and reservations, and continued canonical state after loading. All corresponding hashes match the preceding executable. The largest committed boundary through 20,000 people was 5.5 ms. Across measured workers and repeats, pause p99 stayed below 3.1 ms and look-command p99 below 0.43 ms at 10,000 and 20,000. These were 100 queued samples per run, not every possible command. Maximum individual pause latency was 3.8 ms. At 100,000 people the largest advancement boundary was 14.1 ms; session acknowledgement latency was not measured there.

Shared-store save/load medians were 50/137 ms at 10,000 people, 110/261 ms at 20,000, and 881/1,473 ms at 100,000. Peak RSS across the ordinary population matrix stayed below 282 MiB. Save/load temporarily retains both simulations and serialized snapshots.

Optional allocation instrumentation preserved all six corresponding economy hashes. At 10,000 people, advancement made 1.77 million allocations, down from 1.90 million, with 25.8 MiB requested live heap and 26.5 MiB peak. At 20,000 people those values were 51.4/52.8 MiB. Save/load/continued execution peaked at 104.9/210.3 MiB requested heap at 10,000/20,000. Instrumentation adds atomic accounting overhead; throughput above comes from uninstrumented builds.

### Headroom for richer records

Three-run medians against commit `9fc5f07`, with the same fixture and integer rules:

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

The [architecture experiments](ARCHITECTURE.md) compare persistent hot tables with packed columns and row-level undo, exact analytical physiology, and compact typed pending messages. They retain stable IDs and test rollback, structural changes, retained readers, and continued execution after a checkpoint. These isolated kernels establish promising directions; they are not whole-game speedups or a second production engine.

**Limits:** Extra workers still produce little whole-survival scaling. Shared-store medians at 10,000 people were 201.3, 200.5, 200.0, and 195.6 minutes/s at 1, 2, 4, and 8 workers; at 20,000 they were 93.1, 91.5, 90.7, and 91.4. Decisions, observations, and receiver commits remain mostly serial. Full late-game production, tactical load, unique long histories, campaign catch-up, and mass-death latency remain unmeasured. The configured memory and ordinary-input budgets are met for these fixtures, not established for the complete design.
