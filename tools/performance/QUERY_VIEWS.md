# Pure-query caches: measured, not promoted

Two owner-local cache designs preserved complete histories but did not establish
consistent end-to-end speedups. They were removed from the main simulation.
The Rust/manifests source was restored exactly to the previous best snapshot,
SHA-256 `c55398c794060deab6de6294d6e2aa7a9c18494871481eef9f6136c2707645cf`.
The prototypes, their source snapshots, executables and measurements remain under
`target/history/optimization/` for review.

## Designs evaluated

`game-inscription` classified goods once per catalog, selected supplies using a
streaming minimum with the original key and first-tie rule, and cached the pure
selection. Capability/locality checks still executed every time. Weak catalog,
carried inventory/goods, identified-item and marked-surface versions guarded
reuse. After unrelated global edits, local row versions could still validate an
entry. No observation, property request or `Make` call was skipped.

`game-horticulture` materialized the actor's carried/recalled stocks and eligible
crop keys. It preserved stale beliefs, carried-stock precedence, lexical order,
alias decoding and the first-64 remembered-fact limit. Personal stock/location
payload versions and carried-goods versions guarded reuse. Crop versions guarded
seed keys. The stock image and crop snapshot remained at their original points
in proposal generation; scoring and each typed offer still ran in order.

The first design used 256 inscription slots and 64 gardening slots per thread.
The revision used 2,048 slots for both, a dense material-classification lookup for
good IDs below 4,096, ordinary lookup above that bound, and lazy tool
classification after finding a usable surface. Both retained bounded fallbacks
and no saved cache state. These were materialized views of pure computations,
with dependencies owned by the feature that understood them.

This differs from earlier routine/review/care-filter experiments: those changed
which decision opportunities occurred or were considered. Their mixed outcome
changes are documented in [EXACT_REPLAY.md](EXACT_REPLAY.md). The cache experiments
changed no decision interval, candidate set, scoring rule or action timing.

## Whole-history results

Three alternating serial pairs per workload, against the frozen previous best
PGO executable. Positive values mean less wall time; negative values mean more.

| Workload | First design | Revision | Revision, CPU pinned |
| --- | ---: | ---: | ---: |
| Opening, seed 7 days 10–12 | −3.4% | −2.7% | −11.7% |
| Older, seed 11 days 300–330 | −0.5% | −14.3% | +10.8% |
| Older, seed 19 days 300–330 | −0.2% | +2.9% | +5.4% |
| Older, seed 37 days 300–330 | +1.6% | +1.8% | 0.0% |
| 1,000 founders, seed 23 days 1–2 | −3.0% | +2.7% | −2.3% |

There is no reliable general win here. In the revised unpinned seed-19 run, the
same control ranged from 15.085 to 17.549 seconds. Pinning every process to CPU 2
removed migration but did not remove variability: the scale control ranged from
30.020 to 35.898 seconds. The conflicting small gains/regressions should not be
promoted based on whichever median is favorable. Raw per-pair times, CPU time,
RSS and hashes are retained in each comparison report.

The 1,000-founder workload is a young founding history, not a mature population
of 1,000 survivors. All runs used one worker, zero preparation window, serial
listeners, no appraisal cache and no diagnostic tracing. No builds, tests,
profiling or other simulations overlapped accepted timing. The pinned study
reused the already-passing worker/reload checks for the same source and binaries
and checked every new complete replay against those artifacts.

Every full world, ledger, event journal and individual snapshot matched across
builds and repetitions in all three studies. Separate four-worker runs and
mid-history save/load matched too; the one-day scale interval used four workers
without an interior reload day.

Both source candidates used the same frozen PGO profile as the control, without
new training. LLVM discarded unmatched counts in three changed horticulture
functions; the warnings are retained in build logs. Fresh training could change
performance and was not measured. Instrumented component timings cannot override
the unprofiled whole-history measurements.

## What the query measurements established

Untimed counts show that gardening reuse was 98.8% in seed 19 (57,735 hits / 692
misses) but 15.0% in the larger founding case (9,693 / 54,878) with the first,
64-slot table. Inscription reuse was 93.5% and 83.3%, respectively. These counts
motivated the larger revision; a high hit rate alone did not prove a useful
end-to-end gain.

A separate synthetic microbenchmark compared original supplies, warm memo hits,
and the revised dense/lazy search without result memoization. Median nanoseconds
per query, three rotating rounds of 30,000 calls:

| Inventory | Original | Memo hit | Dense/lazy uncached |
| --- | ---: | ---: | ---: |
| Empty | 104 | 88 | 75 |
| One good, no surface | 142 | 88 | 84 |
| Eight goods, no surface | 397 | 92 | 133 |
| 64 goods, no surface | 2,326 | 87 | 626 |
| Two goods including surface/tool | 230 | 89 | 196 |
| 66 goods including surface/tool | 4,861 | 89 | 2,800 |

This confirms large local savings on wider inventories. It does not demonstrate
a corresponding simulation speedup or model production miss rates. The harness
used the existing PGO-built native libraries without the final whole-program
ThinLTO pass and the same mimalloc allocator; it was CPU pinned and ran only
after compilation and history timing finished. The substantial remaining review,
proposal, observation and action work still executes. A different scope of
incremental computation is needed for a large whole-history win.

The initial `gprofng` sample also had an interval-timer warning; its sample
percentages are not reliable enough to estimate gains. Span counts and complete,
unprofiled replays supplied the useful evidence.

## Artifacts and validation

Paths are relative to `target/history/optimization/`:

- `query-views-source.tar.gz`, `query-index-source.tar.gz`: exact prototype Rust /
  manifests plus affected owner READMEs. Prototype source SHAs are respectively
  `b61bb9d1e214b1fed28ccc644e8f69536be4892097e8b95bcdc5a0eac411747b`
  and `76873c11958aa25b1de7c47245145ba6ce4c9133e83393e0fe79df10becab34d`.
- `candidate-query-views-pgo`, `candidate-query-index-pgo`: frozen executables.
- `query-views-study/`, `query-index-study/`, `query-index-pinned-study/`:
  comparisons, final replay artifacts, logs and summary results.
- Corresponding `*-runtime.json` files: source/binary/profile/compiler hashes,
  build/validation provenance and the decision not to retain the prototypes.
- `query-views-profile-{control-19,candidate-19,candidate-scale}/summary.json`:
  untimed span/count data.
- `query-profitability.rs`, `query-profitability`, `query-profitability.jsonl`:
  synthetic query harness, executable and observations. Its source paths refer
  to the archived revision, which must be restored to rebuild it.
- `query-views-bench.py`, `query-index-bench.py`,
  `query-index-pinned-bench.py`: coordinators; they reject mismatching source.

27 prototype Rust tests passed across inscription, horticulture, literacy and
pastimes. New differential tests retained the original searches as oracles and
covered edits, aliases/first-64 limits, catalog/world isolation, separate tools,
methods, movement, marking, nested rollback, changed crop requirements, future
query time and four-worker save/load. Strict Clippy for all targets of the two
owners, formatting, whitespace and architecture/source-budget checks passed.
The added tests were archived with the rejected implementation. No source budget,
save schema or ordinary simulation rule was changed in the retained main source.
