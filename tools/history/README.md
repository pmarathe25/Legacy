# History experiments

Run the ordinary assembled simulation from physical founding conditions, with no player or prescribed society. Histories do not accelerate aging or invent replacement births.

## Bounded studies

Use `python3 tools/history/chunks.py plan.json` for long runs. A plan supplies a frozen executable, its SHA-256, output and archive directories, seeds, and optional physical content overlays. `chunk_years` defaults to 20.

Keep generated plans, frozen runtimes, checkpoints, journals, progress files, and analysis reports under the ignored `target/history/` directory. Set the plan's `artifacts` and job `output` paths there. `docs/` is for maintained design and usage documentation, rather than transient experiment output.

After every chunk the supervisor:

1. Verifies parentage, ancestry depth, and every recorded human birth and death.
2. Compresses and checks the compact event journal and annual summaries.
3. Retains a compressed individual snapshot, cohort CSV, and analysis.
4. Publishes the matching checkpoint/cursor pair, then removes raw logs and the preceding checkpoint.

The latest checkpoint remains available throughout the next chunk. An interrupted chunk can be replayed from its preceding verified checkpoint. Output ownership markers prevent reusing unrelated directories. Stops from resource limits are censoring, never biological extinctions.

Rerun the same plan command to continue after a stop; completed chunks are reused. Remove the archive directory's `stop-request` file first if you requested the stop. The run horizon can be extended, but the generation target and chunk size must remain the same. Executable changes require the verified optimizer upgrade described below. Progress rows show the last verified chunk, rather than live population estimates.

The production plan uses 20-year chunks. `chunk_days` is available for short developer replay tests. `--generations 20` requires twenty actual reproductive steps through the shallower parental branch; years elapsed and surviving guardians do not count.

## Compact journals

Compact logging is the default. It retains all human births and deaths, social milestones, chosen names and personal forms of address, notable objects and losses, and first occurrences of routine experiences. Repeated meals, trades, symptoms, and disputes update totals without writing every occurrence. Ecological records are sampled annually per patch, with up to four newly observed plant genomes. Ungerminated seed and fruit families are counted without individual entries. Witness lists are capped at four; total witness counts remain available.

During a long chunk, `python3 tools/history/progress.py PLAN.json` reports the
latest verified checkpoint and a separate live clock lower bound from the active
journal. It reads at most 64 KiB, ignores unfinished rows, and keeps population
counts and generation depth labeled as verified. A quiet journal can lag behind
the simulation clock; live progress is never checkpoint or ancestry evidence.

For an intentional behavior change, `fork.py SOURCE_PLAN DESTINATION --executable
BINARY --reason TEXT --validation RESULT_JSON` preserves a stopped study's
population, complete journal prefix, and immutable origin checkpoints in a new
experiment. It records the old and new binary hashes and each phase's starting
day and generation. The original study remains available as a control. Optional
`--days N` extends the horizon. The source must have a stop request and release
its supervisor lease first. Equivalent performance upgrades continue to use the
stricter replay checks in `runtimes.py`; model forks explicitly change behavior.

Event counts, production totals, population and climate snapshots, and the latest feeding context at death remain available. Plant samples describe distributions; they are not a complete plant pedigree. Human parentage and demographic evidence remain complete. Private developer records do not become character knowledge.

New cultural transmissions retain `works.shared` with the speaker, listener, work, and medium. Repeated transmission to the same listener does not produce another record. Snapshot `cultural_reach` counts oral memories, text presentations, works with an audience, and the widest audience. These measure recorded exposure; they do not establish comprehension of an entire work or agreement with it.

Use `--journal full` only for a short diagnostic run requiring individual plant-family records and detailed witnesses. Changing logging does not change simulation decisions.

## Standalone runs

```sh
cargo run --profile simulation -p game-history -- \
  --seed 1 --days 7300 --workers 1 --sample-days 365 \
  --checkpoint-days 7300 --output target/history/seed-1
```

