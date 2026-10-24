# Autonomous history throughput

The subsequent [attention/storage evaluation](ATTENTION.md) uses the newer main
memory model. Its timing controls are separate from the historical results below.

The previous whole-actor prototype was not a general runtime improvement. It
exposed more parallel decision work, but added preparation and merge costs. Its
ten-second clock also continued polling through quiet periods. More occupied CPU
cores did not reliably mean a shorter history run.

This pass evaluates actual history completion time against the preceding frozen
production PGO runtime. It separates three changes:

1. Five-minute decision batching reduces repeated deliberation and enlarges actor
   cohorts. Idle recovery and live eligibility rechecks remain enabled.
2. A demand-driven clock schedules only the next known useful boundary. Character
   wakes can bring it forward. Empty periods stop generating frame jobs.
3. A minimum cohort avoids speculative preparation for small decision groups.
   Those actors decide directly; larger cohorts retain parallel preparation.
   This removes scratch construction, stored alternatives and merge work where
   parallelism has little room to repay those costs.
   Snapshot-recovery retries are also skipped on this direct path; its inputs
   were collected live. Prepared cohorts retain their recovery mitigation.

Physical action durations and world update intervals keep their existing rules.
Actors still
observe their surroundings before cohort preparation. Actions resolve in order
through their owning features, with shared-resource validation and protection for
newly started joint activities. A character does not automatically deliberate at
every boundary: existing attention deadlines and new cues determine participation.

## Behavior and quality

Five-minute batching can defer a cue, including care and injury cues, by up to one
interval. It also changes decision opportunities, observation times, and how long
an actor continues an activity before reconsidering it. Demand timing retains the
frame phase across sleep but removes empty scheduler jobs. Those jobs previously
consumed global identities; removing them can change later random outcomes even
when a decision boundary coincides.
The minimum-cohort variant also lets small groups read sequential live provider
inputs rather than storing whole decisions from the boundary snapshot. Cohort
observations and new-action guards remain in place. The threshold is fixed in the
saved policy, so changing worker counts does not change which path is selected.

These are explicit model variants. They are judged using feeding, hunger, injury,
survival, treatment, births/conceptions, crafting, social encounters, learning,
other activities and daily sampled activity. Faster execution caused by a stalled
population would be a regression. Similar production or population totals alone
do not establish equal welfare or long-term quality. Changed histories are not
inherently better or worse; individual adverse outcomes and large changes in
social activity need to remain visible.

The demand clock is modular: `game-decision-clock` handles choice-owned scheduled
work; `game-autonomy-frames` forecasts character deadlines; `game-character-clock`
requests a boundary when a cue arrives. All writes remain receiver-owned.
There is no new simulation-kernel policy or saved plugin.

## Running the model

```sh
GAME_HISTORY --resume CHECKPOINT_DIRECTORY --days END_DAY --output NEW_DIRECTORY \
  --workers 4 --decision-frame-minutes 5 --frame-recovery true \
  --frame-decisions true --frame-demand true --parallel-listeners true \
  --frame-minimum-cohort 12 --frame-commit-batch 0 --frame-food false \
  --resident-executor true
```

The interval, tuning and demand clock persist in saves. Resupplying unchanged
settings does not issue configuration transactions. Worker and executor settings
affect execution strategy and may be changed after reload. Ordinary event-driven
decisions remain available with `--decision-frame-minutes 0` and without frame
tuning flags. Demand timing is separately selectable with `--frame-demand`.
Disabling grouped commits or changing/disabling frames restores outstanding
individual character wake jobs, so an existing grouped-policy save can transition
without leaving those characters asleep.

`frames.py --demand-clock` compares demand frames across workers and mid-run
reloads. `--baseline-binary` times ordinary histories with a separate frozen
production executable. `--end-day` extends the selected cases beyond their usual
horizons. Reports retain daily sampled activity, prefix-subtracted event counts,
final quality measures, complete replay hashes, wall/CPU/RSS and binary hashes.

Builds, tests and profiling must finish before accepted timing runs. The
production build uses portable ThinLTO, one codegen unit and PGO training
on independent seeds 17, 29 and 43. The training plan includes the frame model
and ordinary decisions. That profile is transferred to the final small-cohort
and recovery changes; LLVM discards counters for changed control flow.
`pgo.py --discard-build-cache` retains both executables,
training evidence and profiles while removing only its compilation caches.

