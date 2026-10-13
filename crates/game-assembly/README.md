# game-assembly

`game-assembly` connects domain crates, registers shared component schemas, builds playable openings, and binds player commands and knowledge-limited views to the headless simulation. Cross-feature effects pass through typed owner interfaces.

The live simulation uses `game-world::GenericHost<AssemblyState, EventKind, Components>`. The host owns the single Store, clock, seed, checked identity counters, and typed event ledger. `AssemblyState` holds feature runtimes and rules, authored definitions and content stamps, area metadata, and application coordination such as due events and accepted command receipts. These are runtime metadata and coordination state; canonical per-entity game records live in `Components`, the shared store. A runtime can hold feature rules, cursors, or derived indexes without becoming a second owner of entity state. `canonical_state()` borrows both state and store; `canonical_snapshot()` makes an owned save or inspection snapshot. Loading validates registered schemas and contextual references before rebuilding query bindings.

Assembly registers schemas and systems, builds openings, binds public commands and views, supplies factual adapters, and sequences cross-feature effects. Command dispatch preflights the publication budget and routes each action through its feature operations; operations that compose guarded effects stage a batch and submit it through the generic transaction boundary. A command receipt and accepted sequence are recorded only after dispatch succeeds. World advancement prepares the existing ordered interval work in a preview host, computes the changed component batch and its typed read projections, then submits the batch, final logical time, ID and revision budgets, and event records through one `game-world` transaction. The live host publishes that interval only after opening-state and owner guards pass. Domain owners choose actions, progression, and event reactions. `game-sim` supplies scheduling and execution, and `game-session` hosts the public engine port.

## Benchmarks and limits

Run benchmarks from the repository root with `--offline -j2` when dependencies are cached:

```sh
cargo bench -p game-assembly --bench interval_staging --offline -j2
cargo bench -p game-assembly --bench full_world --offline -j2 -- 1000 1 120 --save-to=/tmp/legacy-world-1.save
cargo bench -p game-assembly --bench full_world --offline -j2 -- 1000 4 120 --save-to=/tmp/legacy-world-4.save
cmp /tmp/legacy-world-1.save /tmp/legacy-world-4.save
cargo bench -p game-assembly --bench kernel --offline -j2 -- --max-population=1000
cargo bench -p game-health --bench cancellation --offline -j2
cargo bench -p game-conflict --bench encounters --offline -j2
```

`interval_staging` measures one production global wrapper after a warm-up interval, with five samples per case. It uses the small generic kernel at 1,000, 2,000, 4,000, and 10,000 actors; one and four workers; zero or 100,000 synthetic old failed-transfer receipts; and a real public movement command for arrival cases. Setup and warm-up are excluded. These receipts stress retained history, rather than represent a played scenario.

On an Intel i7-8550U, the optimized build produced these ranges of case medians in milliseconds:

| Actors | Global wrapper | Transaction publication |
| --- | ---: | ---: |
| 1,000 | 15.6–18.5 | 7.1–9.3 |
| 2,000 | 30.0–35.9 | 14.8–17.9 |
| 4,000 | 74.5–78.7 | 38.3–41.3 |
| 10,000 | 201.1–212.6 | 107.0–113.4 |

The attached-component projection change reduced wrapper time by 12.6–16.6 times against the same pre-change matrix. Global staging still copies the full shared store and validates its differences; its cost grows with attached rows. Four workers provide little benefit in this fixture because that work is serial. These timings do not establish complete-game performance.

`full_world` constructs the authored populated opening and advances real autonomy, movement, production, interactions, and bodily needs. It measures initialization, advancement, observation, save, and restore separately; verifies a canonical save round-trip; and checks completed activities and surviving population. The optional `--save-to=PATH` writes the measured final save for exact worker-count comparison. Population, workers, and simulated minutes are positional arguments; `policy` as the next argument enables a player acting through public observations and commands.

The composed 1,000-person, one-worker run advanced two simulated hours in 516.9 seconds across 151 boundaries. It kept all 1,000 people alive and completed 10 journeys, 20 transfers, 14 production events, and 10 conceptions. Initialization took 1.64 seconds, observation 13.52 seconds, save 59.8 milliseconds, and restore 166.4 milliseconds. These are single-run measurements on the same machine, not latency distributions.

Its largest recorded phases were `review.apply` at 347.2 seconds (including `apply.Other` at 321.4 seconds across 53,228 attempts) and `review.rank` at 85.5 seconds. Interval publication took 23.4 seconds and evaluation 6.3 seconds. Broad feature work and observation are substantial measured limits in the composed game; the generic interval speedup does not remove them.

Four workers advanced the identical two-hour workload in 455.9 seconds, a 1.13-times overall improvement. Ranking improved to 25.3 seconds (3.38 times), while application still took 346.6 seconds and observation 13.76 seconds. Both runs reached exactly 7,200,000 milliseconds without a caller-input stop, passed canonical save/restore checks, and produced identical 13,473,536-byte saves. This proves worker equivalence for that workload; it does not establish sustained 10,000-person throughput.

The 10,000-person measurement used one worker and only one simulated minute. It reached exactly 60,000 milliseconds without a caller-input stop, kept all 10,000 people alive, and recorded 100 conceptions; no movement, production, or social activity completed in that short horizon. Initialization took 171.8 seconds, advancement 389.1 seconds, observation 1,342.6 seconds (22.38 minutes), save 471.7 milliseconds, and restore 1.59 seconds. Its 129,873,113-byte save passed the internal canonical restore assertion. No second worker run was made, so this sample provides no 10,000-person worker comparison or sustained-play claim. Startup, application work, and observation remain major limits at that scale.

`kernel` retains deterministic synthetic contention, sparse lookup, creation/removal, parallel evaluation, and session-response workloads. The optional `--max-population` caps the population matrix; omitting it runs through 20,000 people. Its separate session-response workload always uses 10,000 people. Health cancellation and Conflict encounter benchmarks exercise owner operations directly. A dense encounter stores its fighters together, so guarded turns still clone that encounter; the maximum-size encounter benchmark measures this cost rather than assuming cheap individual turns.

The capped kernel run compared one, two, four, and eight workers and asserted identical canonical outcomes. At 1,000 actors its sixty-minute fixture medians were 963–981 milliseconds; extra workers gave little overall gain. Its separate contested typed workload improved from 35.1 milliseconds with one worker to 19.8 milliseconds with four workers. These are synthetic access and scheduling measurements. The generic 10,000-person session measured observation p99 at 237.6 milliseconds and pause at 204.8 milliseconds; its simpler observations do not represent the composed game's slow view above.

Health's three-sample cancellation medians were 10.6, 25.9, and 64.9 milliseconds at 1,000, 2,000, and 4,000 actors, showing superlinear cost. Conflict's mean round preparation was 0.276 milliseconds at 128 fighters and 14.897 milliseconds at 4,096 fighters; a full 128-fighter perception-and-wait round took 267.6 milliseconds. These owner timings retain their actual guards and publication work.

The explicit release `health_index_workload_timings` test compares indexed results with exhaustive reference scans. At 10,000 people, exposure indexing took 28.9 milliseconds versus 3.43 seconds; observation took 131.9 milliseconds versus 74.83 seconds. It asserts identical exposures and canonical personal knowledge. The exhaustive scans are test references, not production paths.