Options take separate names and values, for example `--seed 1`. `--content` selects content; repeated `--mod` arguments overlay it. `--resume previous-directory` loads the saved world and matching journal cursor. The saved seed wins. `--final-save false` avoids making a duplicate of the checkpoint; chunked studies use this option automatically.

The checkpoint includes pending work and individual state. `--reload-day` checks a save/load round trip across worker counts. Every committed transaction is collected, including large flowering updates; `--advance-budget` groups work without skipping collection between commits.

`--prepare-window 32` enables the experimental queue-preparation pilot. It prepares
livelihood recipe searches for queued character resumes sharing the first due
job's time and priority, across the configured workers. The bound is 0–128 queued
jobs; 0 is the default and disables preparation. All observations, choices, action
effects, cancellation and commits retain their ordinary order. Prepared plans are
validated again at use. The setting now uses the main `Simulation::advance` API. The report includes
preparation batches, actor count,
maximum batch and advisory failures. Use `LEGACY_PROFILE=1` separately to inspect
owner reuse/invalidation counters; disable profiling for timing. This pilot does
not enable the routine-continuation or five-minute review policies. See
[`game-anticipation`](../../crates/game-anticipation/README.md).

`--parallel-listeners true` restores the earlier per-provider thread fan-out for
comparisons. Independent providers now draft in one batch across ordered barriers,
with conservative resource-read validation and selective retry before application.
Ordered provider callbacks are the default; entity preparation
and large owner operations still use the configured worker pool. Both runtime
settings are reapplied after `--reload-day`.

`--resident-executor true` runs each day's commit loop inside the host worker
pool, letting the coordinator participate in nested preparation without repeated
external handoffs. `--active-executor true` additionally keeps helper workers
polling for work and implies residency. Both are runtime options and default to
false. Active polling can reduce latency while spending much more CPU; compare
wall and CPU time together. Reloads use the newly loaded host's executor.

`--frame-meal-arbitration true` enables the experimental selected-local-meal
cohort policy; it requires `--frame-commit-batch` to be positive and whole-actor
frame decisions. The setting changes actor ordering and can change behavior.
Its arbitration is provisional and bounded by the commit group; food actions
still run through their original owners in sequence. Compare outcomes separately
before choosing it for a history.

`--frame-meal-wave true` enables a separate phased execution experiment and
requires `--frame-meal-arbitration true`. It updates admitted self-meal bodies
as a group and publishes feeding observations after all body and mood effects;
stock and its listeners remain ordered. Hooks, invalidation and failures can
force an ordered retry. This changes observer timing and is not yet validated
for the long history. Body updates use a serial threshold for current cohort
sizes because a small synthetic screen found worker dispatch slower.

`--frame-local-interactions true` selects the in-development local interaction
frame, which groups selected local meals and care. It requires batched frame
decisions and meal arbitration, and cannot be combined with the older meal wave.
It changes interaction ordering and requires performance and replay validation.

`--frame-activity-epochs true` selects the in-development persistent-activity
model. Its first activity batches rest and retains it across recovery epochs,
while urgent events and player control can interrupt it. It changes decision
and rest timing and requires performance and replay validation.

`--frame-mechanics-sessions true` selects the in-development persistent
mechanics session model. It retains an experiment across bounded trials and
batches independent physical calculations, with urgent interruption. It changes
experiment timing and requires performance and replay validation.

`--arrival-window-minutes N` rounds new nonzero journeys up to a shared arrival
boundary of N minutes; zero uses exact arrival times. Passengers retain their
carrier's arrival time, and journeys already in progress keep their deadlines.
This changes travel duration and batches arrivals; compare end-to-end speed and
outcomes before using it for a long history.

`--frame-full-minutes N` enables the experimental two-tier review model. It
requires grouped whole-actor frames and urgent interrupts, and is incompatible
with the nonurgent shortlist cache and meal arbitration. Fresh fast reviews
continue to consider survival, care, conflict, incoming questions and other
unclassified providers; outgoing information and other explicitly designated
discovery wait for a bounded full review. This changes behavior and remains
unvalidated for the long history. Set zero to disable it.

