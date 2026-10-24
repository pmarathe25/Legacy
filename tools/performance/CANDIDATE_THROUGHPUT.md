# Candidate collection and focused-policy compilation

This follow-up preserves the adopted [busy-attention policy](BUSY_ATTENTION.md).
It targets work inside the existing decisions, on main
`3bb8963d522b87873b4d6e4aa83cbb1f6e8024bb`.

## What the new profile shows

Separate diagnostic profiles reproduce the complete accepted production-policy
artifacts on seed 37's days 300–330 and the 1,000-founder days 1–2 continuation.
Candidate collection contains about 62.5% and 55.3% of sampled CPU respectively;
these are inclusive shares, not independent costs to add together. The mature
case performs 48,203 full reviews, and the dense case 55,047. Mechanical choices
are prominent in the mature case; care perception is prominent in the dense case.
Profiles are diagnostic, excluded from accepted timings.

## Internal changes

`game-offers::Collection` retains proposals in slots and maintains the top-K heap
as small `(slot, rank)` entries. Sifting swaps indices and numeric ranks instead
of executable requests, strings and motive records. Final large collections sort
indices by the original full key, then apply a permutation with at most N swaps
of full records. Small collections keep the direct sort. Continuation precedence,
unsigned-ID ties, saturation, changing trait weights and exact key comparisons
remain unchanged. Hashes are an index, not proof of string equality.

The ordinary `offer` helper can reject a provably losing header before allocating
its outer executable payload and dispatching an inner offer request. This path
requires an already ranked full collection with matching current weights, no
planning capture and no `Offer` before-hooks. Possible duplicate/colliding keys
and changing weights use the existing handler. Invalid keys still fail, and
hooks still receive losing offers. This is limited late materialization: nested
payloads constructed by callers can still incur work. It is not a claim that all
unselected alternatives have become free.

`game-invention` builds a disposable area/terminal projection of device rows,
including stable lookup and offer keys. A weak storage revision validates it.
Device edits, world switches, loads and rollback invalidate conservatively.
The projection stores geometry and keys, not another person's knowledge or a
world snapshot. Actor-specific mechanical knowledge and motives remain live.
An `Offer` hook restores live eligibility and inventory queries between offers.
The same maximum-mass tie behavior and continuation payloads are retained.

`game-signals` separates ordinary perception calculation from the prior-state
snapshot needed by deferred assessments. Ordinary notice no longer clones an old
perception it discards. One assessment resolves its prior perception and memory
expiry once. Learning, appetite forecasts, cue interpretation, exact expiry
boundaries and owner writes retain the same formulas.

No review opportunities, actions, random draws, stimuli or physical durations are
intentionally removed by these changes. All state mutations stay with their
existing owners. Collection, device projection and signal calculation have named
modules rather than a second simulation engine.

## Screens and compiler training

Single-pair screens are retained, including neutral/regressive results:

| Screen | Mature control / candidate seconds | Dense control / candidate seconds |
| --- | ---: | ---: |
| Compact slots and indirect ordering | 26.963 / 25.049 | 33.747 / 34.141 |
| Plus guarded rejection and device projection | 23.974 / 23.728 | 33.174 / 33.417 |

These screens do not establish a substantial general speedup. They also show
enough run variation that a single favorable pair is not a precise estimate.
Reports are `compact-offers-screen/` and `candidate-views-screen/`; each checks
complete replay equality and fingerprints its engine and inputs.

The control uses a transferred PGO profile trained predominantly on an older
decision-frame model. A fresh portable profile trains the final code with the
current focused-attention model, on predeclared seeds 17, 29 and 43: a 32-founder
30-day history, the default population over five days, and 1,000 founders over
one day. Held-out seeds 19/37 and the seed-23 dense checkpoint do not enter
training. Compilation, training and tests do not overlap accepted timing runs.

```sh
export RUSTUP_HOME=/workspace/.rustup CARGO_HOME=/workspace/.cargo
export PATH="$CARGO_HOME/bin:$PATH"
RUSTUP_TOOLCHAIN=1.97.1 CARGO_NET_OFFLINE=true \
LEGACY_MEMORY_INDEX=0 LEGACY_APPRAISAL_CACHE=0 \
python3 tools/performance/pgo.py \
  --training tools/performance/training/focused.json \
  --output NEW_PGO_DIRECTORY --build-jobs 4 --discard-build-cache
```

The build tool fingerprints Rust/content/manifests, compiler, training inputs,
plan and driver. It rejects changes during training and removes its own temporary
build caches when requested. Both engines retain portable O3, ThinLTO and one
codegen unit; no fast floating-point arithmetic or CPU-specific requirement is
introduced.

The [versioned training recipe](training/README.md) includes the complete large
world profile, so a fresh checkout can build without local experiment artifacts.
The measured run's original plan and raw results remain under
`target/history/optimization/`.

The exact comparator resumes immutable inputs that already contain the adopted
15-minute busy-only rules. `ordinary`/`ordinary-reference` in its report mean no
CLI policy override, not the old attention policy. Both engines load the same
stored rules. The comparison uses full `canonical-evidence-v2` world hashes and
byte-exact ledger, journal and person snapshots; raw hashes are also retained.
Cross-worker and midpoint-reload checks run separately from timing medians.

```sh
python3 tools/history/attention.py \
  --binary NEW_PGO_DIRECTORY/game-history \
  --reference-binary target/history/optimization/busy-triage-pgo/game-history \
  --fixtures target/history/optimization/focused-inputs \
  --models ordinary-reference ordinary --cases heldout-19 heldout-37 scale \
  --rounds 3 --verify --hash-mode canonical-evidence-v2 \
  --output NEW_COMPARISON_DIRECTORY
```

