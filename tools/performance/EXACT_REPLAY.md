# Exact replay optimization and decision-policy tradeoffs

This pass preserves every decision opportunity, provider invocation and committed
simulation effect. Ordinary content keeps routine continuation, review coalescing,
option memory and care-signal filtering disabled.

## Exact implementation

`game-expression::Projection` shares lazy common subexpressions across a person's
capability and disposition queries: upbringing integration, condition recovery,
aging and equivalent maturation curves. Inputs are a complete copied `Life` and
an exact logical clock. The vitality owner's existing bounded, version-checked
frames retain the projection; owner changes, rollback and clock changes invalidate
it. Integer division order, saturation and signed disposition formulas match the
independent reference `Life` methods. No approximate time buckets or floating
point arithmetic are introduced. Pure math stays in its own crate.

`game-information` validates only records changed since a valid transaction
checkpoint. Persistent-tree diffs skip unchanged subtrees, then shared-record
identity skips unchanged neighboring rows. All information invariants are local
to one person's row; deletion cannot invalidate another row. Full validation on
save/load remains. This removes a population-wide scan from a local update,
without removing any invariant. It is an asymptotic improvement for that
operation, not a claim that the whole simulator becomes sublinear.

A freshly trained profile-guided release build uses the existing
`tools/performance/pgo.py` pipeline. It changes compiler inlining, layout and
optimization choices, with no changed simulation rule. Training timings are
excluded. The ordinary release build isolates source changes from compiler
training; the earlier release executable provides the behavior reference.

## Why the earlier large gains changed histories

A decision review is an opportunity to choose among all providers. A notification
named for food care does not restrict that review to food. A pain cue, an action
completion or an unchanged food estimate can indirectly lead to medical help,
supply gathering, inquiry or a voluntary activity switch. Providers also perform
observations. Avoiding their invocation can change both available actions and
what a character knows.

| Experiment | Work avoided | Model consequence |
| --- | --- | --- |
| Routine continuation | Rebuilding alternatives after eligible short actions | Repeats the preceding choice while its relative appeal may have changed. |
| Five-minute review floor | Nonurgent full reviews between deadlines | Responses and idle actors may wait; observation and care opportunities move in time. Urgent paths still interrupt. |
| Option memory | Rebuilding non-survival offer lists | Unchosen motives and opportunities can become stale; provider observations wait until refresh. Execution validation does not establish current attractiveness. |
| Care-evidence filtering | Reviews for unchanged food evidence or non-food cues | Removes incidental opportunities for all activities. A pain cue can lose a medical-help opportunity. There is no five-minute reaction bound. |

Changed action timing changes access to shared goods and other people. Changed
observations affect later decisions; changed scheduling and identity allocation
can amplify divergence. A different history does not, by itself, prove a bug or a
better model. Conversely, the same selected activity on two reviews does not
prove that the intervening work was redundant: motives, observations and other
characters' actions may have changed.

Randomness is counter-based: an unchanged seed, identity and counter produce the
same result regardless of evaluation order. This is not a single global random
stream shifting on every skipped calculation. However, a changed sequence of
scheduled jobs, actions and births can allocate different later identities;
inherited variation, for example, uses the child's identity. Equal starting seeds
therefore do not isolate a policy change from all later stochastic differences.

The latest care-filter study is mixed. Opening crafting batches fell 45 to 26;
older seed-11 crafting rose 29 to 34, while new conceptions changed from one to
zero. In the one-pair 1,000-founder screen survivors rose 977 to 985, while hungry
characters rose 9 to 13 and medical treatment events fell 16 to 7. Those are
material model changes. Fewer treatments may reflect less need or missed care;
fewer crafting batches may reflect lost useful work or less unnecessary work.
The counts alone cannot distinguish these explanations. The complete committed
ledger also records **616 to 646 attacks**, despite deaths falling **18 to 10**;
all these deaths are attributed to damage. Thirteen people die only in the
ordinary run, five only in the filtered run, and five in both. The change alters
who fights, survives and receives care, rather than uniformly improving each
person's outcome. The condensed journal retains every death but samples repeated
events, so attack and treatment counts must come from the complete ledger, not
line counts in that journal. This audit does not establish the individual causal
chain for each changed death. See
[CARE_EVIDENCE.md](CARE_EVIDENCE.md) for the paired results and limitations.

A finite-attention model may be desirable if completing plans and selective
attention are intended behaviors. The current experiments couple attention to
perception and opportunities in several mechanics, so runtime alone is not a
reason to adopt them. Keep them opt-in. Judge a model change using distributions
of unmet-need duration, signal-to-help latency, preventable deaths, useful output
relative to demand, learning and family outcomes over longer, independent seeded
ensembles. Trace individual failures as well as averages. Current short
continuations do not establish century-scale quality.

## Verification and measurement

