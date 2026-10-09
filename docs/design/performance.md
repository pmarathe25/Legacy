# Performance, concurrency, and compute backends

**Required target:** Approximately 10,000 individually simulated people in the late game on a typical desktop. CPU concurrency is part of the initial architecture. GPU compute is an optional later optimization; the game must remain fully functional without it.

These are design targets, not measured performance claims. Code, correctness tests, and runnable benchmarks establish actual behavior and speed. Benchmark output supplies measurements directly; no separate performance report or evidence bundle is required.

## Measure the whole simulation

Population alone does not define workload. Fixtures also specify areas, active tasks, physical lots, shipments, relationships, institutions, agreements, conflicts, historical records, and the proportion of people reconsidering plans. Print those counts with benchmark timings.

Use a provisional reference desktop with six physical CPU cores, 16 GiB RAM, and no required discrete GPU. Include actual CPU, memory, OS, compiler, build settings, and worker count in benchmark output, detecting them or accepting run metadata. This is a benchmark profile to refine against real hardware, not an announced minimum specification.

Initial engineering budgets, subject to measurement and the unresolved game pacing:

| Measure | Provisional target |
| --- | --- |
| Supported active population | 10,000, preserving individual identity and behavior. |
| Ordinary command or pause acknowledgement | p95 within 50 ms and p99 within 100 ms under the representative late-game load. |
| Normal settlement throughput | At least 60 simulated minutes per wall-clock second in the representative 10,000-person economic fixture. |
| Catch-up after a burst | No continuously growing simulation backlog at the chosen normal play rate. |
| Memory | Start with a 2 GiB budget for the headless process in the representative late-game fixture, including indexes, working buffers, snapshots, and loaded history. |
| Worker scaling | Demonstrate a material end-to-end improvement on parallel workloads; record scaling at 1, 2, 4, and available workers rather than promising linear speedup. |

The throughput budget is an initial test target, not a decision about how fast a life passes for the player. Report accelerated childhood and long-history performance separately; sustaining ordinary play does not prove that decades can be advanced comfortably. Turn-based combat receives map-generation, action-response, round-resolution, and memory budgets once representative tactical rules and encounter sizes exist. Measure concurrent encounters and their interaction with economic work.

Measure release builds with fixed fixtures and repeated runs. Record median and tail latency, CPU time by phase, allocations, peak memory, time blocked on coordination, output sizes, and resulting state hashes. Compare against a correct one-worker baseline, not a deliberately slow implementation.

## Evaluate storage and scheduling before choosing a framework

Compare three implementations of a small common workload:

| Candidate | What to evaluate |
| --- | --- |
| Standalone Bevy ECS | Component storage, queries, dependency scheduling, access to parallel batches, and integration cost without the graphics engine. |
| Legion | The same workload using its storage, queries, parallel schedule, and transaction integration. |
| Custom game-specific system | Typed dense tables, sparse relation stores, explicit phase dependencies, and partitioned jobs for this game's known data. |

