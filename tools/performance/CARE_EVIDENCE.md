# Care evidence and autonomous-history throughput

The new opt-in rule `care_signal_changes_only = true` updates every heard
perception through the ordinary `communication.notice` request, but asks for a
fresh autonomous decision only when that observation contains a new or changed
food hypothesis. Existing food deliveries retain their immediate prompts.

This applies a field-specific invalidation policy: cue, food hypothesis, estimated
need, learned appetite, witnessed feeding and observed area are compared with the
observer's preceding perception. A new timestamp alone does not request a review;
a cue without a food hypothesis does not request one either. Failed observations
retain the old prompt. Own action completions, direct requests, damage, feeding,
arrivals, ordinary review deadlines and every other listener retain their paths.

**This changes the model.** A non-food signal previously created incidental
opportunities for every proposal provider, including medical help after a pain
cue, acquiring supplies, or a voluntary activity switch. Those opportunities can
now wait for the next ordinary decision or another interruption. There is no
five-minute bound. Recorded perceptions and their forecast timestamps still
change normally. Full reviews also produce observations; avoiding a review can
change later knowledge and choices. The ordinary rule is false, omitted from
saves and absent from normal content.

## Implementation and modularity

`game-care-perception` owns the previously existing candidate scope, local need
inference and concern formula, plus the new evidence comparison. It registers no
plugin or resource. `game-nurture` keeps planning, physical delivery and the
notification decision; writes still go through receiver-owned requests.
`game-autonomy-policy` owns the optional autonomous rule. No simulation-kernel or
arithmetic changes are needed. Optional household composition retains the former
no-op behavior, including when the experiments are enabled.

The independent `Policy::preserve_on_care_completion` experiment also remains
opt-in. It allows a retained routine to survive a self-completion care prompt
when no food delivery is active. It does not change external interruptions.

## Measurement

All timed variants use one frozen portable release executable, Rust 1.97.1,
ThinLTO, one codegen unit, one simulation worker, ordered listeners, no advisory
preparation, and appraisal caching disabled. Three alternating pairs are used for
the opening and each 30-day continuation. The 1,000-founder case is a one-pair
screen. Each model also receives a separate four-worker replay, with a mid-run
reload in the multi-day cases, and a separate instrumented profile. These extra
runs are excluded from timing. Builds, tests and other simulations do not overlap
accepted timing. Fresh helper processes measure child CPU and peak RSS.

Inputs fork only the declared character rule. The driver checks complete world,
ledger, event-journal and individual hashes within each model, across repetitions,
worker counts, reloads and profiling. Different models intentionally have
different hashes. Separate old/new-executable comparisons check the disabled
rule on all three primary fixtures; their single-pair timings are verification,
not evidence for a default-mode speedup.

Representative cases are seed 7 day 10–12, seed 11 day 300–330, and held-out seeds
19 and 37 day 300–330. The scale fixture is seed 23 day 1–2: 1,000 founders in 32
areas, not a mature thousand-person civilization. These continuations do not
establish century-scale behavior or the quality of a different model.

## Results

| Workload | Timing pairs | Ordinary wall s | Food-evidence wall s | Speedup | Less wall time |
| --- | ---: | ---: | ---: | ---: | ---: |
| Opening: seed 7, days 10–12 | 3 | 5.132 | 2.234 | 2.30× | 56.5% |
| Older: seed 11, days 300–330 | 3 | 2.226 | 1.598 | 1.39× | 28.2% |
| Held-out seed 19, days 300–330 | 3 | 20.805 | 17.172 | 1.21× | 17.5% |
| Held-out seed 37, days 300–330 | 3 | 15.594 | 11.360 | 1.37× | 27.2% |
| 1,000 founders: seed 23, days 1–2 (screen) | 1 | 51.983 | 49.215 | 1.06× | 5.3% |

The older continuations improve by 17–28%; the opening case improves by 56%.
The one-pair scale result is a screen, not evidence for a general large-population
speedup. Timing is variable: retain the complete per-pair data rather than using
a single run or comparing these numbers to an older build on another occasion.

| Workload | Survivors | Injured | Completed crafting batches | Feeding events | Medical treatment events | New conceptions |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Opening: seed 7, days 10–12 | 88 → 88 | 50 → 49 | 45 → 26 | 260 → 242 | 0 → 0 | 0 → 0 |
| Older: seed 11, days 300–330 | 9 → 9 | 2 → 3 | 29 → 34 | 238 → 234 | 24 → 29 | 1 → 0 |
| Held-out seed 19, days 300–330 | 10 → 10 | 2 → 2 | 30 → 28 | 397 → 364 | 0 → 0 | 1 → 1 |
| Held-out seed 37, days 300–330 | 17 → 17 | 4 → 3 | 38 → 42 | 609 → 621 | 0 → 0 | 1 → 1 |
| 1,000 founders: seed 23, days 1–2 (screen) | 977 → 985 | 247 → 241 | 1554 → 1576 | 1622 → 1618 | 16 → 7 | 72 → 72 |

Arrows are ordinary → food-evidence. Event counts subtract the checkpoint prefix.
A completed crafting batch is a successful `workshop.produced` event: one batch
can produce several items; gathering and agriculture are excluded. These are
activity measures, not equal-output or welfare guarantees. Treatment events do
not by themselves establish successful healing. Matching survivor counts also
do not establish identical lives or future survival.

