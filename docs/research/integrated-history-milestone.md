# Integrated hourly history milestone

The existing five-year hourly run was a diagnostic of an earlier revision,
not evidence that the current feature set forms a complete long-lived world.
Use one interval-host scenario, with scarce resources and explicit hourly
rates, to investigate these connected behaviors:

- People seek and consume real food, gather and cultivate, and choose whether
  to work, maintain equipment, cooperate, or pursue other activities.
- Births create individuals with their own inherited inclinations and physical
  capabilities; later generations can acquire food, work, and learn socially.
- Production, tools, seed cohorts, maintenance, resource renewal, and supply
  constraints continue to operate after founder turnover.
- Language, writing, personal knowledge, experiments, relationships, and
  genetic variation arise from individual choices and actual observations.
- History records verifiable individual and population events, bounded active
  event state, and resumable checkpoints. Descriptions of organizations and
  leadership are interpretations of behavior, not simulation authority.

The sequence is a short day-scale diagnostic, one year, several years covering
founder replacement where the scenario actually permits it, and then a
resumable 300-year run. An extinction or a shortage is a result to investigate,
not a reason to suppress scarcity or force reproduction. The current hourly
scenario and old five-year results have not yet established these chains.

`content/scenarios/economy-history-integrated-hourly.toml` is the candidate
world for this milestone. It enables personal choice profiles for founders and
newborns, growth-based work capability, a physical mechanical experiment,
personal writing, and tracked crop genetics alongside the earlier hourly
owners. Those settings are exploratory, not a claim of calibration. Newborn
work-opportunity learning, inherited equipment access, and durable supply after
founder turnover still need verification and further implementation. History
observations are streamed in bounded batches and the inspector processes them
incrementally; a requested individual's collected report can still grow with
the number of events involving that individual.

One-hour ticks do not imply one-minute behavior: a finer-resolution scenario
needs elapsed-time rates and appropriate decision cadences for every enabled
owner. Local walking, goal fetching, campaign succession, missions,
confinement, and broader invention and ecological behavior remain incomplete;
the restoration audit records their current boundaries.

## Minute-stepped comparison

The first two annual checkpoints in `runs/history/integrated-minute-20261120/`
came from 60,000-millisecond advances with one-hour physical settlement. The
baseline treated a reincarnated newborn as manually controlled and skipped its
ordinary food choices even though the history runner had no player. Both
children born in the first year died with severe hunger.

The paired observer run in
`runs/history/integrated-minute-observer-20261120/` uses the same scenario and
advance size. It keeps succession but leaves incarnations autonomous. Both
first-year children survived. By year two the baseline and observer had each
recorded 13 births; the baseline recorded 13 deaths and the observer recorded
3. By year four the observer recorded 18 births, 3 deaths, 75 living people,
and 14 living animals. Twelve early animal deaths were attributed to hunting.
The archive and person reports preserve the observed succession and the
individual outcomes. These checkpoints cover the early world, before founder
turnover. They do not establish long-term stability or minute behavior for
owners that still settle on the physical clock.

The optimized `e202ac4c` scenario enables daily elapsed care and personal
word order. The one-day minute-step probe completed 1,440 advances and
recorded 11 completed jobs, 37 meals, 4 recipe-learning events, and 5
mechanical experiments. The associated 300-year run is in
`runs/history/integrated-minute-current-20261120/`.

## Whole-world scaling sample

The `elapsed_scaling` example measures one live world and checks saved-state
fingerprints across worker counts. It excludes loading and reporting. Each
measurement below is the median of three six-hour runs with one-minute
advances on a shared machine. The 5,100-person fixtures retain the authored
founders and add 5,040 seeded residents. The concentrated fixture places
those added residents in region 1.

| Scenario | 1 worker | 2 workers | 4 workers | 6 workers | 8 workers |
| --- | ---: | ---: | ---: | ---: | ---: |
| 60 founders | 24 ms | 47 ms | 69 ms | 83 ms | 90 ms |
| 5,100 people, two regions | 650 ms | 439 ms | 392 ms | 422 ms | 508 ms |
| 5,100 people, concentrated | 650 ms | 441 ms | 392 ms | 450 ms | 514 ms |