The invariant checks cover sleeping, earlier wakes, phase preservation, queue
coalescing, failed-wake rollback, reconfiguration, and cross-worker reload. A
composed simulation test exercises demand frames through ordinary advancement.
Complete history comparisons check determinism within each policy; equality
between different policies is not required.

## Production measurements

The main configuration uses five-minute frames, demand scheduling, whole-actor
preparation, a minimum cohort of twelve, and snapshot recovery only for prepared
groups. Individual resolution jobs remain the default (`commit_batch = 0`).
Small populations use one worker; larger cohorts use four. Changing worker count
must preserve the complete history within this policy.

The control is the preceding one-worker production PGO executable. Both runtimes
use Rust 1.97.1, portable CPU targeting, ThinLTO and one codegen unit. The candidate
uses resident coordination, ordinary worker waiting and no advisory lookahead.
Every sample is a fresh process and includes loading, advancement and final
exports. Builds, tests, profiling and other simulations do not overlap timings.
The CPU quota is four cores. Two rotating timing pairs cover each main case;
the small one-worker case has five additional paired samples. Results below use
that one-worker supplement for the quiet case. Short-run variation remains
visible in the raw reports.

| Workload | Previous production s | Fast mode s | New workers | Less wall time |
| --- | ---: | ---: | ---: | ---: |
| Opening, seed 7, days 10–12 | 4.341 | 3.581 | 4 | 17.5% |
| Quiet history, seed 11, days 300–330 | 1.606 | 1.479 | 1 | 7.9% |
| Busy history, seed 19, days 300–330 | 15.705 | 8.769 | 4 | 44.2% |
| Older history, seed 37, days 300–330 | 11.883 | 10.160 | 4 | 14.5% |
| 1,000-founder world, seed 23, days 1–2 | 30.241 | 19.788 | 4 | 34.6% |

The four-worker quiet samples were noisy and showed no convincing improvement.
Its population is below the parallel-preparation threshold, so a one-worker run
avoids unnecessary parallel overhead. The thousand-founder fixture begins with
995 survivors and is a young world, not a mature thousand-person civilization.

Two longer continuations also reached day 390. These are one-pair screens, with
separate worker/reload verification, rather than repeated timing estimates:

| Workload, days 300–390 | Previous production s | Fast mode s | Speedup |
| --- | ---: | ---: | ---: |
| Seed 11 | 7.135 | 4.574 | 1.56× |
| Seed 19 | 46.322 | 23.441 | 1.98× |

After the configuration-transition repair, two fresh rotating timing pairs also
check the delivered executable (`throughput-repaired-pgo/game-history`) against
the frozen production control. These are separate samples from the tables above:

| Workload | Previous production s | Delivered fast mode s | Speedup |
| --- | ---: | ---: | ---: |
| Busy history, seed 19, days 300–330 | 16.489 | 8.777 | 1.88× |
| 1,000-founder world, seed 23, days 1–2 | 30.201 | 18.333 | 1.65× |

Every complete artifact matches its corresponding primary-study model. The final
fix changes configuration transitions, with no change to these fixed-policy
histories. Both timing rounds are retained; the ordinary samples vary appreciably.

## Outcome checks

These histories differ from the ordinary policy. Completed workshop batches,
feeding events and social encounters below subtract the checkpoint prefix.
One workshop batch can create several items. The mechanical `tested` event also
includes interrupted and assisted labor; it is not a completed-research counter.

| Workload | Survivors | Crafting batches | Feedings | Social encounters |
| --- | ---: | ---: | ---: | ---: |
| Opening | 88 → 88 | 45 → 48 | 260 → 280 | 41 → 42 |
| Seed 11, 30 days | 9 → 9 | 29 → 33 | 238 → 227 | 64 → 55 |
| Seed 19, 30 days | 10 → 10 | 30 → 28 | 397 → 377 | 379 → 189 |
| Seed 37, 30 days | 17 → 17 | 38 → 29 | 609 → 609 | 337 → 181 |
| 1,000 founders, one day | 977 → 977 | 1554 → 1663 | 1622 → 1718 | 464 → 434 |
| Seed 11, 90 days | 9 → 9 | 112 → 68 | 728 → 696 | 249 → 157 |
| Seed 19, 90 days | 10 → 11 | 281 → 95 | 1431 → 1654 | 863 → 485 |

Arrows are ordinary → fast. Feeding and survival continue, but the reduction in
social encounters is substantial in the busy older worlds. Individual lives,
conceptions, injuries and activity choices change. This is a usable experimental
policy, not evidence that every behavioral change improves simulation quality.
It remains opt-in; century-scale population and cultural dynamics are untested.