The replay comparison checks the complete world, ledger, event journal and
individual snapshot. Only JSON object key order is canonicalized for the world;
no state fields are removed. Every build repeats the same histories. Separate
four-worker runs and midpoint reloads check continuation stability. Tests compare
shared math against the independent formulas and incremental validation against
full validation, including invalid direct edits and malformed loads.

The three builds use portable release ThinLTO and one codegen unit. PGO training
uses seeds 17, 29 and 43; evaluation uses seeds 7, 11, 19, 37 and 23. Timed runs
use one simulation worker, ordered listeners, no advisory preparation and
appraisal caching disabled. The comparison rotates build order over three
rounds. Builds, tests and other simulations finish before accepted timing begins.
Fresh helper processes measure CPU and peak RSS. Four-worker/reload checks are
excluded from timing.

## Results against the preceding ordinary release

All five cases passed all complete-artifact comparisons: three builds, three
rotating rounds, plus separate four-worker replays of both new builds. The opening
and three older cases also reload at the midpoint. This establishes equivalence
on these tested continuations; it is not a proof for every possible world.

Times are median seconds. Positive percentages mean less runtime; negative values
mean a regression. These measurements compare the immediately preceding ordinary
release with the new source at identical release settings, then with the new
source's freshly trained PGO executable.

| Workload | Preceding release s | New ordinary s | Source-only reduction | New PGO s | Total reduction vs preceding ordinary |
| --- | ---: | ---: | ---: | ---: | ---: |
| Seed 7, days 10–12 | 5.107 | 5.416 | -6.1% | 4.029 | 21.1% |
| Seed 11, days 300–330 | 2.228 | 2.190 | +1.7% | 1.652 | 25.8% |
| Seed 19, days 300–330 | 26.436 | 23.050 | +12.8% | 15.859 | 40.0% |
| Seed 37, days 300–330 | 16.338 | 15.703 | +3.9% | 12.331 | 24.5% |
| 1,000 founders, seed 23, days 1–2 | 38.477 | 39.319 | -2.2% | 30.898 | 19.7% |

The source-only changes improve the three older continuations by 1.7–12.8% in
these medians, but regress the opening by 6.1% and the large founding case by 2.2%.
They are retained for the requested long-history workload; they are **not a
uniform ordinary-release speedup**. The two source changes are measured together,
so this table does not isolate their individual whole-simulation contributions.
The new PGO executable reduces runtime 19.7–40.0% against the preceding ordinary
release and 21.4–31.2% against the new ordinary release. PGO was already available
before this pass: these figures include its established compiler benefit.

Timings vary substantially. For example, seed 19 baseline runs span 23.238–27.003 s
and new ordinary runs 21.932–25.165 s; the third source-only pair regresses. The
opening ordinary spans 4.974–5.704 s. Interpret the small source differences
cautiously, rather than treating each median as a precise universal gain. The
large-population fixture is a young founding history, not a mature thousand-person
civilization. No century-scale performance claim follows from these windows.

## Local validation scaling

A separate warm-cache benchmark compares the preceding full invariant check with
incremental validation after editing one person's interest in an otherwise valid
snapshot. It includes both interest and inbox tables, one interest per person,
three alternating rounds, and the original checks for topics, capacities and
importance. It runs after compilation and before whole-history timing.

| People with records | Full validation median | Incremental median | Local-operation speedup |
| ---: | ---: | ---: | ---: |
| 100 | 0.735 µs | 0.167 µs | 4.4× |
| 1,000 | 6.573 µs | 0.420 µs | 15.7× |
| 10,000 | 116.128 µs | 0.640 µs | 181.5× |
| 100,000 | 1792.798 µs | 1.186 µs | 1511.8× |

This demonstrates removal of the population-wide scan from one local operation.
It does not multiply whole-simulation throughput by those factors. The standalone
benchmark uses the standard allocator; validation itself performs no allocation.
Its source, executable and raw results are preserved as
`exact-validation-bench.rs`, `exact-validation-bench` and
`exact-validation-results.json` under the optimization artifact directory.

## Comparison with the previous trained executable

A second serial study compares the new trained build with the previously saved
PGO executable (`projection-pgo/game-history`), using three alternating pairs per
case. All five workloads reproduce identical complete world, ledger, journal and
individual artifacts in both builds. These are fresh same-session timings, not
comparisons with historical absolute numbers. This comparison includes all
intervening exact source changes and updated training; it does not isolate only
the two new source changes described above.

| Workload | Previous PGO median s | New PGO median s | Less runtime |
| --- | ---: | ---: | ---: |
| Seed 7, days 10–12 | 4.899 | 4.628 | 5.5% |
| Seed 11, days 300–330 | 2.178 | 2.160 | 0.8% |
| Seed 19, days 300–330 | 17.716 | 16.685 | 5.8% |
| Seed 37, days 300–330 | 12.853 | 12.817 | 0.3% |
| 1,000 founders, seed 23, days 1–2 | 33.050 | 31.352 | 5.1% |

