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
`local_transport.walking.elapsed.unit_millis` gives within-region legs their
own unit. Departure follows an individual's choice. A paused leg retains
its remaining duration, and arrival changes physical reach at the exact
elapsed deadline.
An optional `rules.starvation_rate` uses elapsed exposure after each person's
hunger crosses the starvation threshold. Fractional damage is saved, and the
interval coordinator stops at fatal deadlines before advancing work or travel
farther. Fatal helper withdrawals and family deaths are staged and saved. The elapsed host enforces the fatal deadline.
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
scenario selects this mode. Older elapsed saves may retain boundary settlement.
An environment can set `season_unit_millis` to give the authored calendar
durations a stable elapsed unit. Decisions observe the season at their actual
instant, including transitions inside a physical interval. Seasonal renewal
integrates the growth multiplier across the entire interval and retains
fractions across boundaries. In an interval scenario, `gathering.elapsed_renewal`
credits new stock as time passes, before any withdrawal at the later boundary.
Renewable nodes then require explicit rates and seasonal rates require an elapsed
calendar if seasons are configured. Older saves retain settlement-based renewal.
Gathering still uses boundary actions. Crop scenarios can set
`cultivation.growth_unit_millis` to separate crop duration from settlement.
The host integrates fractional growth up to season changes and ripening
deadlines using the opening moisture and each seed cohort's seasonal traits.
Ripening and descendant batch allocation commit together. The optional
`irrigation.evaporation_period_millis` retains fractional water loss and pauses
growth at the exact drying deadline. Planting, harvest, and actors adding
water still use boundary actions.

`fauna.lifecycle_period_millis` gives wildlife a fixed ecological cadence.
At each deadline the wildlife owner stages age, hunger, finite grazing, climate
damage, mate choice, births, and mortality against the resources available at
that instant. It saves the completed ecological event ordinal, which provides
the random trials' input and is independent of the physical settlement index.
Accepted hunts and offerings affect animals when the physical actions settle.
When using a seasonal calendar with a wildlife clock, configure
`season_unit_millis` to give climate an independent elapsed cadence as well.
The integrated history scenario defines both clocks. The report retains exact
birth and death times from the wildlife owner.
Set `elapsed_spoilage = true` to date newly created physical lots in elapsed
milliseconds. Preparation, travel, or storage transfer retains an existing lot\'s
deadline. Spoilage wakes from an index of active deadlines and removes the due
lots before work or consumption at the same instant, including when a deadline
coincides with settlement. Prepared work dates fresh output on completion and places it directly into
regional custody when capacity permits. The job holds output when the stock is
full.
Older saves without an elapsed custody clock retain their archived boundary
dates.

Prepared recipes can define `matures:`, `maturation-days:`, and Celsius bounds
on their inedible object. Maturation uses the same deadline index. Moving
a preparation preserves its due time. The physical holder determines its
temperature when it is ready. Acceptable conditions transform the actual
cohort into food, and its shelf life begins at that instant.

A scenario can supply `storage_climate` with an `ambient_celsius` temperature
and optional `places` overrides for particular physical holders. Its
`aging_rates` list defines lower Celsius bounds and a positive
`per_thousand` rate at each bound. A rate of 1000 is standard and 500
consumes shelf life at half speed. Food retains exact fractional age
when moved across different thermal rates, including when only part of
a batch moves. The index follows its new projected deadline. Curing
continues on its own clock. To vary exposed storage by season, give every
elapsed season a `storage_celsius` and set the initial ambient temperature
to the first season. Seasonal changes keep the exact accumulated age of
exposed lots. A holder with an explicit temperature is insulated from the
seasonal ambient change. Lots due at the change finish under the closing
conditions. New work output uses the opening season's temperature.

An elapsed environment can set `exposure_period_millis` to schedule personal
hazard damage and shelter rotation at fixed simulated times. The host uses
the season just before each exposure event and applies it before new choices
and ordinary healing at that instant. Legacy exposure still uses settlement.
`dynamics.disease.time_unit_millis` measures case stages and recurring damage
from the exact exposure time. The interval host stops at damage, recovery, and
immunity expiry deadlines. Meetings and other contact opportunities follow
their respective decision schedules.
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
enabled. `family.elapsed_lifetimes.elapsed_gestation` also schedules deliveries
and maternal recovery independently. Delivery events carry exact elapsed times
in the history stream. `conception_period_millis` gives subjective conception
opportunities their own configured cadence. Partnership decisions, most family
history, and succession still use physical settlement.
`care.elapsed.period_millis` schedules visits and neglect at independent elapsed
deadlines. Each visit reevaluates personal preferences, current age, and physical
reach. Food moves through custody when the transfer succeeds. Saves retain the
last visit deadline. Scenarios without this clock retain their settlement cadence.
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
and animal clocks, other ecological processes, or social randomness. In
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