The large crafting reduction in the 90-day seed-19 run warrants inspection. Its
checkpoint recipe produces ten root seeds per batch. Ordinary play creates 2,010
root seeds, while fast play creates 210. This explains 180 of the 186 fewer
workshop completions. Both start with 320 seeds, have a net noncraft outflow of
ten, and finish with twenty root plots. Ending seed stocks are 2,320 versus 520.
The difference is mostly accumulated seed reserves within this observation
window. Whether the larger reserve helps beyond the measured horizon is unknown.
Food-store counts are nine in both runs; creative-work totals are 156 versus 158.
Raw batch totals alone would overstate the loss of current productive activity.

## Grouped-commit and interval experiments

`--frame-commit-batch 64` optionally shares a transaction among up to 64 actors.
It replaces individual resume jobs with character-owned scheduled resolution
groups. Actions still validate and resolve sequentially. A failure rolls back
the entire group, and other same-time jobs may execute between groups. Group
size is saved and limited to 128, following the existing periodic-review bound.
This changes scheduler identities and resolution visibility, so it is a separate
model rather than an exact optimization of the individual-job policy.

Single-pair five-minute screens gave 30.383 → 17.563 s for the thousand-founder
world and 15.990 → 7.054 s for seed 19's 30-day continuation. The quiet case was
effectively unchanged at 1.644 → 1.641 s. In the large world, crafting was
1554 → 1531, feeding 1622 → 1710, social encounters 464 → 433, inquiry-learning
events 654 → 648, and new deaths 18 → 18. These results do not establish a
universal additional win over individual commits. The option remains available.

A one-minute grouped screen gave 31.640 → 21.832 s in the large world and
15.248 → 12.525 s in seed 19. Large-world social encounters were 464 → 472;
seed-19 encounters were still 379 → 204. Reducing the interval therefore did
not restore the older world's original social history. The earlier fifteen-minute
screen was faster on the busy cases but changed activity more strongly. It used
the preceding prototype with a different minimum-cohort/recovery policy; it is
not an interval-only comparison with the final configuration.

Resident coordination now stays on worker zero. Passive helpers return to normal
stealing instead of busy waiting. This is an execution-strategy change: complete
world, ledger, journal and people hashes match the preceding executable when
grouping is disabled. Separate grouped replays match across workers and reloads.

## Stimulus priorities and multiple rates

A separate screen isolates the existing nonurgent review floor from routine
repetition. Injury, arrival and urgent-hunger wake paths can advance a deferred
review; ordinary completion/evidence wakes coalesce behind a five-minute floor.
These still launch full reviews. This is a different policy from the frame model,
which delays all its participating actors to frame boundaries.

The prototype changes only the checkpoint's character rules. Its routine-kind
sentinel `__rate_limit_only__` matches no authored action, so no cached action is
repeated. Both the ordinary and priority policies use one worker; the existing
fast frame policy uses four. All use the same frozen delivered PGO executable,
resident coordination and unchanged physical action/update timings. Timings
include loading and final exports, with no overlapping simulations or builds.

| Workload | Ordinary s | Priority floor s | Fast frames s |
| --- | ---: | ---: | ---: |
| Seed 19, days 300–330, two rotating rounds | 17.742 | 8.449 | 7.950 |
| 1,000 founders, days 1–2, one-round screen | 29.784 | 9.191 | 15.469 |

This establishes a useful throughput/responsiveness tradeoff, not a quality win.
The urgent bypass applies to detected wake conditions; these measurements do not
measure physical-event-to-response latency or make other characters aware of an
unperceived event. Most care/new-information cues still use the ordinary floor.

| Outcome | Seed 19 ordinary → priority | Large world ordinary → priority |
| --- | ---: | ---: |
| Survivors | 10 → 10 | 977 → 990 |
| Final hungry characters | 0 → 1 | 9 → 3 |
| Crafting batches | 30 → 27 | 1554 → 615 |
| Feedings | 397 → 381 | 1622 → 1620 |
| Social encounters | 379 → 152 | 464 → 156 |
| Creative-work total at end | 155 → 155 | 1111 → 788 |

Event counts subtract the input checkpoint. Creative-work totals include the
prefix. Survival alone cannot establish quality; changed event ordering and random
identity allocation also change lives. The one-day large-world sample is too
short to assess population or cultural dynamics. Lower output cannot be assumed
to be harmless stockpiling without a separate audit.

