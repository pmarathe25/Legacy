# Performance workloads

Run release benchmarks from the repository root. Each process checks its resulting records; timings alone do not constitute a pass.

```sh
python3 tools/performance/run.py
cargo bench -p game-performance --bench ledger --locked --offline
cargo bench -p game-performance --bench storage --locked --offline
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

Three-run medians on the same laptop, one worker, 120 simulated minutes:

| People | Layout | Simulated minutes/s | RSS after advance MiB | Peak RSS through save/load MiB |
| --- | --- | ---: | ---: | ---: |
| 1,000 | Shared store | 858.3 | 17.7 | 27.7 |
| 10,000 | Shared store | 61.0 | 76.6 | 181.8 |
| 10,000 | 32 settlements | 59.9 | 77.6 | 181.8 |
| 20,000 | Shared store | 28.1 | 143.3 | 353.5 |
| 20,000 | 32 settlements | 26.9 | 142.4 | 353.1 |

The pre-change 1,000-person shared-store run took 12.306 seconds for the same survival interval; its longest committed boundary took 4.856 seconds. The retained implementation's three-run median was 0.140 seconds. All final economy runs checked exact meal consumption, live bodies, pending shipments and reservations, and continued canonical state after loading. The matrix checked 96 economy runs at 1, 2, 4, and 8 workers. The largest committed boundary in that matrix was 6.1 ms.

Across all measured worker counts and repeats, pause p95/p99 remained below 3.6/4.4 ms at 20,000 people; public look-command p95/p99 remained below 0.25/0.37 ms. These were 100 queued samples per run. Save/load medians were 86/122 ms at 10,000 people and 186/247 ms at 20,000 people in the shared-store fixture. Peak RSS includes both simulations, serialized snapshots, validation indexes, and working buffers. The configured 2 GiB memory budget and ordinary-input latency budgets were met for these fixtures.

The cargo ledger filled 20,000 reservations in one store in 34.3 ms and released or fulfilled them in 31.8 ms with one worker. Reservation-command p99 stayed below 0.005 ms in that configuration. Disjoint stores took 110.7/109.7 ms; neither path copied or scanned an entire population or reservation book per request.

Allocation instrumentation preserved every economy state hash. At 10,000 people, advancement made 7.69 million allocations, with 55.6 MiB requested live heap and 56.3 MiB peak heap. Save/load peaked at 162.4 MiB requested heap; its two retained simulations and snapshots account for the larger footprint. At 20,000 people, advance peak heap was 112.6 MiB and save/load peak heap was 325.8 MiB in the shared-store fixture.

**Limits:** The 10,000-person survival workload sits close to the provisional 60-minute throughput threshold, with little headroom. Extra workers produced little end-to-end improvement there: shared-store medians were 61.0, 61.9, 60.2, and 61.9 minutes/s at 1, 2, 4, and 8 workers. Most decision and arbitration code still runs serially; reviews retain their authored frequency and scan registered minds in bounded pages. Sparse wakeups and lower transient allocation need further work. The parallel packed protocol scaled, but that does not establish scaling for the full game. Autonomous enterprises, a representative late-game economy, growing historical knowledge, campaign catch-up, and tactical workloads remain unmeasured. See [the performance design](../../docs/design/performance.md) for provisional budgets and requirements beyond these implemented fixtures.
