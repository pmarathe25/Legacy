# Separate attention from execution

The later [focused-attention evaluation](BUSY_ATTENTION.md) adopts a busy-only
15-minute floor for new simulations. Idle decisions, completed actions and urgent
subjective care evidence stay prompt. The blanket floors and goal-continuation
experiment described below remain opt-in. Existing saves retain their own policy.

The behavior-preserving [memory throughput follow-up](MEMORY_THROUGHPUT.md)
removes additional copying, validation and evidence-query work from the ordinary
model. Its comparison also accounts explicitly for more compact save dictionaries.

This experiment keeps physical events on their existing clocks and separates
owner-authorized work continuation from full activity comparison. This original
blanket-floor/goal-continuation experiment is opt-in; the later content default
uses the focused model linked above.
The implementation is integrated with `main` commit
`3bb8963d522b87873b4d6e4aa83cbb1f6e8024bb`, including its sleep/forgetting memory
model and influence arrangements. Older throughput studies used an earlier model
and are not controls for these timings.

## Implemented policy

| Trigger | Response |
| --- | --- |
| Injury, arrival, urgent hunger | Existing immediate full-review path |
| Nonurgent evidence | Coalesce to the evidence floor after the last full review |
| Ordinary completion or planned review | Coalesce to the ordinary floor |
| Successful livelihood step | Owner-authorized next step, if the same goal is selected and broad review is not due |
| Continuation failure or exhaustion | Request a full review immediately |

Floors are minimum intervals between relevant full reviews, not mandatory polling
rates. Both use simulation time; available cores and wall time never select game
rules. Evidence can advance a pending ordinary callback. Configuration cancels and
replaces pending jobs atomically, and unchanged configuration is a no-op on resume.

A livelihood continuation consumes a one-use token for the finished action and
advances the remaining plan. It does not replay the original `Pursue` request,
reinstate consumed steps, or choose a fresh goal. It runs after the producing
transaction finishes, including output deposits. Current stocks, location,
capability and urgent body state are checked. A newer full review, abandonment,
cancellation or disabled execution prevents an obsolete queued callback from
advancing the goal. Full-review timestamps are not reset by continuing a step.

These are behavioral changes: attention can delay nonurgent help and activity
switches, and goal continuation can keep work going when another activity would
have won a fresh comparison. Survival, food, craft and social/cultural activity all
need evaluation. A lower death count alone is not proof of a better simulation.
Existing food/travel owners retain their own execution paths; this new generic
contract currently has a livelihood implementation.

The checkpoint recipes have work durations of 10–240 minutes and five-minute
gathering steps. A five-minute full-review floor therefore offers few opportunities
to continue those steps before deliberation is due. Fifteen-minute ordinary floors
with shorter evidence floors test that distinction explicitly.

## Modular ownership

- `game-attention`: saved coalescing clock and typed rate pair, no resource/plugin.
- `game-autonomy-policy`: personal decision rules; retains public clock reexports.
- `game-character-clock`: wake admission and guarded continuation scheduling.
- `game-choice`: saved execution mode and generic `GoalReady` contract.
- `game-livelihoods`: actual step token, plan advancement and execution validation.
- `game-memory-state`: saved evidence/trace contract, storage and validation hooks.
- `game-memory`: observation, retention, recall and sleep behavior.

Upstream's `game-deliberation` still owns influence arrangements. The personal
policy crate was renamed to `game-autonomy-policy` to preserve both modules when
integrating main. Public character-facing type reexports and wire fields remain.

## Memory batching and the ECS lesson