`--frame-nonurgent-minutes N` enables a separate experimental nonurgent offer
cache on whole-actor frames; `0` disables it. The maximum is one day. Food and
body pressures are recomputed, while remembered nonurgent offers can remain
stale for the specified window and until the next frame. Injury and other urgent
signals invalidate the cache; hunger threshold crossing is not independently
scheduled. Compare this behavioral model with a matching baseline and retain
its distinct journal and checkpoint lineage.

Biological years last 365 days; authored climate years currently last 360. Small scenarios retain their controlled fixture clocks. See the [physical profile guide](profiles/README.md) for experimental habitats and their limits.

## Analysis and verification

`python3 tools/history/analyze.py manifest.json output-directory` independently reconstructs human ancestry and reconciles births and deaths. A segment can name a raw directory or a compressed `journal` path, with optional exclusive `after` and inclusive `through` sequence bounds. Chunked studies write the manifest automatically.

`mortality.py` reads compact demographic evidence. `patterns.py` describes recorded social, ecological, and personal-value patterns. Recorded intermediate exchange is not proof that a monetary system formed. Trait/reproductive associations do not separate selection from drift, family preferences, or culture; young cohorts remain censored.

`LEGACY_PROFILE=1` adds inclusive feature timings and failed-plan reasons to summaries. Compare matched conditions when measuring performance. The legacy `study.py` supervisor remains available for older experiment plans; prefer chunked studies for new work.

For diagnostic traces, repeat `--trace-event` with exact event keys (for example,
`--trace-event transport.arrived`). The selected events are added to the journal
without changing simulation or checkpoint ledger state. Trace journals contain
extra records and must be kept separate from ordinary history segments.

`--journey-cooldown-minutes N` enables an experimental delay between completed
voluntary round trips and the next discretionary journey. It changes behavior,
is saved with the world, and does not block direct or survival travel; use zero
to disable it. Compare outcomes and replay separately before adoption.

`--care-refusal-minutes N` enables experimental, bounded memory of a voluntary
food refusal. It permits other known foods and quantities, reacts to newer
observed need and relevant action completion, and schedules a retry at expiry.
It changes behavior, is saved with the world, and defaults to disabled. Use
zero to disable it; evaluate care outcomes, responsiveness and replay separately.

The profile separates history loading, advancement, journal collection, checkpoints, JSON export, and snapshot construction. Feature spans include character review, barter, food planning, observation, physiology, wild growth, and plant genetics. Nested spans overlap; do not add them as independent costs. The summary cannot include the time spent writing itself. Profiling changes no simulation state.

For a performance comparison, freeze both executables and replay the same completed checkpoint:

```sh
python3 tools/history/compare.py \
  --checkpoint target/history/source --days 1095 \
  --control target/history/control --candidate target/history/candidate \
  --pairs 3 --workers 1 --output target/history/comparison
```

The end day is absolute. The driver disables instrumentation, alternates execution order, records wall time, CPU time, and peak RSS, and checks complete world, ledger, journal, and individual-snapshot hashes. World JSON hashes use canonical object field order; arrays and every value remain significant. Ledger, journal, and individual snapshots require exact bytes. Reports declare the hash mode; older byte-hash evidence remains supported. A mismatch retains both outputs and fails the comparison. It retains the last matching pair and small measurements, pruning earlier duplicate worlds. Run comparisons without concurrent builds or simulations. Keep measured results under `target/history/`; usage guidance belongs here.

Run `cargo test -p game-history` and `python3 -m unittest discover -s tools/history/tests -p 'test_*.py'`. Replay checks compare complete worlds and journals across worker counts, advancement batches, and checkpoint continuation.

For sustained studies, `cargo build --release -p game-history` uses the workspace’s thin LTO profile. The `simulation` profile remains useful for faster iteration. Freeze the selected executable before beginning a chunked study; do not change its binary mid-run.