Bevy ECS supports standalone use and scheduling from declared data access. Legion also provides queries and a parallel system schedule. These establish them as candidates, not evidence that either is fastest for this game. See the primary [Bevy ECS documentation](https://docs.rs/bevy_ecs/latest/bevy_ecs/) and [Legion documentation](https://docs.rs/legion/latest/legion/).

Use identical rules, inputs, worker budgets, observation output, and correctness checks. Include needs updates, candidate scoring, sparse relationship lookups, contested inventories, shipments between settlements, births/deaths, and extracting client views. Measure conversion and scheduling costs as well as numerical loops.

Also compare implementation complexity, diagnostics, dependency and maintenance status at selection time, save/load integration, and ease of changing gameplay rules. Build the [first executable slice](implementation.md#first-coding-task), compare small candidate adapters on its workload, and keep one production implementation afterward. Keep useful tests and benchmarks; remove losing experiments. Do not build three complete simulation engines, a universal storage adapter, or a separate selection report.

A custom system is a first-class option. Its scope is the domain's tables, indexes, job dependencies, and commit rules; it need not recreate a general-purpose ECS, plugin framework, allocator, or thread pool. Established libraries can supply worker execution even when storage and scheduling policy are tailored to the game.

The original standard-library-only prototype constraint is not a permanent restriction on this full-game design. Evaluate small infrastructure dependencies on their benefits and costs. ECS or compute choices must not impose a renderer dependency.

## Data layout and access

Keep frequently processed numeric state compact: physiology, needs, skill values, action progress, location indices, and deadlines. Store names, descriptions, detailed memories, agreement text, and historical records separately. A needs update should not fetch an entire character's social history.

Use columnar arrays or suitable ECS component storage for bulk work, with stable typed game IDs mapped to internal row or entity handles. Internal handles and row positions can change; persistent IDs cannot. Neither save files nor client commands expose storage-library entity handles.

Relationships, knowledge, claims, memberships, and obligations are sparse graphs or indexed records. Store actual edges and default attitudes rather than materializing every pair of people. Ten thousand people must not require every citizen to scan all other citizens or all offers on each decision.

Index opportunities by relevant area, skill, resource, organization, and known information. Maintain per-domain change versions and dirty queues. Invalidate a derived result when its dependencies change, rather than rebuilding every index each tick. Any search budget that changes which opportunities a person considers is a game rule, not a hardware-dependent optimization.

Avoid one enormous mutable world resource requested by every system: that would prevent useful parallel work even inside an ECS. Structural changes, such as creation, deletion, or component membership changes, occur at declared boundaries. Do not toggle structural composition for every small numeric or action-state change.

Reuse batch and output buffers. Prefer contiguous traversal over per-character dynamic dispatch and allocation in frequent loops. Build read-only phase views from borrowed columns or versioned chunks; do not deep-copy the entire world every phase.

## Execution model

Use a bounded worker pool. Jobs process useful batches; there is no OS thread or asynchronous task per citizen. Coordinate CPU pools so ECS scheduling, numerical kernels, background saves, and a future renderer do not each assume all cores are available.

A phase is a set of operations whose prerequisites are complete. Its dependency graph declares what each operation reads, produces, or exclusively writes. Ready jobs can run concurrently; dependent jobs wait for a defined boundary.

The main pattern is:

```text
consistent read view
→ parallel evaluation into owned result buffers
→ deterministic resolution of competing intentions
→ commit disjoint changes, coordinating cross-owner transactions
→ publish the next revision and meaningful events
```

For independent per-record state, workers can write disjoint next-state ranges directly. Where actors interact, workers emit intentions rather than changing shared balances or other actors during evaluation. A common snapshot is a consistency guarantee, not necessarily an extra copy of every record.

| Work | Parallel approach | Required coordination |
| --- | --- | --- |
| Need rates and passive progress | Disjoint character/site batches. | Materialize before dependent decisions; keep threshold semantics. |
| Candidate generation and utility | Batches reading known opportunities and local state. | Reserve contested tasks, stock, or equipment afterward. |
| Route queries | Independent queries with immutable graph versions. | Apply route/access changes at declared boundaries. |
| Production | Disjoint sites and reserved inputs. | Aggregate shared storage and obligations exactly once. |
| Institutions and agreements | Due records or groups with disjoint dependencies. | Coordinate overlapping mandates, parties, and assets. |
| Transfers and conflict | Independent conflict groups or ledger partitions. | Resolve overlapping resource and participant sets canonically. |
| Observation and save preparation | Read immutable committed revisions. | Never expose a partly committed world. |

Start with coarse enough jobs to amortize scheduling. For small batches, execute inline through the same rules. Tune worker count and batch size without changing logical outcomes.

## Ownership and cross-partition transactions

Give each mutable record one write owner during a commit phase. Partition frequently local work by stable storage chunks or locality; a settlement is useful locality, not a permanently isolated simulation.

Group operations by the records they can change. Disjoint groups can resolve and commit concurrently. Operations sharing a balance, participant, claim, or exclusive tool must share arbitration. A contested central warehouse remains a real coordination point and must appear in benchmarks.

Cross-partition transfers use one transaction record that validates all inputs, reserves them, and commits all applicable changes at the same logical boundary. Sort dependencies by stable keys and resolve conflicts in a defined order. A shipment changing locality transfers record ownership once; it cannot be processed by both partitions or disappear between them.

A simple ordered commit is acceptable for the first reference path. The architecture must allow independent groups to commit concurrently, and benchmarks must expose the fraction still serialized. If commit dominates, improve ownership and grouping rather than only parallelizing more scoring work.

Do not use races for resource allocation or hold a global world lock while doing expensive decisions. Worker failure must leave the last committed revision intact; incomplete result buffers are not partially applied state.

## Determinism across worker counts

Thread completion order, ECS iteration order, physical chunk layout, and GPU execution order are not game rules. Define a logical operation key from simulation time, phase, actor or source ID, and an operation-local ordinal.

Resolve intentions and sequence externally visible events by stable logical keys, with the simulation's specified fairness rule for equivalent competing claims. Merge already ordered local buffers where useful; avoid a needless global sort of every unchanged record.

Workers emit creation requests. Allocate stable IDs after ordering accepted requests, using deterministic ranges or prefix counts, rather than letting racing atomic increments decide which child, item, or proposal receives which ID. Invalid commands and rejected creation attempts do not consume IDs unless the domain explicitly records an attempted object.

Randomness is keyed to a logical operation: world seed, domain, time or persistent occurrence, stable subject IDs, and draw slot. Persist seeds, algorithm versions, and any logical occurrence counters. Do not use one shared sequential generator, thread-local generators, job IDs, or wall time to choose gameplay outcomes. Worker count and job splitting must not change random draws.

Use integer/fixed-point arithmetic with specified widths, overflow behavior, and rounding for authoritative kernels. Reduction order must be fixed when an operation is not associative; checked sums can use a proven-wide intermediate and one defined range check. Parallel reduction order is not automatically deterministic, particularly with floating-point arithmetic, as the [Rayon documentation](https://docs.rs/rayon/latest/rayon/iter/trait.ParallelIterator.html#method.reduce) explains.

Test identical canonical state and meaningful event streams across worker counts, alternative job partitioning, repeated runs, save/load, and intentionally perturbed job completion. Compare logical records in stable ID order, clock, pending work, allocation/random counters, and event contents. Exclude diagnostic timings, physical storage layout, and presentation-only publication counters from canonical state. Queries and client yield points must not create gameplay events. Supported CPU backends must satisfy this contract before being selectable.

## Do less unnecessary work

Use scheduled reviews, threshold events, and dependency invalidation from the first implementation. Independent systems need not run at combat frequency. Agent interest, a changed offer, an injury, a broken promise, or a newly learned fact can wake only the affected decisions.

Analytically accumulate unchanged rates until the earliest relevant event. Recalculate deadlines when inputs change. Keep fractional remainders and the same result as the reference stepper. Cache validity must include the facts that could change eligibility, utility ordering, access, or an interruption threshold.

This does not justify freezing distant people, reducing intelligence when the CPU is busy, silently dropping scheduled work, or skipping interactions. A performance optimization preserves defined behavior. Any intentional approximation of behavior requires a separate design decision.

Long-term history is retained in appropriate storage with indexes and summaries. Do not keep every routine per-minute trace in active memory forever. Preserve historical facts needed by the mechanics, such as obligations, memories, ancestry, and disputes. Benchmarks include growing history rather than only fresh worlds.

## Optional GPU compute

Keep a narrow compute interface around suitable pure batches. A compute backend receives versioned, packed inputs and produces typed results. The simulation validates and commits them through the same rules as CPU work. GPU code does not own the world, assign identities, decide institutional authority, acknowledge player warnings, or mutate a renderer's scene as simulation state.

Candidate workloads include large batches of utility terms, regular need/progress calculations, and numeric environmental updates. Irregular negotiations, pointer-heavy graph traversal, small batches, and tightly contested transactions stay on the CPU unless measurement gives a reason to change that decision. These are candidates, not claims of a GPU speedup.

Define each eligible kernel's input/output schema, units, integer widths, valid ranges, rounding, overflow and error behavior, and result ordering. Use flat numeric buffers, offsets for variable-length records, explicit enums, and batch-local indices. Keep strings, Rust references, trait objects, ECS handles, and driver-specific types outside the compute contract. Pack only required fields and count the packing cost.

Provide an ordinary CPU implementation of each kernel first. An accelerator is eligible only if it reproduces authoritative results for its supported range; otherwise use the CPU. Unsupported integer capabilities, excessive transfer cost, unavailable devices, or device failure must preserve a working CPU path. Do not make the saved world dependent on a GPU vendor or device allocation.

Submit large enough batches and retain reusable read-only data when beneficial. Measure packing, upload, launch, execution, synchronization, readback, and integration together. Device transfer cost and batching matter to end-to-end performance; [NVIDIA's best-practices guide](https://docs.nvidia.com/cuda/cuda-c-best-practices-guide/index.html#data-transfer-between-host-and-device) provides supporting guidance, without committing this game to CUDA.

Tag results with their input revision and operation identity. A phase cannot commit without all its required results; late or stale outputs are discarded. On failure, rerun the unchanged batch on the CPU and commit once. Backend selection or timeout can affect elapsed wall time, not game outcomes or whether warnings occur.

Defer GPU API selection and kernel implementation until profiling establishes a benefit. Keep compute optional and distinct from rendering: a headless game can use GPU compute, and a graphical client can run a CPU-only simulation. Future compute must share GPU and memory budgets with rendering rather than assuming exclusive device use.

## Workloads and tests

Maintain fixtures at roughly 15, 1,000, and 10,000 people, plus a 20,000-person stress case to expose scaling failures. The stress case is not a promised supported population.

Include ordinary production and social activity, many simultaneous decisions, one highly contested stockpile, cross-settlement deliveries, famine and migration, institutional disputes, births/deaths, reincarnation and recovery, frequent client inspection, and long history. Add mixed combat and economic load when conflict mechanics exist.

From the first milestone:

- Profile serial and parallel execution with the same rules and fixture.
- Prove state and event equivalence before treating a speedup as valid.
- Measure candidate retrieval, arbitration, commit, event production, and view extraction, not only arithmetic kernels.
- Check sparse memory growth and absence of routine whole-population pair scans.
- Measure pause/command responsiveness while accelerated time runs.
- Exercise snapshot and save/load consistency under worker activity.

Extend shared workloads as mechanics are added and use profiling when a cost needs investigation. A new mechanic does not require a separate benchmark suite or written cost report. Investigate reproducible regressions on fixed hardware; do not turn noisy CI timing into a gameplay correctness failure. The complete headless game must meet validated budgets at 10,000 people before graphics is treated as the remaining integration task.
