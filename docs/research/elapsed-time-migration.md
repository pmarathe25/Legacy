# Elapsed-time migration

The interval host accepts millisecond advances, but its physical quantum still
acts as a rate denominator and settlement cadence for many owners. Splitting an
advance within one quantum is therefore different from changing the quantum.
Minute-sized calls alone do not make all behavior minute-resolution or preserve
behavior when the physical quantum changes.

Separate four concepts: an absolute simulated instant, a caller's advance
horizon, an owner's physical rate or event cadence, and the person's own
decision cadence. Persistent remainders and event deadlines must use stable
elapsed units. Owners should stage independent updates in parallel and resolve
simultaneous shared-resource requests under a stable phase order. Random events
need persistent entity/event occurrence counters and either fixed-time trials
or integrated hazard clocks; rescaling a per-tick probability preserves at most
a distribution, not an identical history under subdivision.

The first implementation slice adds an optional explicit `rules.hunger_rate` with
`units` and `per_millis` to an interval-host scenario. Hunger and its next
threshold then use that duration instead of the physical quantum. The saved
fractional remainder uses the same rate denominator. An optional
`decision_period_millis` separately specifies how often the interval host
opens autonomous decision rounds; it does not determine owner rates or
physical settlement. Renewable gathering nodes can also specify
`renewal_rate = { units = ..., per_millis = ... }`; their fractional renewal
is retained in the node rather than rounded separately at each boundary.
Production can specify `work_rate_period_millis`, a stable denominator for
work rates, fractional progress, and completion deadlines independent of the
physical quantum. Its absence retains the legacy per-boundary interpretation.
The scenario importer uses that same rate period when rounding authored
recipe durations up to whole units of work.
Travel can specify `travel_unit_millis` for its route lengths and saved journey
progress, so travel and arrival deadlines no longer have to use the physical
quantum. A route closure can specify `unit_millis` to measure its start and
end from the simulation epoch. Departures, food searches, and hauling then
check whether it is open at the actual time of the attempt. Route observations
retain the time of the attempt through reporting and save/reload. When travel
has an explicit unit, `food_search.retry_ticks` counts those units, and
elapsed food search reviews and closed-route rechecks use the actual time.
Legacy route reports and older saved review dates retain their original
boundary-based interpretation until replaced. The interval player view shows
the controlled person's remembered route reports and their dates.
An optional `rules.starvation_rate` uses elapsed exposure after each person's
hunger crosses the starvation threshold. Fractional damage is saved, and the
interval coordinator stops at fatal deadlines before advancing work or travel
farther. Fatal helper withdrawals and family deaths are staged and saved. The
older boundary-only economy runner rejects elapsed physiology rates; use the
interval host.
An immortal can also specify `recovery_rate = { units = ..., per_millis = ... }`.
Its fractional recovery is saved and the interval coordinator schedules the
return of physical capability, so work and travel cannot accrue while the
person remains incapacitated. Recovery itself does not decide whether they rest.
Ordinary healing can set `recovery.elapsed.period_millis` to schedule recovery
independently of settlement. Its optional bonus uses the exact time the
person rested since the previous recovery event. A save retains that time even
if the recovery period crosses one or more physical boundaries.
An interval-host scenario may set `fatigue_rate_period_millis` so saved
fatigue fractions use a stable rate period. Interval-host rest and resumption
are individual choices, evaluated from signed personal preferences and current
projected fatigue. Idle people compare rest with their other local options;
workers compare it with a continuation preference. A scenario can also set
`elapsed_fatigue = true` to integrate fatigue at activity transitions and
schedule exhaustion damage at elapsed deadlines. The integrated history
scenario selects this mode. Older interval saves retain boundary settlement,
and the legacy whole-boundary runner still applies automatic rest thresholds.
An environment can set `season_unit_millis` to give the authored calendar
durations a stable elapsed unit. Decisions observe the season at their actual
instant, including transitions inside a physical interval. Seasonal renewal
integrates the growth multiplier across the entire interval and retains
fractions across boundaries. In an interval scenario, `gathering.elapsed_renewal`
credits new stock as time passes, before any withdrawal at the later boundary.
Renewable nodes then require explicit rates and seasonal rates require an elapsed
calendar if seasons are configured. Older saves retain settlement-based renewal.
Gathering and grazing still use boundary actions. Crop scenarios can set
`cultivation.growth_unit_millis` to separate crop duration from settlement.
The host integrates fractional growth up to season changes and ripening
deadlines using the opening moisture and each seed cohort's seasonal traits.
Ripening and descendant batch allocation commit together. Planting, harvest,
and changes to moisture still use boundary actions.
An elapsed environment can set `exposure_period_millis` to schedule personal
hazard damage and shelter rotation at fixed simulated times. The host uses
the season just before each exposure event and applies it before new choices
and ordinary healing at that instant. Legacy exposure still uses settlement.
Optional `nutrition.rules.interval_millis` sets absolute metabolism deadlines
and applies deficiency damage at those times. A meal credits nutrients at
successful consumption in this mode. An explicit
`nutrition.rules.health_effect_delay_millis` schedules food health effects
relative to the actual consumption time. They survive settlement and
save/reload. The integrated history scenario uses both elapsed modes.
`family.elapsed_lifetimes.unit_millis` schedules natural mortality from a
person's inherited lifespan and starting age, or from their actual birth
instant. The saved calendar stops work and travel at the fatal instant.
Scheduled runs emit timestamped incapacitations and identify natural deaths.
Work capability and care use age at the actual instant when the calendar is
enabled. Family history, pregnancies, encounter rolls, and succession still
settle at the physical boundary.
Purpose can set `purpose.rules.elapsed_period_millis`. Deprivation and its
physical consequences then apply at multiples of that period. A physical
settlement applies work outcomes without charging for elapsed time again.
Older saves without these explicit rates retain their original quantum-based
semantics. Interval snapshots also
retain absolute milliseconds at the current boundary; an older snapshot
derives this value from its original tick and quantum when loaded. This clock
does not by itself convert other owners' tick-based state when a quantum
changes. This does not yet convert
all fatigue decision opportunities, family
and animal clocks, other ecological processes, disease, or social randomness. In
particular, a short physical quantum must not be advertised as a faithful
minute-world until those owners have independent elapsed-time rules.

Personal-choice tie breaking, cultivation opportunity ranking, and their
intra-round ordering use the saved absolute decision instant rather than the
physical tick number. This makes those particular random choices stable under
a different quantum when the available opportunities are the same. Other
owners still key random events to ticks and require their own persistent event
clocks before whole-world outcomes can have that property.

Acceptance for each migrated owner includes exact state and event-order
equivalence across one advance and irregular partitions, checkpoint/resume in
the middle of an interval, and worker-count independence. Test threshold and
event crossings, not only final totals. Rate and hazard state must survive
save/load; changing the denominator of an existing nonzero remainder requires
an explicit conversion rather than silently reinterpreting it.

The probe accepts a final step duration in milliseconds. For example,
`cargo run -p game-economy-cli --example history_probe -- content/scenarios/economy-history-integrated-hourly.toml runs/history/minute.ndjson 180 15 1 60000`
advances in one-minute calls and checkpoints every 15 calls. The configured
physical quantum is still one hour, so this is a minute-step diagnostic of a
partially migrated world, not a fully minute-resolution world. A three-hour
run is recorded under `runs/history/integrated-20261116/minute-steps.ndjson`;
its first 15-minute checkpoint retains fractional hunger at 900,000 of
3,600,000 rate units. The probe de-duplicates retained events across steps and
uses absolute simulated time in checkpoint names for explicit step sizes.