The additional gains over the previous best build are modest, approximately
0–6%. Differences below one percent are within the observed variation. This pass
has **not established another large multiplier over that existing optimized
build**. Its large exact result is the local validation algorithm's scaling; the
whole-history source changes and net compiler-build improvements are smaller.
The 20–40% comparison with ordinary release should not be presented as entirely
new work. The source-only opening and founding regressions remain visible rather
than being hidden by the trained build.

## Larger exact architectural opportunities

The next substantial direction is incremental view maintenance: update pure,
owner-defined query results from the fields that actually changed, as a database
maintains an indexed view. This pass's local validation is a small example with a
measured asymptotic benefit. Candidate generation needs finer dependencies than
the entire world revision or clock; a growing population or an unrelated person's
edit should not invalidate every personal derived value. Exact time-dependent
views can use proved integer transition deadlines, rather than approximate time
buckets. Neither broader caching nor these deadlines are implemented here.

Whole proposal-provider caching needs more work: providers also observe and issue
requests, and request hooks can change state. Preserve those effects at their
original point in the review while extracting the pure query portion. Then reuse
only results whose complete inputs remain equal, with bounded caches and
rollback/load invalidation. The earlier option-memory and review-filter
experiments did not meet that contract. A full review returning the same chosen
activity is insufficient evidence that its computation or observations can be
skipped. This architecture is a stronger exact avenue than suppressing decision
opportunities, but its whole-history gain remains to be measured.

## Artifacts and reproduction

All artifact paths below are relative to `target/history/optimization/`:

- `exact-factors-study/{case}/comparison.json`: three-build timing rounds and
  separate worker/reload verification; `results.json` summarizes medians.
- `exact-factors-vs-previous-pgo/{case}/comparison.json`: fresh old/new PGO pairs.
- `exact-factors-source.tar.gz`: complete Rust/manifests snapshot of this WIP.
- `exact-factors-runtime.json`: source, executables, drivers, reports, CPU policy
  and local validation/audit hashes; `exact-factors-results.json` summarizes both
  timing studies.
- `candidate-exact-factors-release`: ordinary source candidate.
- `exact-factors-pgo/game-history`: new trained executable; `metadata.json`,
  `merged.profdata`, build logs and training runs retain its provenance.
- `exact-factors-bench.py` and `exact-factors-best-bench.py`: the serial study
  coordinators. The latter uses the existing `tools/history/compare.py` driver.
- `care-evidence-outcome-audit.json`: complete-ledger counts, every deceased
  identity and hashes for the earlier behavior-changing screen.

Rust/manifests source SHA-256:
`c55398c794060deab6de6294d6e2aa7a9c18494871481eef9f6136c2707645cf`.
This hashes sorted relative paths, NUL, file bytes, NUL for Cargo.toml/Cargo.lock
and all Rust sources/Cargo manifests under crates/tools (673 files), excluding
performance outputs. PGO's own metadata also records its broader content-inclusive
fingerprint. Ordinary executable SHA-256:
`f9a2c7a8ed289d6a74f2b8c6eb532e0dd6ddb8e6c54164b64727beb398621787`.
New PGO executable SHA-256:
`664e9e330d324fb22a9cc2a07aec46416f98e8dd0152dc32588d0cc4bf0a53f4`.

To build and compare a new candidate, preserve a control executable first, then:

```sh
export RUSTUP_HOME=/workspace/.rustup CARGO_HOME=/workspace/.cargo
export PATH="$CARGO_HOME/bin:$PATH"
cargo +1.97.1 build --release --locked --offline -j 1 -p game-history
# The training plan uses ordinary policies on independent seeds 17, 29 and 43.
CARGO_NET_OFFLINE=true RUSTUP_TOOLCHAIN=1.97.1 LEGACY_APPRAISAL_CACHE=0 \
  python3 tools/performance/pgo.py \
  --training target/history/optimization/exact-factors-pgo-training.json \
  --output target/history/new-exact-pgo --build-jobs 1
# Finish compilation before timing; use a new output directory.
LEGACY_APPRAISAL_CACHE=0 python3 tools/history/compare.py \
  --checkpoint target/history/optimization/warm-prefix --days 330 --pairs 3 \
  --control target/history/optimization/candidate-care-evidence-release \
  --candidate target/history/new-exact-pgo/game-history \
  --output target/history/new-exact-comparison
```

Validation: 22 Rust tests across game-expression, game-vitality and
game-information passed, including shared-factor/reference equivalence,
transaction edits, nested rollback, collisions, other worlds/threads and malformed
loads. Strict Clippy for all targets of those crates passed. Formatting,
architecture/source budgets, whitespace checks and 12 performance-tool Python
tests passed. No source budgets, save schemas or ordinary decision rules changed.
