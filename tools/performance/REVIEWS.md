# Review causality and incremental appraisal

This experiment keeps ordinary review timing, provider order, observation effects,
proposal payloads, choice rules, and action execution. The reusable computation is
subjective appraisal used by family and caregiving choices, rather than an entire
provider with a large implicit input surface.

## Incremental views

`game-attraction` splits physical appraisal and compatibility into named modules.
Compatibility is an incremental join of an observer's own values and personally
heard opinions. Each observer view checks weak value/heard-row revision tokens and
current beliefs before reusing pair scores. Beliefs matter because `sharing`,
`authority`, and `honesty` override ordinary learned values. Emotional relationship
terms are always read afresh. Another observer's changes do not invalidate this
view. The original sorted traversal, exclusions, eight-opinion limit, clamp, and
integer arithmetic remain intact.

Physical appraisal checks exact time, observer values, trait definitions/profiles,
and vitality revisions. It reuses numeric ideals and apparent builds within that
validated frame. Every query still checks current physical visibility. Global
trait/vitality invalidation is deliberately conservative. Weak tokens retain no
old world snapshot. Values are disposable and thread-local, bounded to 1024
compatibility observers and 128 physical observers, with at most 64 partners each.
Slot collisions and capacity eviction affect performance only.

`LEGACY_APPRAISAL_CACHE=1` enables the experimental views. The default uses the
original formulas; `0` explicitly disables the views. The initial paired study
found small gains on opening/older populations and a regression on the large
checkpoint, so the prototype is opt-in. `tools/history/appraisals.py` alternates both settings in the same frozen
ordinary release executable, checks all four replay artifacts, compares a previous
executable, and separately verifies four-worker and intermediate-reload runs.
Whole-world JSON uses canonical object ordering; ledger, journal, and person
snapshot hashes are exact bytes. Instrumented runs do not supply accepted timings.

`game-character-state` contains the saved `Minds` resource and checkpoint contract.
`game-characters` re-exports, registers, and exclusively writes that state. Its
owner, wire key, schema and plugin version remain unchanged. This extraction keeps
review orchestration within the existing source budget without raising it.

## Optional tracing

```sh
cargo +1.97.1 build --locked --offline -j 1 --profile simulation \
  -p game-history --features review-trace
LEGACY_REVIEW_TRACE="$PWD/target/history/review-attempts.jsonl" \
  target/simulation/game-history --resume target/history/optimization/baseline-profile \
  --days 12 --workers 1 --sample-days 1 --final-save false \
  --output target/history/review-replay
python3 tools/performance/reviews.py target/history/review-attempts.jsonl \
  --output target/history/review-summary.json
```

The trace path must be unused: the backend uses `create_new`. A missing environment
variable disables output. I/O failure reports a diagnostic and stops trace writes;
it does not fail gameplay. The ordinary build compiles out the backend and never
evaluates observer closures or candidate iterators.

The infrastructure `game-contracts/causality` backend owns generic scopes and JSONL
output. `game-sim/review-trace` labels nested requests, events, and listeners.
Feature owners add review observations and wake reasons. No kernel dependency on
an autonomy feature is introduced.

Each review-attempt row has a diagnostic ID, person, exact simulated timestamp,
attempt revision, nesting stack, pending wake causes, selected state before/after,
local success, and an ordered numeric proposal fingerprint when available. Wake
rows include suppression, caller hunger-only/immediate flags, nesting stack, and
an enclosing review ID. The state sample covers body, current action, chosen key,
last decision and failure count. It does not enumerate every owner input.

Interpretation limits:

- Attempts can roll back. A locally successful review and `attempt_revision` are
  not proof that its surrounding transaction committed. Tracing is diagnostic;
  complete replay hashes remain the behavioral check.
- The fingerprint covers proposal IDs, numeric motive terms, continuation IDs and
  order. It excludes payloads and many live inputs; its hash can also collide.
  Equality is a lead for investigation, never permission to skip a review.
- Retaining the chosen key does not establish redundancy: observations, learning,
  validation, or a changed payload can still matter.
- Pending wake correlation is thread-local and bounded to eight causes per person
  and 4096 people. It can lose or retain attempted causes around rollback and does
  not follow a person migrating between execution threads. Raw wake rows remain
  available. Use serial listener delivery for the initial causal investigation.
- A row without a numeric fingerprint can be a deferred review, routine shortcut,
  controlled/dead/incapacitated actor, combat commitment, or failure before proposal
  completion. The analyzer does not count it as a completed full preparation.

The analyzer streams JSONL and keeps only the previous review per person. It reports
wake chains, causes attached to reviews, same-clock repetitions, repeated numeric
fingerprints, observed field changes, and retained choices. These identify where
finer dependency tracking or selective provider reuse should be investigated next.

## Predicate pushdown

Family bonding checks now reject actors who are not living adults, conflicting
exclusive commitments, and close relatives before calculating subjective
attraction. A failing desire threshold also skips the comparison with a previous
bond kind. These are the same predicates as before, applied in a cheaper order.
The skipped calculations are read-only; no observation, offer request, learning,
request hook, or event is skipped. Checks remain local to each assent call rather
than being hoisted across offer requests, whose hooks may mutate the world.

This is the query-planning rule of filtering inadmissible rows before evaluating
an expensive projection. It is enabled by default independently of the opt-in
appraisal views. Comparative timings pin the views off in both binaries.

## Measured cache prototype

Three alternating pairs per fixture, portable release ThinLTO/one codegen unit,
one worker, unchanged checkpoints and ordinary decision policy. Measurements use
one frozen executable with the view flag set explicitly in each process.