## Repeated single-history results

Three alternating pairs per case, one worker and identical stored rules:

| Continuation | Previous median seconds | New median seconds | Less wall time | Less CPU time |
| --- | ---: | ---: | ---: | ---: |
| Seed 19, days 300–330 | 22.435 | 17.395 | 22.5% | 22.5% |
| Seed 37, days 300–330 | 26.581 | 20.458 | 23.0% | 23.1% |
| 1,000 founders, days 1–2 | 35.869 | 30.251 | 15.7% | 15.7% |

Complete worlds agree under the declared normalization; ledger, journal and
people are byte-exact. Candidate four-worker replays also agree, including
day-315 reloads for both mature cases. Verification times do not enter medians.
This compares the final source changes **and** fresh PGO against the previous
production build. It does not attribute the gain to compact ranking or any other
individual edit. The much smaller source-only screens remain part of the record.

Wall ranges were 21.704–22.651 versus 15.483–17.459 seconds for seed 19,
26.532–26.787 versus 20.321–20.458 for seed 37, and 35.663–35.930 versus
30.209–30.389 for the dense case. The favorable first seed-19 pair is deliberately
not used as the representative percentage. Three pairs are useful evidence on
these workloads, not a universal performance guarantee.

Peak RSS medians were 73.7→74.5 MiB, 91.0→90.2 MiB and 338.5→331.4 MiB.
There is no substantial general memory reduction claim. The primary report is
`focused-candidates-exact/comparison.json`. Frozen code/content, compiler/training
metadata and the build recipe are indexed by `focused-candidates-runtime.json`.
Use `target/history/optimization/focused-compiler-pgo/game-history` for this
measured portable build; a plain release build has not received its fresh PGO.

The first instrumented link exhausted disk space. Only rebuildable `target/debug`
artifacts were removed; source, frozen engines and histories were retained. The
build resumed before training, with the original source/input guards. Its failure
and restart logs are retained. This did not overlap any accepted timing run.

## Independent-history concurrency

`tools/history/batch.py` adds bounded process concurrency for separate saved
worlds. Each child has one simulation worker and its own output/state. Errors
request committed-day stops for sibling jobs and cancel queued jobs. Output names
are validated and existing output roots are refused. The tool hashes its plan,
binary, input checkpoints and drivers, and requires complete per-history equality
across its declared concurrency levels.

Four predeclared conditional future continuations (seeds 101–104, same day-300
population/past, through day 330) were run with concurrency 1 and 4. Two rounds
alternate group order: 1/4, then 4/1. Only the declared children overlap within a
timed group; builds, profiling, tests and other simulations remain separate.
Post-group hashing is excluded. The hardware quota is four CPUs.

| Measure for four histories | Serial | Four concurrent children |
| --- | ---: | ---: |
| Median group wall seconds | 76.106 | 31.150 |
| Individual group wall seconds | 79.157, 73.056 | 35.490, 26.811 |
| Median total child CPU seconds | 75.837 | 99.136 |

This gives **2.44× batch throughput**, or 59.1% less elapsed time. It applies to
multiple independent worlds. CPU cost rises about 31%, and unequal history
lengths and shared hardware limit scaling. The run variation remains visible;
two groups per concurrency level do not establish a precise universal ratio.
The sum of child peak RSS is about 361 MiB in both modes; this is an upper bound,
not a simultaneous peak. Serial execution keeps only one child resident at once.

Complete artifacts agree between serial/concurrent runs and the preceding
production engine's retained per-seed hashes. No cross-world information or shared
simulation resource is introduced. The report and full outputs are retained in
`focused-batch-throughput/`.

```sh
python3 tools/history/batch.py \
  --plan target/history/optimization/focused-batch-plan.json \
  --output NEW_BATCH_DIRECTORY --concurrency 1 4 --rounds 2 --retain-state
```

The plan contains `binary` and `runs` entries with `name`, `checkpoint` and absolute
end `days`; paths resolve relative to the plan. A single concurrency level runs a
normal batch, while multiple levels provide a controlled throughput comparison.
Continuing multi-century studies can still use the existing chunk/archive runner;
this new tool handles fresh replay outputs from frozen checkpoints.

## Validation and limits

The final code passes 170 Rust tests across choices, invention/mechanics, signals,
care, characters, assembly and the history runner, including architecture/source
budgets. Fifty-four Python history-tool tests pass. New cases cover indexed
ordering below/at the retention limit, saturating ties and changing weights,
hook-visible losing offers, rejected invalid/duplicate keys, device geometry/area
changes and rollback, bounded process overlap and prompt sibling stops after a
later child's failure. Affected-crate/history Clippy passes with warnings denied.

Supplementary complete-state checks cover opening and older populations, legacy
saves with their original attention policy, and a fresh independently generated
seed-71 world. The fresh candidate uses four workers and reloads on day 2. These
checks are separate from the three-pair performance medians and retain their
own reports (`focused-additional-exact`, `focused-legacy-exact`, and
`focused-fresh-verification.json`). Existing saves keep their stored rules.

`focused-candidates-findings.json` audits source/content/compiler/training/input
fingerprints, complete replays, timings and validation logs. Its source archive
includes runtime content, and `focused-candidates-evaluation.tar.gz` freezes the
Python evaluation tools and tests. No existing study plan is silently upgraded;
the continuing-study runner retains its per-checkpoint replay-evidence guard.

This preserves the current simulation's decision opportunities and emergent
histories on the tested workloads. Further large within-world parallelism still
needs larger independent read batches and clear dependencies for effectful
providers. The separate snapshot/actor-model experiments and their behavioral
tradeoffs remain documented in [the concurrency evaluation](CONCURRENCY.md).