[Pyro](CONCURRENCY.md#pyro-as-a-storage-reference) was inspected at commit
`5b32b724c4e4656160248164a251a62e29954c5f`; it has not been adopted or benchmarked
as a replacement engine. Its useful bulk-processing principle exposed concrete
work to remove here without abandoning owner transactions.

Main's memory path bounded short-term memory after every entry of an observation.
A scene larger than the 512-entry limit could evict a fact whose key was still
waiting to be recorded, then panic on that key. The initial large-world control
failed in that path. The repaired policy records the whole observation and ranks
crowding once, then removes forgotten association links in one pass. Personal
context is computed once per batch. This is a bug correction, not an
identical-history claim against the broken path.

A request-local cue index is compared with ordered linear lookup under the same
corrected batch semantics. Both must return exactly the first four matching keys
in sorted order. Complete artifacts, rather than counts alone, test equivalence.
`LEGACY_MEMORY_INDEX=0` selects the linear reference; `1` selects the index.

Main's memory resource also lost the prior knowledge owner's packed-row transaction
journaling. Whole-resource snapshots could copy every character's fact-store row
when one character learned something. The revised resource journals fact rows and
retains only the persistent trace-table root. Transaction validation checks changed
rows/books; full validation remains available for the complete resource. Batch
recording acquires the fact row and book once, avoiding per-entry host resource
lookups. These storage changes are intended to preserve the corrected batch policy.

## Reproduce

```sh
python3 tools/history/attention.py \
  --binary FROZEN_GAME_HISTORY --output NEW_RESULT_DIRECTORY \
  --cases heldout-19 scale --rounds 2 --verify
```

The driver uses unchanged input checkpoints and fixed horizons, fingerprints its
inputs/binary/source, records wall/CPU/RSS and prefix-subtracted events, and checks
complete world, ledger, journal and people artifacts. Repeats, worker changes and
reloads must agree within each policy. Histories across attention policies are
expected to differ. Run after builds/tests, without overlapping simulations.

The runtime flags are `--attention-minutes N --evidence-minutes N
--goal-execution true`. Rates require `0 < evidence <= ordinary <= DAY` and cannot
combine with frames, legacy routine rules or option memory. Goal execution
requires a floor. Disable execution before disabling attention. All settings and
pending work survive reload and external resume.

## Initial index screen

The indexed and linear paths produced identical complete artifacts. On seed 19's
first continued day the two-round wall medians were 1.846 s linear and 1.923 s
indexed. On the thousand-founder continuation, one matched day took 179.507 s
linear and 176.318 s indexed. This does not establish a useful index gain; the
index remains opt-in and linear lookup is the default.

The slow large-world control motivated bounding this initial screen after its
first completed pair. `attention-memory-index-screen/comparison.json` retains the
incomplete planned study; `attention-memory-index-subset.json` explicitly describes
only its completed subset. The small case also has cross-worker verification.
This is preliminary timing evidence, not a repeated large-world timing result.
Both use the repaired batch policy, before row journaling/local validation.

## Exact storage comparison

The frozen journaled build reproduces all four complete artifacts from the repaired
snapshot control on both continuations. Two one-worker candidate timings gave:

| Continuation | Snapshot control s | Journal/local-validation median s | Speedup |
| --- | ---: | ---: | ---: |
| Seed 19, days 300–301 | 1.846 (two-run median) | 0.831 | 2.22× |
| 1,000 founders, days 1–2 | 179.507 (one run) | 71.606 | 2.51× |

The large candidate range was 67.772–75.440 s; the control is deliberately only one
run, so treat its speedup as preliminary. Candidate CPU time fell with wall time
(large-world median 71.454 CPU s versus 179.341). Peak RSS was about 338 MiB versus
341 MiB. Four-worker replays also match complete artifacts; their timing is not
included in these one-worker medians. These checks are in
`attention-journal-exact/comparison.json`, with its control subset fingerprint.

This gain comes from packed-row journaling, one acquisition per observation batch,
and validation restricted to actually changed personal books. A table's conservative
changed-leaf report is filtered using each book's shared entry-root identity;
unchanged neighboring books no longer incur complete fact/association validation.
Changed fact rows are included independently, and removed-book checks remain.
No gameplay events, random draws or scheduler identities are added by this storage
change. The default ordinary model benefits without enabling new attention rules.

A separate 30-day exact check uses the same seed-19 checkpoint through day 330.
The snapshot control took 74.737 s and the journaled build 26.616 s (2.81×), with
all four complete artifacts identical. These are one timing each, recorded in
`attention-snapshot-long-exact/` and `attention-policy-screen/`; they are not a
precise repeated speed estimate. They establish equivalence over a substantially
longer autonomous history in addition to the initial day checks.

## Parallelism separated from policy

On the thousand-founder frame policy, one worker took 60.158 s and four workers
34.105 s: 1.76× for the **same policy and complete outputs**. The earlier four-worker
screen took 34.307 s. The matched scaling pair uses no midpoint reload on either
side, so it does not mix reload overhead into the scaling estimate. This remains
one matched pair; CPU cost and full measurements are in `attention-frame-scaling/`.

The separate ordinary-policy screen took 67.058 s. Comparing it with the 34-second
frame result combines a policy change with parallelism. Frame activity counts
were close to ordinary in this founding case, but survivors were 973 versus 987.
Neither the activity counts nor a fast wall time establish equivalent quality.
The selected model therefore remains opt-in. The 17-person seed-37 frame history
showed little benefit in its worker/reload check; the latter includes midpoint
reload costs, so its times are verification rather than a clean scaling estimate.

## Attention-policy screen

The screen below uses the same frozen journaled binary. Attention variants use
one worker; the frame screen uses four. Each cell is one timing, so small timing
differences are not meaningful.

| Policy | Seed 19, days 300–330 s | 1,000 founders, days 1–2 s |
| --- | ---: | ---: |
| Ordinary | 26.616 | 67.058 |
| Ordinary/evidence 5/5 min, no continuation | 20.015 | 22.938 |
| 5/5 min with continuation | 19.815 | 23.641 |
| 5/1 min with continuation | 29.896 | 40.829 |
| 15/5 min with continuation | 15.375 | 20.139 |
| Five-minute demand frames | 11.735 | 34.307 |

Five-minute continuation does not establish an improvement. Work steps generally
outlast that floor, so another full review is already due when they finish. A
shorter evidence floor also is not automatically faster: it changes reaction
ordering and the resulting activity history, not just the number of polls.

The 15/5-minute variant is promising on seed 19: 49 versus 59 crafting batches,
233 versus 257 newly created works, and 95 versus 72 social encounters. Both end
with nine survivors; final hunger is one versus zero. In the founding case,
however, it gives 1002 versus 2085 crafting batches, 349 versus 737 new works, and
279 versus 542 encounters. Survivors improve from 987 to 991, while achieved-goal
and pregnancy totals also change. Lower interaction/activity rates can suppress
both conflict and beneficial activity; extra survivors do not establish a better
simulation.

Fewer full comparisons delay observation, help, voluntary switching and discovery
of opportunities. Continuing a selected plan changes how readily someone leaves
that work for another opportunity. Different action/job sequences also allocate
different identities, affecting seeded random outcomes later. These policy effects
are distinct from the exact storage optimization. Physical completion timestamps
still follow their owner clocks; receiver validation prevents duplicating stock.

No attention or frame policy is promoted to the default. The measured exact
storage improvement benefits ordinary simulation immediately. Additional controls
separate a 15-minute planning floor from the new continuation path, test a second
30-day history, and check complete replay equality across workers/reloads.

### Continuation ablation and second history

With the same 15/5-minute floors, disabling goal continuation took 12.412 s on
seed 19, versus 15.375 s enabled. Crafting was 45 versus 49 batches and new works
216 versus 233. Continuation restored some activity but cost time in this case.
In the founding case the comparison was 22.423 versus 20.139 s, crafting
1045 versus 1002, new works 359 versus 349, and survivors 987 versus 991.
These single-history comparisons are mixed; continuation is an execution
mechanism, not a universal throughput improvement. Changed job/choice histories
also change later seeded outcomes, so aggregate differences are not a microbenchmark
of the continuation callback.

On independent seed 37, days 300–330, ordinary simulation took 44.530–44.549 s.
The 15/5-minute continuation model took 24.365 s, but new works fell from 99 to 24;
crafting rose from 64 to 85 and social encounters fell from 104 to 97. Both ended
with 17 survivors and three hungry characters. One-worker frames took 40.603 s,
with 95 crafting batches, 66 new works and 90 encounters. These models change the
balance of activity, even when survival and completed stock goals look similar.
This is another reason to keep the new policies opt-in.

Ordinary, continuation and frame histories reproduce across workers and midpoint
reloads in the tested small histories. Large-world continuation and frame replays
also reproduce across workers; that one-day fixture has no integer midpoint day
for the driver's reload option. Rust tests separately pause between a completed
producer and its queued continuation, test mode changes/abandonment, reject
cancelled/stale tokens, and verify nested rollback and live-stock failure.

## Validation and retained evidence

Affected Rust tests passed across attention, autonomous policy, characters,
livelihoods, choice, memory, knowledge, information, expression, physiology,
kernel, save compatibility and composed simulation. New memory tests cover
oversized initialization/observation, association bounds, expiration, nested undo,
invalid incremental updates and cross-worker reload. Strict Clippy, formatting,
whitespace and architecture/source-budget checks passed; all 48 history-tool
Python tests passed.

Evidence is retained under `target/history/optimization/`:

- `attention-journal-runtime.json`, `attention-journal-source.tar.gz`,
  `attention-journal-pgo/game-history`: delivered source/runtime/profile provenance
  and frozen executable. Its base main remains `3bb8963` (remote rechecked).
- `attention-memory-runtime.json`, `attention-memory-source.tar.gz`,
  `attention-memory-pgo/game-history`: repaired snapshot control.
- `attention-main-screen/`: the initial failed oversized-scene control, not a
  completed speed comparison; its pre-repair source/runtime are retained separately.
- `attention-journal-exact/`, `attention-snapshot-long-exact/`: storage equality
  and timing controls, including the complete 30-day replay.
- `attention-policy-screen/`, `attention-planning-ablation/`: policy outcomes and
  the control separating longer planning from goal execution.
- `attention-frame-scaling/`, `attention-ordinary-heldout/`,
  `attention-owner-verification/`: same-policy worker/reload checks and second seed.
- `attention-findings.json`, `audit-attention.py`, `attention-audit.log`: derived
  comparisons and assertions over all completed studies, cross-study complete
  hashes, frozen binary/driver fingerprints and the final Rust source fingerprint.
- `attention-*-tests.log`, `attention-*-clippy.log`, `attention-final-python.log`:
  validation logs. The main-sync WIP backup remains in `upstream-sync-20261023/`.

All accepted timings ran without overlapping builds, tests, profiling or other
simulations, under a four-CPU cgroup quota. Timings include load and export; large
world artifacts were pruned only after complete hashes/outcomes were recorded.
PGO was transferred from the previous profile rather than retrained for the new
memory code; both frozen control and candidate use that profile. The initial
index study's deliberately incomplete parent and completed-subset scope remain
explicit. No Pyro dependency or production attention default was added.