The opening case loses 42% of completed crafting batches (45→26), despite its
large runtime improvement. The older seed-11 case increases crafting (29→34)
and treatment events (24→29), but changes new conceptions (1→0). The scale
screen changes survival (977→985). These are material history changes. The
filter remains **opt-in**; it is not promoted as a behavior-preserving optimizer.

A subsequent complete-ledger audit of the same scale outputs found 616 → 646
committed attacks while deaths fell 18 → 10, all attributed to damage. Thirteen
people die only in the ordinary continuation, five only in the filtered one, and
five in both. Improved net survival does not mean every person's outcome
improves or that conflict decreases. Repeated events are condensed in the notable
journal; use complete ledger counts for attacks and treatments. The journal
retains every death, and those records were checked against the ledger.
`care-evidence-outcome-audit.json` records these counts and identities beside the
original reports. See [EXACT_REPLAY.md](EXACT_REPLAY.md) for the interpretation of
these model changes and the separate behavior-preserving optimization pass.

| Workload | Ordinary full review attempts | Food-evidence full review attempts | Filtered signal callbacks |
| --- | ---: | ---: | ---: |
| Opening: seed 7, days 10–12 | 11157 | 3912 | 4772 |
| Older: seed 11, days 300–330 | 7243 | 4710 | 2913 |
| Held-out seed 19, days 300–330 | 58427 | 47132 | 5413 |
| Held-out seed 37, days 300–330 | 31187 | 22117 | 8376 |
| 1,000 founders: seed 23, days 1–2 (screen) | 64679 | 63734 | 2383 |

Profiles are separate untimed runs. Counts describe attempts and callbacks, not
proof of committed effects. Fewer direct wake-ups also alter subsequent actions
and broadcasts, so filtered callbacks do not equal the reduction in full reviews.
All five studies reproduced each model’s complete artifact hashes across worker
counts and profiling; the four multi-day cases also reproduced a mid-run reload.
All three disabled-rule comparisons matched the preceding executable.

## Reproduction and artifacts

```sh
export RUSTUP_HOME=/workspace/.rustup CARGO_HOME=/workspace/.cargo
export PATH="$CARGO_HOME/bin:$PATH"
cargo +1.97.1 build --release --locked --offline -j 1 -p game-history
LEGACY_APPRAISAL_CACHE=0 python3 tools/history/decisions.py \
  --binary target/release/game-history \
  --checkpoint target/history/optimization/warm-prefix --days 330 \
  --policies tools/history/policies/care-evidence.json \
  --pairs 3 --reload-day 315 --profile --output target/history/care-evidence-study
```

`--mod tools/history/profiles/care-evidence` enables the rule for a new history.
The policy JSON is for checkpoint experiments. `decisions.py --policies` accepts
named character-rule patches, reserves the ordinary control, and records/checks
the declaration file's hash. It never treats a behavioral fork as an equivalent
runtime upgrade.

All reports live under `target/history/optimization/care-evidence-{case}`;
`care-evidence-default-{case}` contains disabled-rule equivalence checks.
`care-evidence-runtime.json` records source, executable, driver and policy hashes,
compiler, CPU quota and affinity. The frozen executable is
`candidate-care-evidence-release`; Rust sources/manifests and the Python driver
are archived beside it. Source SHA-256 is
`3f89c164e5c8163fd111a05408de21523213d0619d9795baf77351e5e9aeacb3`;
executable SHA-256 is
`94c20c059a09d9a88eb98fdc95fac4a1d15f4b03c10d2c8f3584be0bca4b79ff`.
The source hash covers Cargo.toml/Cargo.lock and Rust sources/Cargo manifests under
crates and tools; input and policy hashes identify runtime data separately.

## Earlier completion-only screen

The independent completion-preservation trial did not produce a large win. Three
pairs at seed 7 gave 5.615 s ordinary, 6.023 s with a 5-minute routine, and 5.747 s
with a 15-minute routine. The 15-minute case recorded only three routine
continuations and 11,160 full reviews versus 11,157 ordinary. At seed 11 the
corresponding medians were 2.710, 2.457 and 2.742 s. These small/noisy differences
did not justify adoption.

The young 1,000-founder screen was stopped after five accepted timing runs,
including one complete ordinary/5-minute/15-minute round. Its first round took
38.981/39.443/41.390 s; the repeated candidate runs took 35.362/41.257 s. Survivor
counts changed 977→987/984. This is an incomplete matrix with no timing or replay
promotion claim. Its report explicitly records the stop and incomplete
verification. Completed opening/older repetitions, four-worker reloads and
profiles matched within each model. Both experimental behaviors stay off by
default; preserving a chosen routine and changing signal relevance are separate
options.

These reports are `completion-{opening,older,scale}/comparison.json`. The frozen
completion executable, source archive and exact reconstructed driver are retained
with `completion-runtime.json`. They describe the earlier source snapshot; current
sources additionally contain the care-perception extraction and optional-owner
fallback correction.

## Validation

Focused character, deliberation, care and new care-perception tests passed.
The evidence tests cover non-food cues, changed food estimates, timestamp updates,
failed observations, rollback and reload. Composed character tests distinguish
filtered signals from real evidence and verify that an existing delivery still
interrupts. A further regression checks both experiments without the optional
household owner. Formatting, strict Clippy on changed owners and their targets,
and the current-source architecture/dependency/budget check passed. Python history
and performance suites passed (53 tests).