| History | Original formulas | Prototype views | Wall reduction |
| --- | ---: | ---: | ---: |
| Opening, days 10–12 | 5.094 s | 5.025 s | 1.35% |
| Older population, days 300–330 | 2.289 s | 2.254 s | 1.52% |
| 1000 founders, days 1–2 | 41.172 s | 42.521 s | -3.28% |

The small studies do not justify enabling the views by default. The final code
keeps them opt-in. All 27 complete replays agreed within each workload, including
the previous executable and four-worker checks; opening and older checks also
reload midway. The final timing pair is retained for inspection. Reports and
source snapshot provenance are under `target/history/optimization/appraisal-study`
and `appraisal-runtime.json`.

## Causal findings and pushed-down work

The original and predicate variants produce byte-identical raw causal traces for
all three fixtures. Complete world/ledger/journal/person hashes also agree. These
instrumented runs are excluded from accepted timings; they overlapped builds.

| History | Full reviews with numeric proposals | Same numeric fingerprint as previous | Accepted new-evidence wakes / all accepted wakes |
| --- | ---: | ---: | ---: |
| Opening | 11,157 | 390 | 10,845 / 11,154 |
| Older | 7,243 | 113 | 6,836 / 7,243 |
| 1000 founders | 64,679 | 3,386 | 54,370 / 64,982 |

The largest large-world cause chain is `choice.finish → care.food-review →
choice.changed → characters.new-evidence`, attached to 41,710 review attempts.
Need broadcasts through `nurture.heard-need` are also prominent, especially in the
opening population. Approximately half of all attempted wakes are already
suppressed. Retaining an action key is much more common than retaining its entire
numeric candidate fingerprint. These measurements favor finer reuse inside
providers over treating a retained choice as permission to skip a review.

With the views explicitly enabled for query counters, predicate pushdown removes
expensive visible physical queries while preserving every traced review and wake:

| History | Visible physical queries before | After | Reduction |
| --- | ---: | ---: | ---: |
| Opening | 345,108 | 183,813 | 46.74% |
| Older | 145,797 | 78,241 | 46.34% |
| 1000 founders | 2,381,118 | 1,538,606 | 35.38% |

Compatibility queries fall from 543,079 to 381,784; 246,075 to 168,015; and
3,636,298 to 2,793,786 respectively. The query traces, summaries and compressed
raw JSONL are under `target/history/optimization/{appraisal,predicate}-traces`.
Raw-trace hashes and the frozen instrumented executable hashes are recorded in
each comparison report. Numeric fingerprint equality remains diagnostic only.

## Final portable-release comparison

The control is the frozen executable from before this work (`candidate-attention-release`).
The final executable includes predicate pushdown, the modular state/trace changes,
and the experimental views disabled. Both binaries use portable ThinLTO and one
codegen unit, with no PGO. Each workload has three alternating pairs and complete
four-artifact equality checks. Builds, tests, traces and other simulations do not
overlap accepted timing runs.

| History | Previous runtime median | Final runtime median | Wall reduction |
| --- | ---: | ---: | ---: |
| Opening, days 10–12 | 5.554 s | 5.398 s | 2.81% |
| Older, days 300–330 | 2.297 s | 2.384 s | -3.79% |
| 1000 founders, days 1–2 | 37.917 s | 37.360 s | 1.47% |

The older result prompted additional investigation rather than assuming a gain.
Five alternating pairs pinned to CPU 0 give 2.485 → 2.442 s, a 1.73% improvement
over the previous runtime. A separate five-pair check on that CPU isolates the
predicate change from the earlier appraisal/state snapshot, with views disabled
in both: 2.502 → 2.384 s, a 4.71% improvement. The first three-pair result remains
part of the evidence. These are modest, timing-sensitive gains rather than a
large universal speedup. The large case is a young 1000-founder checkpoint,
not a mature 1000-person multigeneration society.

All comparisons preserve complete replay artifacts. Three further four-worker
runs of the predicate code also agree with the original histories; the opening
and older runs reload midway. Their research build has the tracing feature
compiled in with output disabled. Instrumented old/new runs preserve raw causal
trace bytes, proposal fingerprints, chosen actions and review/wake counts.

Timing uses the fresh Python `wait4` helper shared with `decisions.py`: wall time
includes the measurement wrapper, while CPU/RSS describe the simulation child.
The comparator no longer requires `/usr/bin/time`. Its first attempt failed
before starting a simulation because that executable was absent; the empty
attempt is preserved separately and supplies no accepted sample.

Reproduction artifacts under `target/history/optimization/`:

- `predicate-{opening,older,scale}/comparison.json`: full before/after timings and
  replay hashes, with final pairs retained.
- `predicate-older-recheck-{original,component}/`: five-pair pinned checks and
  affinity metadata.
- `predicate-verify/comparison.json`: separate four-worker/reload checks.
- `predicate-runtime.json`: final source, executable and compiler fingerprints;
  `appraisal-runtime.json` indexes the earlier source snapshots.

The final release executable SHA-256 is
`951aa5ff04e822c6ae3788872fa0078fa5423c1993769783497ae6ed1ca530d6`;
its Rust/manifests source fingerprint is
`617db9061360313dd1a6778f1ea9543cd6867e68d237511b931a5ad2fde41ae8`.

The follow-up [care-evidence study](CARE_EVIDENCE.md) implements and measures a
narrower interruption policy while preserving every ordinary signal observation.
It also records the unsuccessful attempt to preserve routines through
self-completion care prompts. These are independent, opt-in model experiments;
default behavior is checked with complete replay artifacts.