Each fixture produced the same fingerprint and event batch count at all
worker counts. At four workers the larger runs were 1.66 times as fast as one
worker. Similar timings across the two layouts give little evidence that
regional contention explains the gap. Source review identified repeated
deadline discovery, repeated activity construction, and many small owner
batches as candidates for the next comparison.

The next optimized build included the checked-deadline and fatigue changes,
the single elapsed execution path, and the newer wildlife owner. The same
archived reference scenario used its original wildlife configuration.

| Scenario and caller step | 1 worker | 2 workers | 4 workers | 6 workers | 8 workers |
| --- | ---: | ---: | ---: | ---: | ---: |
| 60 founders, minute | 31 ms | 45 ms | 67 ms | 81 ms | 113 ms |
| 5,100 people across two regions, minute | 790 ms | 515 ms | 441 ms | 535 ms | 629 ms |
| 5,100 people concentrated, minute | 623 ms | 491 ms | 546 ms | 503 ms | 610 ms |
| 5,100 people across two regions, hour | 404 ms | 234 ms | 209 ms | 227 ms | 257 ms |
| 5,100 people concentrated, hour | 389 ms | 259 ms | 219 ms | 221 ms | 255 ms |

Each of the larger fixtures produced identical final state and event
fingerprints across both caller step sizes and all sampled worker counts.
The minute-step run took more wall time, especially at higher concurrency.
These measurements came from a shared workstation and are evidence of
remaining overhead. They do not identify a hardware or architectural ceiling.
The archived inputs, raw timings and executable hash are under
`runs/scaling/20261120/`.

### Reused preparation across minute requests

The optimized build at `5e62025a` measured six simulated hours with the same
reference scenario and population fixtures. Each range spans two medians
from batches of three runs. Both modes used the same binary and produced
identical state and ordered event fingerprints at 1, 2, 4, 6, and 8 workers.

| Layout | One worker, replan | One worker, retain | Four workers, replan | Four workers, retain |
| --- | ---: | ---: | ---: | ---: |
| 5,100 people across two regions | 830 ms | 739–823 ms | 625–715 ms | 496–577 ms |
| 5,100 people concentrated | 816–851 ms | 693–707 ms | 631–753 ms | 462–488 ms |

The best four-worker speedups with retained preparation were 1.66 times
across two regions and 1.50 times in the concentrated layout. The samples
show lower wall time with retained preparation and leave the balance between
parallel overhead and genuine contention unresolved. Other histories and
builds were running on the shared workstation.

An independent custody test expired 120,000 lots spread across holders in
121.5 ms at one worker, 67.6 ms at four, and 60.9 ms at eight. The same quantity
in one holder took 1.80 ms at one worker and 1.29 ms at four. The concentrated
path removes the due prefix in bulk. Timings and executable hashes are in
`runs/scaling/20261120/`.


### Observation cost

The instrumented `elapsed_scaling` example supports `LEGACY_BENCH_READ`
values `none`, `person`, and `snapshot`. Turn on owner counters with
`--features profile` and `LEGACY_PROFILE=1`. Row counts describe actual staging
attempts and inline bytes exclude heap allocations.

A one-hour development-build sample with 5,100 concentrated residents
staged 306,000 vitals rows and 306,000 fatigue rows in one-minute advances
under all three observation modes. A single one-hour advance staged 5,100
rows per owner. Each combination produced identical saved state and 14
ordered receipts through all five measured worker counts. The count grows
with the number of caller horizons even when there is no intermediate read.
This diagnostic attributes work and does not estimate production throughput.


## First thirty years

The `e202ac4c` reference run in `runs/history/integrated-minute-current-20261120/`
advanced in one-minute requests with hourly settlement plus independent,
finer deadlines. It began with 60 people and 24 animals. Its year-30 archive
recorded 69 human births and 33 deaths. The human population peaked at 103 in
year 25 and reached 96 in year 30. Wildlife reached 61, including 58 berry
browsers. At that point animal death receipts attributed 12 deaths to hunting
and 14 to old age.

