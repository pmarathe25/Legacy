# Focused attention without idle delays

This study compares behavior changes on top of the behavior-preserving
[memory improvements](MEMORY_THROUGHPUT.md), based on main
`3bb8963d522b87873b4d6e4aa83cbb1f6e8024bb`.

## Model and ownership

The busy-attention model coalesces ordinary interruptions while a character has
an action. It preserves prompt decisions after action completion and while idle.
Perceptions still update on their normal path. Own injury, arrival and urgent
hunger retain immediate review paths. Newly observed or worsening urgent food
or pain evidence also admits an immediate review, using the observer's subjective
perception rather than remote body state. Repeated unchanged cues are not new
emergencies. Physical action durations, output quantities, learning rules and
choice providers are unchanged.

The interval is a minimum gap between full busy reviews, not a polling period.
Short actions can still trigger more frequent full decisions when they complete.
Nonurgent help and voluntary activity switches can wait until the interval ends;
that is an intentional attention limitation. The model does not guarantee the
same long-term history: delayed choices change actions, shared stocks, later
evidence, allocated IDs and subsequent deterministic random draws.

`game-autonomy-policy` owns the saved rule. `game-character-clock` owns admission
and the receiver-owned `Alert` event. `game-care-perception` classifies subjective
evidence; `game-nurture` sends the appropriate decision notification. The history
CLI keeps configuration parsing in its own `attention` module. No alternate world
store or second action scheduler is introduced.

## Rejected variants and corrected confounds

Blanket floors also delayed idle decisions and action completions. Even one
millisecond could materially change dense-world activity. These variants remain
experimental; their large apparent gains alone do not justify adopting them.

The first comparisons exposed two timing problems. Configuring a floor used to
queue an immediate extra review for every character. The controlled driver now
changes only the checkpoint's declared rules and leaves its existing jobs intact;
runtime configuration also preserves unrelated wakes. More seriously, a tiny
deferral across a periodic-review boundary could add an entire extra 12 hours of
idle waiting after grid rounding. Attention now selects the next periodic grid
point. The legacy path is unchanged. Tests cover the boundary, deferred idle
reviews, configuration and worker/reload behavior.

The early `attention-fine-screen` and `cadence-direct-screen` reports are diagnostic,
not adoption evidence. `cadence-future-ensemble` was stopped after finding the grid
bug and is explicitly incomplete. `cadence-corrected-screen` evaluated corrected
blanket floors; `busy-triage-screen` and `busy-triage-ensemble` evaluate the focused
model with urgent care triage. Ordinary controls in both corrected screens match
all four complete legacy artifacts from the direct screen.

## Evaluation method

Every model starts from the same complete checkpoint and advances to the same
fixed end day. Accepted timing runs execute sequentially, without concurrent
builds, tests, profiling or simulations. Binary, drivers and inputs are hashed.
Within-model replay checks compare the complete normalized world, ledger, journal
and people; differing models are expected to differ. Save normalization is the
explicit `canonical-evidence-v2` representation check described in the memory
report, not an outcome-tolerance comparison.

Richness diagnostics use full saved skill tables rather than the UI's top eight
skills. They measure individual participation in goals, conversation, observation
and skill growth; creative forms and creators; retained personal knowledge; and
relationships. Counts from final living cohorts exclude people who died, so
survival and participant counts are reported together. Retained knowledge is not
total learning, and repeated performances are not distinct creative discoveries.
Production completions count batches, not units. No single score establishes
richness or realism.

Five predeclared future seeds (101–105) branch from the same day-300 seed-37
population and past. These are conditional stochastic continuations, not five
independently generated civilizations. The interval candidates were declared
before these runs. All five results are retained, including slower candidates.

| Total over five 30-day continuations | Legacy | Busy 5 minutes | Busy 15 minutes |
| --- | ---: | ---: | ---: |
| Wall seconds | 138.984 | 134.774 | 121.367 |
| Final living people | 77 | 79 | 80 |
| People completing goals | 23 | 22 | 22 |
| People conversing | 60 | 60 | 59 |
| People gaining skill | 49 | 49 | 49 |
| Craft completions | 276 | 316 | 438 |
| Harvest completions | 309 | 280 | 439 |
| Social encounters | 545 | 575 | 616 |
| Inquiry learning events | 63 | 72 | 71 |
| Retained new songs | 193 | 95 | 188 |
| Retained flute performances | 71 | 216 | 188 |

The 15-minute model's combined wall time is 12.7% lower in this ensemble. These
are different histories and one timing per seed/model, not repeated measurements
of identical work. Increased counts do not prove better quality; importantly,
individual participation and the existing creative forms do not collapse in this
mature population. The five-minute candidate mostly shifts songs toward repeated
performance, while delivering only a 3.0% combined time reduction here.

The initial five-fixture screen used one run per model and case:

| Continuation | Legacy seconds | Busy 5 minutes | Busy 15 minutes |
| --- | ---: | ---: | ---: |
| Opening, days 10–12 | 10.756 | 9.626 | 8.934 |
| Older population, days 300–330 | 10.109 | 7.178 | 8.060 |
| Seed 19, days 300–330 | 21.583 | 20.685 | 19.971 |
| Seed 37, days 300–330 | 34.729 | 27.813 | 24.197 |
| 1,000 founders, days 1–2 | 55.537 | 35.277 | 33.993 |

