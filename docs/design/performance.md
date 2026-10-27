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

## Parallel-first actor pipeline

The current frame experiment parallelizes preparation, but observation and
action commits still pass through ordered transactions over feature-wide
resources. More workers cannot scale work that remains behind those boundaries.
Explore a separate phased model that exposes owner-controlled disjoint updates:

1. Materialize immutable, revision-tagged local scenes at a simulation boundary.
2. Update each observer's own perception and memory in parallel, subject to the
   same visibility and personal-knowledge rules.
3. Prepare decisions from the committed observations in parallel and produce
   typed intents naming participants, stock claims, expected versions and effects.
4. Resolve overlapping claims deterministically; reserve participants and resources
   atomically for joint actions. Execute disjoint conflict groups concurrently.
5. Publish action results, owner events and scheduled work at a defined boundary,
   then deliver evidence in a subsequent causal wave.

Feature owners retain validation and rule authority. Storage needs row- or
partition-level ownership and rollback rather than unrestricted mutable access to
an entire feature resource. Unconverted mechanics must have an explicit ordered
fallback. Start with an end-to-end slice covering observation, food or work choice,
shared-stock arbitration, action execution and personal evidence; a scoring-only
kernel would omit the dominant coordination costs.

The experimental stock planner currently provides advisory allocation only; it
neither reserves nor consumes goods. A review-local selection cursor now separates
choosing the next scored alternative from invoking it, without changing review
admission, sensing, stale checks, failure learning or finalization. It cannot
cross a transaction boundary. A possible next slice would suspend eligible reviews
after their selected local meal passes the stale check, arbitrate those actual
selections, then execute admitted reviews through the existing food and cargo
owners. A provisional loss should try another alternative without failure learning.
An admitted intention still needs current-state and access validation; any
intervening effect or urgent wake can invalidate it. Unconverted intentions and
review hooks need an explicit ordered path. Suspending a review cannot call the
normal resume request again, because admission and wake cleanup already occurred.
In particular, review admission marks an actor as waking at the current time;
the urgent-wake queue can then coalesce a later alert without creating an urgent
job. A paused review therefore needs its own invalidation signal, recorded before
wake suppression, and must discard its old prepared decision and reprepare after
an intervening urgent change. Checking only the presence of an urgent job is not
sufficient. Ordinary synchronous reviews do not suspend between actors; the
experimental selected-meal cohort below interleaves a bounded group instead.
An experimental transaction-local paused-review marker and urgency serial now
record these invalidations before suppression, including cues during selection and
hooks before the pause is armed. They must be closed before commit and roll back
with nested attempts. The opt-in local-meal cohort clears the frame and prepares a
fresh review when an intervening urgent cue invalidates a selected actor.

Prepared proposal payloads may also be unbound after reload even when their wire
key is valid; selected-intent inspection must use the same checked typed-or-wire
decoding as frame decisions. `decode_pending` supplies this inspection to meal
arbitration. Test loading after preparation and before resolution,
so reload does not silently change which intents participate in arbitration.

This opt-in slice tests arbitration of selected local self-meals, with urgent
claims first and a persisted participation counter rotating actor order before
bounded groups are formed. Each group resolves its own stock conflicts; ordering
is not a guarantee of starvation freedom as participants and resources change.
Denied claims refresh personally visible stock without recording an action
failure, and failed grants allow denied claims to be reconsidered against the
remaining stock. Resume hooks force the original ordered path. It still does not
parallelize action effects. Cohort selection does not yet provide per-actor
causal-review trace attribution; use ordinary resolution for those traces.
Cargo change listeners reconstruct
per-operation stock and item-lot state, so batching multiple mutations before
publishing old notifications is unsafe. A parallel commit requires a revised
owner-defined effect contract with exact per-operation data, independent row
updates, deterministic event publication and rollback across owners.

Cargo notifications now include the source's exact before/after quantities and,
for a transfer, the destination's quantities. These describe the mutation within
its enclosing transaction and are undone with it. The named-item listener uses
the captured source amount only when it still matches the live post-mutation
amount; if an earlier reentrant listener changed stock, it retains the original
live reconstruction. Old notification payloads may lack the optional snapshots.
This prepares one effect contract but does not parallelize commits or make it
safe to batch notifications: named-item, cultivar, preservation and maturation
effects remain ordered and require their own conversion or fallback.