The [history inspector](../../crates/game-economy-cli/README.md#record-a-history)
produced `through-year-30-narrative.md` alongside the archive. It attributed 170
successful load moves and 11,325 stalled experiments to the same person,
recorded one incarnation succession, and counted 275,377 care visits. The
succession follows the authored lineage. Institutional continuity requires
evidence from recurring actions among independent participants. The run was
still advancing when this report was compiled.

## Fifty-year ecological comparison

In the reference run, six grazers were hunted during the first 15 simulated
days. The 12 founders produced three descendants. The nine animals that
survived hunting eventually died of old age, leaving the grazer population
extinct by year 41. By year 50 there were 76 living berry browsers and 88
living people. The archive records 92 human births and 64 human deaths
through that year. The year-50 narrative and ecological counts are saved
alongside the reference archive.

A separate 300-year run in
`runs/history/integrated-minute-elapsed-fauna-20261120/` enables the new
wildlife lifecycle and current scenario choices. It advances in 60,000 ms
requests and records exact animal births and deaths. The launch record
includes the scenario and executable hashes. The original and revised
scenarios differ in several settings, so compare individual causal receipts
before attributing changes to any single mechanism.

The thermal-storage build at `5e62025a`, using the integrated scenario from
`953f9f95`, passed a five-day probe with 7,200 minute advances. At 432,000,000
ms, the archive records 24 units of prepared roots maturing in inventory 1
at 8 °C. The corresponding 300-year history is in
`runs/history/integrated-minute-thermal-20261120/`. A separate CLI smoke
check at `1141aef3` advanced all 111 standalone scenarios by 60,000 ms. The
campaign population file also passed when supplied as an overlay to its
campaign scenario.


## First diagnostic

The initial candidate was run for 8,760 hourly ticks with resumable probe
checkpoints in `runs/history/integrated-20261116/`. The probe's metadata
contains the actual scenario text and source revision for that run; subsequent
scenario edits do not alter those results. At the end there were 61 living
people after two births and one death, and 13 animals remained from the initial
24. The highest recorded hunger was 57, belonging to a newborn. This single
run does not establish why that child was hungry or why animals died.

Work starts stopped after the second day. Inspection revealed that several
founder tools lacked tracked wear conditions, and the configured repairer
eventually ran out of personally carried timber. Regional food stock was empty
at the end, but 5,480 units of barley were in people's custody and 450 in a
store, so empty regional stock alone is not evidence that all food was gone.
Repeated denied writing, repair, and work attempts also merit investigation.
The next candidate tracks the missing tools and lets some people choose to
fetch personal repair supplies. Those are scenario and capability changes,
not evidence that the world is sustainable; compare later diagnostics with
the original recorded scenario.

The revised candidate was run for one year as `supply-v2.ndjson` followed by
`supply-v2-year.ndjson`. In the final 48 hours of its first ten days it recorded
23 accepted work starts and 11 accepted repairs; the corresponding original
interval had no work starts and 16 denied repairs. At tick 2,400, region 1
held 540 tools and one unit of timber, while the other region held 100 timber.
Later samples again show denied work starts. The revised run changed several
scenario settings, so this comparison does not isolate their individual
effects or establish whether workers can obtain the distant timber. Both
probe files were produced before the probe counted early interval work
completions; their `work_finished` totals are incomplete and must not be used
as total production. The checkpoint's physical custody provides independent
evidence of the tools that were actually produced.

The same revised scenario was rerun with the corrected probe as
`supply-v2-counted.ndjson` and `supply-v2-counted-year.ndjson`. Its samples
ending at hours 960 and 1,680 record 341 and 110 completed jobs respectively;
the samples from hour 2,400 onward show no starts and repeated denied work
choices. These are observations from one seed, not a sustainability or
scaling result.

The subsequent `recipe-learning-counted.ndjson` diagnostic enables specific
recipe knowledge for a few founders, learners, and newborns. Over its first
240 hours, four configured learners acquired the tool recipe during actual
meetings; the two configured people who declined learning retained no recipe.
The history probe records four learning events, and the saved state preserves
the specific learners. This does not establish that they obtained tools or
were able to complete that work. `recipe-learning.ndjson` and
`recipe-learning-debug.ndjson` used an earlier incorrect event-tick filter and
underreported these events; their saved knowledge states are unaffected.

The 240-hour `gathering-choice.ndjson` diagnostic adds independent gathering
choices and an optional timber-gathering/deposit path for two people. At hour
240, regional timber stock was 3,086 (initially 3,000), while maximum human
hunger reached 109. This is a different scenario, not a controlled comparison
or evidence of long-term stability. Gathering strength requirements remain
unmodeled, so a young person's willingness is not yet enough to establish
their physical ability to gather.