These screening times suggest workload-dependent reductions, not a universal
speed estimate. The dense case initially had fewer craft completions, learning
events and participating people. That concern motivated the longer days 1–3
comparison rather than treating less simulation activity as automatically good.

## Adoption decision

New worlds now use `attention = { ordinary = 900000, evidence = 900000 }` and
`busy_attention = true` in `content/characters/default.toml`. The Rust default and
missing save fields retain legacy behavior. Existing saves keep their stored
rules; opt in explicitly with
`--attention-minutes 15 --evidence-minutes 15 --busy-attention true`.
`--attention-minutes 0 --busy-attention false` restores legacy event-driven review
when goal execution is disabled. Changing rates does not manufacture an extra
initial review of the population.

The longer dense-world comparison (days 1–3) gave the following results:

| Measure | Legacy | Busy 5 minutes | Adopted 15 minutes |
| --- | ---: | ---: | ---: |
| Wall seconds | 147.532 | 100.281 | 94.277 |
| Final living people | 970 | 974 | 973 |
| People completing goals | 828 | 800 | 805 |
| People conversing | 821 | 800 | 799 |
| People gaining skill | 924 | 912 | 912 |
| People actively doing something at the end | 803 | 813 | 798 |
| Distinct produced goods | 72 | 71 | 72 |
| Observed creative forms | 8 | 8 | 8 |
| New retained work creators | 523 | 469 | 474 |
| Goal completions since prefix | 4,663 | 3,676 | 3,892 |
| Craft completions | 3,757 | 3,010 | 3,133 |
| Social encounters | 1,027 | 830 | 856 |
| Inquiry learning events | 1,763 | 1,258 | 1,289 |

The adopted policy took 36.1% less wall time. These are single runs per model at
this extended horizon. Its produced-goods set differs by one member from legacy
(plum planting stock instead of pressed fiberboard); equal cardinality is not an
identical product set. All eight observed creative forms remain, including rare
mask play. The 941 people alive in all three histories retain median skill breadth
of six in every model. Their conversation-growth median is two in legacy and the
adopted model; skill growth is lower (129 versus 105). Final hungry/injured counts
are 15/315 in legacy and 5/289 in the adopted model.

This is **slower activity progression**, not merely unfinished tasks at the cutoff.
For example, actual bread output falls from 1,800 to 1,548 units, flour from 4,464
to 3,792 and medicine from 156 to 105. Only 77 versus 78 final actions have over
an hour remaining, so longer pending jobs do not explain away the difference.
There are fewer retained facts and fewer creators in the dense early world.

I adopt this pacing change because the rule is a plausible limit on interruption,
keeps sensing and urgent responses, and preserves broad participation, creative
forms, skill breadth and useful production without a measured survival collapse.
The mature conditional ensemble also avoids systematic loss of the tested domains.
The evidence does not establish equivalent emergent histories or prove unchanged
quality over centuries. Lower output is an explicit tradeoff, not labeled wasted
work. A preference for maximum early-world activity would favor the legacy model.
Both choices remain available without modifying action providers.

Complete retained states and post-timing quantity/common-cohort diagnostics are
in `busy-triage-scale-long/` and `busy-triage-long-quality.json`. Workers/reload
verification is recorded separately in `busy-triage-verified/` and
`busy-triage-scale-verified/`; those timings are excluded from the tables.
These post-timing correctness runs can overlap tests and one another. Only the
sequential, uncontended screen, ensemble and extended-scale runs enter the timing
tables.

Targeted clock/character/perception/nurture/policy tests pass (94 tests), as do the
complete assembly/history suites after default adoption (41 tests, including
architecture/source budgets). The 50 Python history-tool tests, affected-crate
Clippy with warnings denied, Rust formatting and whitespace checks also pass.
Fresh-world CLI checks verify the saved new default and explicit frame selection
disabling the stored attention floor. Provenance, report checks and validation
logs are summarized by `busy-triage-findings.json`; Python evaluation sources are
frozen in `busy-triage-evaluation.tar.gz`.

## Reproduce

```sh
python3 tools/history/cadence.py \
  --binary FROZEN_GAME_HISTORY --busy-attention --intervals 300000 900000 \
  --cases opening older heldout-19 heldout-37 scale --rounds 1 --retain-state \
  --output NEW_SCREEN_DIRECTORY

python3 tools/history/ensemble.py \
  --binary FROZEN_GAME_HISTORY --busy-attention --intervals 300000 900000 \
  --case heldout-37 --seeds 101 102 103 104 105 \
  --output NEW_ENSEMBLE_DIRECTORY

python3 tools/history/cadence.py \
  --binary FROZEN_GAME_HISTORY --busy-attention --intervals 300000 900000 \
  --cases scale --end-day 3 --rounds 1 --retain-state \
  --output NEW_LONG_SCALE_DIRECTORY

python3 tools/history/verify_attention.py \
  --report COMPLETED_STUDY/comparison.json --models busy-900000ms \
  --workers 4 --output NEW_VERIFICATION_DIRECTORY
```

Verification runs are separate from accepted timing. `--verify` on the cadence
driver can instead verify every model after its sequential timing runs. Complete
state retention is optional because the large-world exports are substantial.

All measured artifacts and frozen engine provenance live under
`target/history/optimization/`, including `busy-triage-runtime.json` and
`busy-triage-source.tar.gz`. The PGO profile is the same transferred profile used
by the exact-behavior control; no policy has received fresh training.