The provisional arbiter can now claim every required participant as well as
stock: any common participant joins the same conflict group, and admission is
all-or-nothing. This is not an actual reservation. The sparse packed-row API
can journal and mutate distinct existing owner rows concurrently, rejecting
duplicates and missing rows before mutation. It does not publish effects or
call requests, and its parallel path scans the full row array; callers must
measure their own threshold. Participant claims are not yet wired into a joint
action; the selected-meal wave below uses the row facility for body effects.

An opt-in physiology-owned self-meal wave is connected to the selected-meal
cohort. It admits and revalidates participants, performs property, eating and
individual cargo notifications in order, and uses the body owner's disjoint-row
batch for survivors. Digestion, nutrition and vitality remain ordered. Fed
observations occur after all meal body and mood effects, so this is a distinct
behavioral model. Incompatible hooks, duplicate participants, invalidation,
and potential death fall back to the ordinary ordered path. A transactional
attempt rolls back stock, personal rows, notices, schedules and identity
counters before cohort fallback. Worker callbacks cannot call requests or
allocate IDs. This slice still needs end-to-end performance and welfare screens.

Phased arrival processing is another candidate for owner-controlled effects:
validate transit and action completion, update personal locations and knowledge
in disjoint owner rows, merge shared area indexes, then publish evidence and
wakes in a deterministic order under one rollback boundary. Passenger, household,
guardian, logistics, mission and carried-stock listeners need explicit conversion
or ordered fallback. A one-day mature small-population trace found only singleton
arrival cohorts after the experimental journey cooldown, even in 15-minute bins.
This is not evidence about larger worlds, but it does not justify assuming a
same-time arrival wave will speed that history. Windows spanning different event
times are a distinct behavioral model and need latency and welfare evaluation.

An opt-in food-refusal experiment now records bounded observations of actual
declines and schedules a coalesced retry, while allowing other food and serving
choices and responding to relevant observations and urgency. It is a distinct
behavioral model under validation, not an adopted history policy.

Repeated declined food offers are a more frequent mature-history workload than
completed workshop products. A caregiver can choose the same food and recipient
again after refusal because no observed refusal is retained. Evaluate an explicit,
bounded memory of the caregiver's observed refusal that permits other foods and
reacts promptly to new evidence or urgent need. Do not use private recipient
state as the caregiver's knowledge, suppress all care for a patient, or assume a
time-only delay preserves responsiveness. Measure failed offers, food delivery,
care latency and welfare as well as throughput across workers and reload.
The record should distinguish caregiver, recipient, food and serving size, and
must not survive a rolled-back attempt. A rejection of one food must not block a
different food, another caregiver or urgent reassessment. Any future recheck
mechanism needs a bounded wake for time-dependent appetite, coalesced generation
tokens, death cleanup and invalidation when the relevant observed situation
changes. Validating only a nutrition version is insufficient when willingness
also changes with time and action commitments. Exhausted-option caching must
also detect newly observed food and must not replace the actual serving owner's
validation. Treat this as a proposed behavioral model, not an implemented speedup.

Signal completion alone is not an independent row update: a signal reaches the
food-care listener through the communication event re-export, which can produce
perception changes and decision wakes. A parallel signal wave must account for
those effects and their ordering rather than treating the published event as
subscriber-free.

Classify competing claims by actual people, goods and locations, rather than by
whole-resource type. Use a specified deterministic fairness policy for equal
claims, and measure starvation. A stale prepared intent requires bounded replanning
or an alternative; it does not automatically imply a personal failure. Allocate
stable identities and random occurrence keys independently of worker order and
scheduler bookkeeping.

Urgent detected changes must invalidate affected intentions and receive a prompt
causal phase independently of broad planning cadence. Persistent goals may advance
on their natural completion events with owner validation; broad reconsideration
can have a slower bounded deadline. These are behavioral-model changes. Test
response latency and social, cultural, survival and productive participation, and
compare multiple seeds and mature worlds before choosing them for a study.

