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
