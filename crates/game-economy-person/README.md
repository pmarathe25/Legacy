# Personal economic needs

Stages individual hunger, digestion and nutrient reserves for the economic runtime.

Hunger and opt-in starvation damage can accumulate against explicit elapsed-time rates with exact saved remainders. Meals retain fractional hunger left after relief and clear it when relief exceeds the accumulated amount. Starvation accrues only after hunger crosses the configured threshold; feeding pauses its damage without erasing fractional exposure.

`progress_hunger` validates the complete batch before applying independent updates in parallel. `progress_vitals` stages hunger and starvation from the same opening state; `next_starvation_death` provides the earliest fatal deadline. The interval coordinator owns timing, work interruption, family updates, and draft invalidation. `settle_elapsed` applies remaining boundary effects without adding elapsed hunger or starvation damage twice.

`meal::choose_now` selects carried, unexpired food at the current instant for autonomous hungry people who seek meals. It excludes controlled and host-blocked people; the host stages physical consumption and any affected item owners separately.

`meal::opportunities_now` instead enumerates every carried, unexpired food with a complete portion for an available person who seeks meals. It does not impose a hunger threshold or select a food; the caller may compare these possibilities using the person's own preferences.

`provision::opportunities_now` exposes locally visible regional food portions that fit the person's carrying capacity and their own claim inclinations. It leaves the choice and shared-stock admission to the caller and does not impose a hunger threshold.

`hunger_threshold_after` computes an individual's exact earliest whole-millisecond threshold from their saved remainder. `next_hunger_threshold` reduces the earliest deadline across people in parallel and validates every record. These are scheduling inputs; they do not themselves publish damage or introduce new world boundaries.

With the remainder present on every person, the mixed campaign benchmark with market, work requests, and local visits took median 392 ms with one worker and 279 ms with six for 10,000 initial people and 12 boundaries in one run; the final fingerprints matched. These timings are workload-specific and show sublinear whole-world scaling.

Meal seekers can choose an actual portion from regional stock or an observed named local store. Store sources are grouped by region and good, so selecting among many stores does not scan every store for every person. Transfers remain subject to physical arbitration.

Directed routes can have authored half-open closure intervals. A valid departure batch resolves physical closures independently, records each traveler's own bounded observations, and leaves already-started journeys unchanged. Observations include their time and are not automatically broadcast. Optional personal routing uses only an individual's reports and their estimated travel times; a route attempt updates whether it is open without replacing that estimate with hidden physical information.

Journeys retain exact fractional progress and can report an arrival deadline. `travel_unit_millis` specifies the duration of one route unit independently of the physical boundary. A finished journey can be saved as pending until arrival is published. The traveler remains in transit until publication. Travelers on the same route are colocated only when their recorded progress matches. A closure with `unit_millis` applies from `start * unit_millis` inclusive to `end * unit_millis` exclusive, measured from the simulation epoch. Interval route reports retain the absolute time of the observation. A configured travel unit also measures the time before an individual rechecks a closed route. Older boundary reports keep their original timestamps.

`Travel::progress_elapsed` validates a sorted batch before progressing independent journeys in parallel. It leaves incapacitated travelers in place and does not publish arrivals; `publish_arrivals` publishes completed living travelers without settling deaths, and `settle_elapsed` publishes boundary arrivals and deaths without adding another interval of travel. The caller coordinates world time and draft invalidation.

The existing `traveling_social_benchmark` accepts `LEGACY_INTERVAL_PROGRESS=1` to time only the travel and hunger counter batches. For 100,000 people, one run took 5.24 ms with one worker and 1.28 ms with six, with matching state fingerprints. This excludes settlement and all other world systems.

The report owner can merge batches of already validated received reports, resolving each person's bounded history in parallel while retaining observation time and immediate source. The ignored `route_report_batch_benchmark` accepts `LEGACY_PEOPLE`, `LEGACY_WORKERS`, and `LEGACY_HOT=1` for a single recipient.

An optional `immortal` physiology can specify `recovery_rate = { units = ..., per_millis = ... }`. Its saved remainder preserves fractional recovery, and the interval coordinator schedules the instant an incapacitated person regains physical capability. Their existing body and possessions remain authoritative; a journey pauses while incapacitated and can continue afterward. Recovery itself does not decide whether they rest. Immortals do not accrue hunger, metabolize nutrients, or suffer nutritional deficiency. Older content can still specify boundary-based `recovery_per_tick`.