### Separate execution from planning

The next architectural step needs distinct units of work, not just more priority
levels around the same full-review function. Candidate rates below are starting
points for experiments, not implemented defaults:

| Computation | Trigger or proposed maximum delay |
| --- | --- |
| Perception and urgent local response | On relevant perceived evidence; immediate for danger/critical need |
| Continue the selected work/food/travel goal | At its natural step-completion time, with current validation |
| Local opportunity checks, including social opportunities | Relevant changes; otherwise 30–60 seconds when attentive |
| Broad activity/goal comparison | 5–15 minutes, advanced when the current goal fails or ends |
| Long-horizon planning | 15–60 minutes or a material change |

Fast responses should evaluate the relevant owners rather than every provider.
Slower providers retain their intent and dirty state, with bounded deadlines so
frequent urgent work cannot starve social or background decisions. Physical
completion, consumption and resource arbitration keep their own clocks.

The work-continuation refinement is **not enabled by this screen**. Inspection
found that the existing retained-routine helper admits generic `choice.undertake`
requests, while livelihoods selects a mutable, multi-step plan. Replaying the
original selection would reinstate its old steps. The work owner should instead
expose a typed continuation of its current goal, matched to that goal/action's
identity and validated against present stocks, location and capability. The
character scheduler can invoke that continuation while leaving full planning on
its slower deadline. Urgent needs must still be able to interrupt it.

Owners should classify their stimuli and validate their own continuation; the
scheduler handles coalescing, deadlines and fair service. Due nonurgent cohorts
can prepare in parallel, while sparse urgent responses take a direct path.
Rates and ordering must depend on simulation state/time, not available CPU time,
so changing hardware or worker count preserves the selected policy's history.

Evidence is in `target/history/optimization/multirate-screen/`, with its driver
`multirate-screen.py` and derived `multirate-findings.json`. Ordinary and frame
controls match the preceding complete histories. Priority and frame replays agree
after worker changes and, for seed 19, a day-315 reload. Fourteen existing routine
tests passed, including injury replacing a deferred callback, urgent hunger,
coalescing, continuation invalidation and cross-worker checkpoint continuation.
No production policy default or runtime code changed in this screen.

## Evidence and reproducibility

All evidence is under `target/history/optimization/`:

- `throughput-final-production/`, `throughput-final-small-workers/`: final timing
  pairs, complete hashes, fixed horizons and quality measurements.
- `throughput-final-long/`: 90-day continuations and mid-run worker/reload checks.
- `throughput-final-grouped-scale-check/`, `throughput-final-resume/`: large-world
  one/four-worker equality and external continuation with unchanged policy flags.
- `commit-cohorts-zero-control/`: preceding/current executable equality within
  the existing frame model, including the stable coordinator.
- `commit-cohorts-{64,60}-pilot/`: separate grouped-policy and interval screens.
- `throughput-findings.json`, `audit-throughput.py`: derived tables and checks
  against the preceding build, including the seed-stock audit.
- `commit-cohorts-runtime.json`, `commit-cohorts-source.tar.gz`: source, runtime,
  profile and driver provenance. The frozen executable is
  `commit-cohorts-pgo/game-history`.
- `throughput-repaired-runtime.json`, `throughput-repaired-source.tar.gz`: the
  subsequent configuration-transition repair and its source provenance. The
  runnable build is `throughput-repaired-pgo/game-history`. Timings above belong
  to the preceding frozen executable; separate fixed-policy replay checks verify
  the repaired build against it in
  `throughput-repaired-{individual,grouped}-replay/`.
- `throughput-repaired-production-check/`: two fresh timing pairs on the delivered
  executable, plus complete artifact equality against the primary studies.

With grouped commits disabled, the measured executable reproduces all four complete
artifacts from the preceding fast-policy build on all five main histories and both
longer runs.
The ordinary policy also reproduces the preceding production histories. Within
each model, all repeated, cross-worker, mid-run-reload and external-resume checks
agree. Large generated worlds were pruned only after recording and checking the
complete artifacts; hashes and outcome records remain.

Validation passed 105 Rust tests across the affected feature, composed-simulation
and history suites; 48 history-tool and 12 performance-tool Python tests; strict
Clippy; formatting, whitespace and architecture/source-budget checks. Rollback
tests cover failed actor groups, and continuation tests pause before and between
resolution groups with different worker counts.
The configuration-transition test checks both restoring individual commits and
disabling frames with existing pending wakes.
