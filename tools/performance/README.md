# Performance workloads

Run release benchmarks from the repository root. Each process checks its resulting records; timings alone do not constitute a pass.

```sh
python3 tools/performance/run.py
cargo bench -p game-performance --bench ledger --locked --offline
cargo bench -p game-performance --bench storage --locked --offline
cargo bench -p game-performance --bench perception --locked --offline
cargo bench -p game-performance --bench traits --locked --offline
LEGACY_POPULATION=10000 LEGACY_WORKERS=1 cargo bench -p game-performance --bench economy --locked --offline
LEGACY_POPULATION=10000 LEGACY_WORKERS=4 LEGACY_DISTRIBUTED=true cargo bench -p game-performance --bench economy --locked --offline
LEGACY_POPULATION=10000 cargo bench -p game-performance --bench latency --locked --offline
cargo bench -p game-performance --features allocation-profile --bench economy --locked --offline
```

Use `LEGACY_POPULATION` values 15, 1000, 10000, and 20000, and worker counts 1, 2, 4, and the available bounded budget. Repeat each economy and latency run at least three times. `LEGACY_MINUTES` defaults to 120. The storage bench runs this population/worker matrix with three repeats and distributed/contested stock automatically.

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

Measured on the same i7-8550U laptop, 4 cores/8 threads, 7.15 GiB RAM, Linux, Rust 1.98.1. Release builds use optimization level 3, thin LTO, and one codegen unit, with portable CPU defaults. This machine is below the provisional six-core/16-GiB reference. The composed survival workload runs 120 minutes; values are three-run medians with one worker.

| People | Layout | Simulated minutes/s | Active RSS MiB | Peak RSS through save/load MiB |
| --- | --- | ---: | ---: | ---: |
| 1,000 | Shared store | 1,995.7 | 12.5 | 19.9 |
| 1,000 | 32 settlements | 1,694.1 | 12.5 | 19.6 |
| 10,000 | Shared store | 137.1 | 39.9 | 114.4 |
| 10,000 | 32 settlements | 128.8 | 39.0 | 111.7 |
| 20,000 | Shared store | 62.2 | 69.2 | 219.8 |
| 20,000 | 32 settlements | 58.2 | 68.4 | 214.3 |

The preceding committed implementation measured 61.0 minutes/s and 76.6 MiB active RSS at 10,000 people, and 28.1 minutes/s and 143.3 MiB at 20,000. The optimization pass includes compiler settings as well as representation, indexing, and scheduling changes. It more than doubles ordinary survival throughput and roughly halves active memory in this fixture.

Recorded facts use typed immutable payloads rather than repeated JSON conversion. Personal histories, relationships, and skills use bounded persistent updates. Trait profiles compile into five numeric weights. Object IDs, local patches, standing access, carried containers, and reservations have owner-maintained indexes. Active minds use a compact ID set; waiting minds wake on action completion or a physiological threshold. Nested attempts share immutable resource checkpoints, and adjacent wake completions share bounded commits.

The final matrix checked 96 economy runs at 1, 2, 4, and 8 workers, exact meals and live bodies, actual births/deaths, in-flight shipments and reservations, and continued canonical state after loading. The largest committed boundary was 4.8 ms. Across measured workers and repeats, pause p99 stayed below 3.0 ms at 20,000 people; public look-command p99 stayed below 0.36 ms at 10,000 and 20,000. These were 100 queued samples per run, not every possible command. Maximum individual pause latency was 3.9 ms.

Shared-store save/load medians were 74/138 ms at 10,000 people and 156/281 ms at 20,000. Peak RSS across normal runs stayed below 225 MiB. Load reconstructs shared payloads and derived indexes; it trades some CPU work for a smaller retained footprint. Save/load temporarily holds both simulations and snapshots.

Optional allocation instrumentation preserved all 12 corresponding economy hashes. At 10,000 people, advancement made 1.90 million allocations, down from 7.69 million in the preceding pass, with 26.1 MiB requested live heap and 26.8 MiB peak. At 20,000 people those values were 52.0/53.4 MiB. Save/load/continued execution peaked at 105.5/211.6 MiB requested heap at 10,000/20,000 people. This instrumentation adds atomic accounting overhead; throughput above comes from uninstrumented builds. Reconstructing shared typed payloads makes more small allocations during loading, despite the smaller live footprint.

### Headroom for richer records

Three-run medians against commit `9fc5f07`, with the same fixture and integer rules:

| Workload | Before ms | After ms |
| --- | ---: | ---: |
| 10,000 local observations with 1,000 historical accounts, 1,000 maps, 10,000 world patches | 2,840.3 | 97.1 |
| 10,000 provisioning queries with those histories | 465.7 | 26.7 |
| 10,000 known-road routes with those histories | 4,127.8 | 8.8 |
| 10,000 effective-weight reads from fully populated 1,000-trait profiles | 2,128.3 | 0.57 |

Routing looks up reached records instead of reconstructing the whole known graph. Decision preparation seeks relevant fact namespaces. Repeated trait reads use compiled numeric effects. These gains remove work proportional to unrelated history and definition count. The reference comparison also checked complete logical perception state; physical JSON field order is not gameplay state.

### CPU and GPU layouts

The [numeric decision experiment](../compute/README.md) compares packed records, tiled columns, runtime AVX2, and actual Intel UHD 620 Vulkan compute. Every candidate uses the production selection rule, including saturation, urgency, stable-ID ties, and hysteresis. The GPU receives no private world knowledge. Packed production candidates occupy 56 bytes; experimental columns use 49 bytes per candidate before policy/output buffers.

See the compute tool for measured arithmetic, packing, GPU transfers, resident dispatches, and a component-level Bevy comparison. Packed numeric inputs are in production. SIMD columns and GPU execution remain experiments: their conversion costs must be included before changing the simulation backend.

**Limits:** Extra workers still produce little whole-survival scaling. Shared-store medians at 10,000 people were 137.1, 138.2, 137.4, and 136.7 minutes/s at 1, 2, 4, and 8 workers; at 20,000 they were 62.2, 62.6, 64.3, and 63.7. Decisions, observations, and receiver commits remain mostly serial. The distributed 20,000-person case still sits near the 60-minute target. Full late-game production, tactical load, unique long histories, campaign catch-up, and mass-death latency remain unmeasured. The configured memory and ordinary-input budgets are met for these fixtures, not established for the complete design.