The broad nonurgent shortlist experiment suppresses every non-food provider during
its reuse window. Its short histories changed conflict and mortality as well as
social and cultural activity, so it is not a safe proxy for an interrupt-driven
policy. A narrower next experiment should keep care, conflict responses, family,
and other safety-relevant providers live while reusing only explicitly opted-in
provider results. A cached empty result must remain distinguishable from a missing
cache entry. Cached offers need their provider identity, original collection order,
bounded lifetime, and live revalidation before execution; a selected stale offer
must not be treated as a personal failure. Urgent cues must invalidate both a
prepared decision and the relevant persistent entries, including during an open
cohort. Hooks and providers with sensing or other effects require the live path
unless their semantics are explicitly supported. Persist model settings and any
behavior-affecting cache state so reload and worker scheduling cannot change the
history. These requirements are a design target, not an implemented cache.

A possible implementation keeps the normal `Collect` call for every actor and
reuses only explicitly selected provider output at its original listener boundary.
Capture the provider's ordered raw offers before global pruning, not just the
winning shortlist, then submit them through the normal collection with their
provider identity. Distinguish an empty captured result from a miss, make the
stale-selection recheck force a live provider call, and carry cache deltas from
parallel drafts into owner-controlled commit. Do not reuse provider outputs that
contain continuations, deferred effects or unsupported hooks. Detect intervening
urgent changes before applying drafts. Capturing and replaying offers must retain
duplicate-key errors, offer hooks, limit behavior and ordered barriers; otherwise
fall back to live collection. Measure the added capture and memory costs as well
as reduced provider work before deciding if this is worth retaining.
Owner-defined freshness must cover all dependencies and nested rollback; the
generic `Changed` event is not emitted by every relevant write. Compare an
invalidation generation before and after applying deferred effects, even when no
entry existed to clear. An expiry that should trigger reconsideration also needs
a coalesced actor wake; a timestamp alone does not wake an idle actor. An exact
derived cache may be discarded on reload only if rebuilding produces the same
offers, motives and order. A deliberately frozen deliberation must instead save
its entries, consumed alternatives, policy version and invalidation state.

The information provider is a candidate for a more conservative first slice:
cache immutable construction inputs while evaluating availability and motives
against current personal state. Do not use `Facts::namespace_version` alone to
cache answers: it intentionally ignores observation and confidence metadata,
while answer selection ranks by those fields. The information integration test
`answer_offer_reacts_to_metadata_only_personal_knowledge_updates` demonstrates
that an answer can change while the payload namespace token remains unchanged.
Any versioned answer cache must track that metadata as well, including scene
refreshes and rollback, or evaluate answer selection live. Measure complete
simulation throughput and allocation cost on fixed histories before retaining
the added cache machinery; a high local hit rate is not sufficient evidence.

A more substantial candidate is two-tier deliberation: run fresh survival,
care, conflict and urgent-response choices at the existing responsive cadence,
and run fresh broader discovery at episode boundaries or a bounded deadline.
This intentionally changes when ordinary opportunities are considered; it does
not reuse a stale shortlist. Providers must explicitly opt into deferral, with
unknown and effectful listeners staying live. The cheap tier must preserve an
ongoing action when no urgent alternative warrants interruption, and its idle
path must not immediately trigger the full tier on every frame. Deadline expiry
must schedule a wake even for idle actors; injury, care and other urgent cues
must preempt it, including after parallel preparation and before group commit.
Current scheduling already sleeps many busy actors until completion or a
predicted hunger threshold, so simply skipping all busy actors is not this
redesign. Introduce it only as a separate saved model and test provider coverage,
urgent latency, progress without starvation, rollback, worker and reload replay,
and matched welfare and activity outcomes before using it for the history.

The first building block is now implemented: whole-actor drafts can explicitly
select a fast collection scope, and a pure provider can register for deferral
in that scope. Ordinary providers, unclassified listeners and ordered observers
remain active; the full provider is used for stale rechecks. Literacy discovery
is the first explicitly deferrable provider. The information provider also
keeps incoming answers and refusals live but defers outgoing questions and
document discovery in fast scope, retaining its single listener and full-review
order. An opt-in scheduler policy now uses a saved per-person full-review deadline
and urgent invalidation generation; the long history has not adopted it. No
performance or welfare claim is established yet. A provider must not opt in if its
callback includes required sensing or other effects, because skipping the
callback prevents the framework from observing them.

Regional timelines may expose work across different simulation times, but need
explicit ownership transfer, timestamped messages and a safe inbound horizon.
Prior traces found little same-time geographic independence and a concentrated
older population, so region partitioning alone cannot address a busy settlement.
GPU candidates should likewise be drawn from large resident numeric phases, not
assumed to accelerate irregular state mutation or small decision batches.

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