For a study on the build machine, `RUSTFLAGS="-C target-cpu=native" cargo build --release -p game-history --locked` also targets its CPU instructions. The [performance guide](../performance/README.md#profile-guided-history-builds) describes profile-guided builds, including hardware-local builds with `--target-cpu native`. Verify complete replay hashes before adopting either optimizer in a continuing study. GPU calibration in `tools/compute` measures numeric kernels; the history runner executes the assembled simulation on CPU.

For explicitly different decision policies, use a separately identified experiment:

```sh
python3 tools/history/decisions.py \
  --checkpoint target/history/source --days 330 \
  --binary target/history/frozen-game-history \
  --option-minutes 5 --option-limits 16 64 --review-minutes 5 \
  --pairs 3 --verify-workers 4 --reload-day 315 --profile \
  --output target/history/decision-policies
```

This Linux tool forks only the character rules and retains the original journal
prefix. The source must use the ordinary decision policy. It records every input
and executable hash, alternates run order, fixes the horizon even after extinction,
and reports wall/CPU time, peak RSS, checkpoint size, event-count changes and daily
sampled population, hunger, injury and activity. Daily samples do not measure exact
idle or action durations. Complete replay hashes must agree **within each policy**
across repeated runs, worker counts and an optional mid-run reload. Profiling runs
are separate from accepted timing. It retains the last timed world per policy and
prunes verified duplicates. Run timings without other simulations or builds.
Changed-policy evidence cannot approve an equivalent runtime upgrade.

`--review-millis 1` screens one-millisecond review coalescing; positive integer
milliseconds are stored as exact simulation ticks. Reports identify these as
`reviews-1ms` and retain the complete routine policy patch. As with
`--review-minutes`, this also enables the declared narrow routine kinds and their
attention windows. A smaller interval does not establish equivalent outcomes.

`--option-minutes` sets the independent option-memory experiment. Food planning
and action clocks remain ordinary, while other considered offers can be stale
until expiry. `--option-limits` selects retained capacities (default 16, maximum
256); fresh reviews still consider the ordinary full list. Smaller remembered
lists can defer alternatives, and used options are consumed until refresh.
`--review-minutes` sets the broader review floor and the existing
narrow routine policy; characters can wait idle for a review. The `options-5` and
`reviews-5` content overlays select the respective five-minute policies in a new
history. `--mod` does not change rules already embedded in a resumed checkpoint.

## Verified optimizer upgrades

A long study can keep its exact population and journal prefix when applying a behavior-preserving optimizer. Stop through `stop-request` and wait for the supervisor to exit after verifying its current checkpoint. Freeze the new executable and run three alternating replay pairs with `compare.py` against every continuing population’s current checkpoint. All four outputs must match: world, ledger, journal, and individual snapshot.

Set `runtime_upgrade` in the revised JSON plan to `{"from_sha256": "<previous binary hash>", "comparisons": {"<job name>": "<comparison.json path>"}}` alongside the new root, executable, and hash. Paths can be absolute or relative to the new root. The supervisor validates the evidence, checkpoint hashes, and journal cursors; backs up the prior study state; and records both runtimes and evidence hashes. It preserves generations, days, checkpoint, and all archived segments. Earlier chunk ownership markers retain their original hashes. Remove `stop-request` before continuing. Rejected evidence leaves the study unchanged.

The supervisor holds an exclusive process lease to prevent two invocations from writing one study. Optimizer replay evidence is specific to the tested checkpoints; it is not a proof for every possible world. Changed decision policies or biology belong in a separately identified experiment.

Set `chunks_per_invocation` to a positive number to return after that many verified chunks, making room for inspection before continuing. Omit it for continuous supervision. Current studies declare `reproductive_window: {"lifespan_fraction": [2, 3]}` so cohort censoring follows expressed natural lifespan. Legacy manifests without this metadata retain their original fixed age-45 interpretation. This analysis metadata describes the frozen mechanics; it does not alter fertility.

`--distress true` writes a separate diagnostic `distress.jsonl` for urgent hunger. It records up to eight samples per continuous episode, at least two days apart, including current intentions, journeys, carried food, bulky goods, local physical food stocks, and caregiver context. It reads developer state and grants characters no information. Keep this mode to short investigations; normal histories leave it disabled.

A study can set `continue_after_extinction: true` to observe the physical world through its requested horizon after all mortals die. Standalone histories accept `--continue-after-extinction true`. Extinction is still recorded in summaries and mortality evidence; no replacement population is introduced. This option extends ecological observation and does not turn extinct worlds into successful human lineages.

Replay comparison supports `--start-temperature 72 --maximum-temperature 88`
(Celsius). Cooling occurs before timing and is reported separately. Crossing the
maximum aborts the replay, discards its timing and terminates the measured process
group, including descendants of an exited timing wrapper. Affinity and CPU/memory
limits must also be applied externally on thermally constrained machines.

Compact journals retain first maker/design constructions and coarse capacity or
practical-use milestones (`mechanics.built`, `mechanics.tested`), including actual
geometry, mechanism kinds, observed effort, moved load, and task. Repeated trials
remain aggregate counts. Script development and voluntary care changes are also
retained. Research overlays may set `guardians = 0`; all reports must distinguish
mortals, guardians, actual ancestry depth, and years elapsed.

`appraisals.py` measures exact incremental appraisal views with alternating on/off
runs in one frozen executable, optional previous-runtime verification, and separate
four-worker/reload checks. See `tools/performance/REVIEWS.md` for the trace workflow.

To evaluate routine continuation through self-completion care prompts, use
`decisions.py --completion-minutes 5 15` with the usual checkpoint, binary, output,
and end-day arguments. These variants set a fixed routine deadline and
`preserve_on_care_completion = true`, leaving `review_floor` absent. Existing care
intentions and external evidence still interrupt. Delayed discovery of an
unsignalled need and repeated formerly preferred actions make these intentionally
different models; compare outcomes as well as time. Ordinary content is unchanged.

`decisions.py --policies tools/history/policies/care-evidence.json` evaluates
food-evidence filtering independently of routine continuation. The JSON file maps
experiment names to character-rule patches; `ordinary` is reserved for the
unmodified control. Reports record the exact patches and policy-file hash, and
reject edits to that file during the study. Each fork changes only character
rules. Use `--mod tools/history/profiles/care-evidence` to start a new history with
the same opt-in rule. This model still observes signals; it avoids waking for
non-food cues or unchanged food estimates, unless food delivery is in progress.
Reaction to an unsignalled need can wait for the current activity to finish.


`--decision-frame-minutes 1` explicitly enables the experimental phased proposal
model in the main simulation. Ordinary event-driven behavior remains the default;
`0` disables a previously enabled frame policy. A saved policy continues after
reload without resupplying the flag. Physical events keep their timestamps, while
cohort members prepare independent proposals from a shared earlier step snapshot.
Wake notifications coalesce to one review per actor at the next step; reactions
can wait up to the configured interval. Effectful providers and physical execution
remain ordered, so this is a phased decision model rather than a fixed-step
physical simulation. New frame jobs also consume global
identities: outcome differences from ordinary histories include that schedule
change as well as observation and proposal timing.

`--decision-frame-millis 10000` selects a ten-second interval. Resupplying the
saved interval on external resume does not dispatch a redundant configuration
request. Action completion can leave an actor idle until the next boundary, so
shorter steps test a different model as well as a different batching size. Final
selection and effectful providers remain live, as does food planning unless the
separate food-input experiment below is enabled; a late entrant to a
periodic review can use live proposal fallback.
An actor newly involved in an action after cohort preparation defers its obsolete
turn until the next boundary, preserving already resolved joint activities.

`python3 tools/history/frames.py --binary FROZEN_BINARY --output NEW_DIRECTORY
--cases opening heldout-19 scale --rounds 3 --verify-reload` compares one-worker
ordinary behavior, four-worker exact drafting and one/two/four-worker frames.
It rotates configuration order, records wall/CPU/RSS and effective core usage,
checks all four replay artifacts within each model, and reports outcomes between
models. `--control-binary` verifies the original ordinary history;
`--control-frame-binary` separately verifies the preceding frame model. Do not overlap
these timing runs with builds, profiling or other simulations. Frame acceleration
is explicitly behavioral evidence, not an exact runtime upgrade.
Use `--milliseconds 10000` to compare shorter steps, `--resident-executor` for
resident coordination, or `--active-executor` for the dedicated-worker experiment.
See [the concurrency evaluation](../performance/CONCURRENCY.md) for measured
limitations and the distinction between CPU occupancy and useful parallel work.

`--frame-recovery true` enables one next-step review after an idle fallback and
rechecks selected prepared proposals before execution. Proposals that vanished
are skipped without teaching a personal failure; genuine rejected actions still
teach. Recovery is bounded and does not give every idle actor a review every step.
`--frame-food true` separately snapshots food alternatives in the parallel cohort
batch. It changes query timing; live urgency, scoring and execution remain ordinary.
Both settings are saved, and resupplying an unchanged setting on resume avoids a
configuration transaction. They require enabled frames and default to false.

`frames.py --models ordinary frame recovery recovery-food` compares these models
separately across workers. Include `food` to measure food preparation independently.
`--span-days 1` performs a short screen from each saved fixture; it fingerprints
the prefix summary used to choose the horizon. `--discard-verified` removes large
verified worlds after recording complete hashes, outcomes and measurements.

For exact engine upgrades, `attention.py --binary CANDIDATE --reference-binary
FROZEN_CONTROL --models ordinary-reference ordinary --cases heldout-19 heldout-37
scale --rounds 2 --verify --output NEW_DIRECTORY` alternates control/candidate
order across two rounds. Both use the ordinary policy and must reproduce identical
complete worlds, ledgers, journals and people, including worker/reload checks.
Keep both engines frozen and run without builds, profiling or other simulations.

`--frame-decisions true` enables population-wide whole-actor preparation, including
caregiving assessment, food planning, continuation values and final scoring.
Typed sensing updates and physical actions still resolve in order. It snapshots
inputs that the preceding frame model evaluated live, so compare outcomes as well
as time. The setting defaults to false and persists across reload. It requires
enabled decision frames; optional recovery can be enabled separately.
Add `--parallel-listeners true` to share work across independent providers inside
small actor cohorts. Larger cohorts keep whole-actor tasks. This changes task
granularity, while worker counts and execution strategy must preserve results
within the selected model.

`--frame-urgent true` enables the separate interrupt experiment for frame models.
Detected injury, arrival and urgent-care alerts trigger a live review without
waiting for the next frame. Other wakes interrupt when the character is already
above the urgent-hunger threshold; this setting does not add a threshold-crossing
detector. Same-time wakes coalesce, and the affected actor's prepared frame inputs
are invalidated. The option is saved and defaults to false. Compare this behavioral
model separately, including care outcomes, latency, worker counts and reloads.

`--frame-parallel-observations true` enables an experimental phased observation
step. Personal scene-memory transformations prepare in parallel and publish in
stable order, then accessible and carried stores are inspected individually.
Cross-person observation and scheduling order changes. Per-observation hooks
fall back to the ordinary path. This setting is saved, defaults to false and
requires decision frames; measure complete history throughput and outcomes.

`--frame-demand true` enables demand-driven frame timing. Mind deadlines and new
cues request the next useful boundary; empty periods stop polling. The saved
pending identity is checked against the scheduled queue. This can improve quiet
histories, but also changes global identity allocation and therefore may change
random outcomes. See [the throughput evaluation](../performance/THROUGHPUT.md)
for the complete model, quality criteria and measurements.

`--frame-minimum-cohort 12` keeps whole-actor preparation for groups of at least
twelve actors. Smaller groups collect and decide live, avoiding scratch and merge
overhead. Cohort observations and the new-action guard remain. This changes
provider read timing and is a separately saved behavioral setting; the default
zero prepares all groups. The threshold is independent of worker count.
Live small groups use ordinary attention deadlines without a snapshot-recovery
retry. Prepared groups retain the bounded retry. Empty-cache rechecks are skipped.

`--frame-commit-batch 64` shares commits among groups of up to 64 actors. It requires
whole-actor frames and persists in saves; zero restores individual resume jobs.
Actions within a group resolve sequentially through ordinary receiver validation.
Failure rolls back the whole group, and same-time jobs can run between groups.
This changes scheduler identity allocation and resolution ordering. Use
`frames.py --commit-batch 64` for a separate model comparison.

Resident coordination stays on worker zero. Passive helpers return to ordinary
work stealing; active helpers use the separately selectable waiting experiment.

`frames.py --demand-clock --baseline-binary PRODUCTION_BINARY` times the selected
model against a separate frozen ordinary production executable. Reports now
retain prefix-subtracted events and daily sampled activity before pruning large
artifacts. `--end-day` extends a selected fixture to an absolute horizon. A run
that stops before that horizon is rejected.

Use `frames.py --models ordinary recovery decisions recovery-decisions` to compare
these policies separately. `--control-recovery-binary` verifies the preceding
recovery history independently of ordinary and provider-only frame controls.
`--control-decisions-binary` verifies the earlier actor-only preparation runtime
against adaptive nested execution. Decision models in this driver enable parallel
listeners and record that setting per run.

`--attention-minutes 5 --evidence-minutes 1 --goal-execution true` enables the
separate event-driven multi-rate experiment. Ordinary completions coalesce behind
the ordinary floor; evidence can bring the review forward to the evidence floor.
Urgent hunger, injuries and arrivals remain immediate. Successful livelihood steps
may continue their existing plan before broad deliberation is due. Failed or
exhausted goals request a full review immediately. These settings change behavior,
persist across internal/external resume. Goal execution remains disabled by default. They
cannot combine with frame or option-memory policies. Set goal execution false
before disabling attention with `--attention-minutes 0`.

New worlds use a 15-minute ordinary/evidence floor with `busy_attention = true`:
only ordinary interruptions while acting coalesce. Idle decisions, action
completions, own urgent hunger/injury/arrival and newly observed or worsening
urgent care cues stay prompt. Perception still updates normally. Existing saves
retain their stored policy; enable the new model explicitly with
`--attention-minutes 15 --evidence-minutes 15 --busy-attention true`.
Use `--attention-minutes 0 --busy-attention false` for legacy event-driven review
(disable goal execution first if it was enabled). Explicit frame selection
switches off a stored attention floor when no attention override is supplied.
Minute and millisecond flags are both supported; duplicate units for the same
rate are rejected. See [the behavior and performance evaluation](../performance/BUSY_ATTENTION.md).

`cadence.py` forks only declared rules without injecting extra startup reviews.
`ensemble.py` compares predeclared future seeds from the same saved population.
`richness.py` records personal participation, full skill progress and creative
forms alongside ledger events. `verify_attention.py` checks completed studies
across workers and midpoint reloads separately from accepted timing.

`batch.py --plan PLAN.json --output NEW_DIRECTORY --concurrency 4` advances
independent saved histories with bounded process concurrency and one simulation
worker per child. The plan names a frozen `binary` and a `runs` list of
`{ "name": "run-1", "checkpoint": "SAVED_DIRECTORY", "days": 330 }` objects;
paths resolve relative to the plan. Outputs must be new. Failures request
committed-day stops for sibling runs. `--concurrency 1 4 --rounds 2` compares
aggregate throughput, requiring complete per-history replay equality at every
concurrency level. Those group timings intentionally include the declared
concurrent children; run them separately from single-history benchmarks.
See [candidate and compiler throughput](../performance/CANDIDATE_THROUGHPUT.md).

`attention.py` compares fixed-horizon ordinary, multi-rate and frame policies on
unchanged checkpoints. `--memory-index 1` enables the request-local cue index; `ordinary-reference`
always disables it and must match all complete artifacts from `ordinary`; all policy variants
must reproduce their own repeated/cross-worker/reload artifacts. Timings include
loading and export. Run after builds/tests finish, with no overlapping simulation.
