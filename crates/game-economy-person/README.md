# Personal economic needs

Stages individual hunger, digestion and nutrient reserves for the economic runtime.

Hunger can accumulate fractional progress with an exact saved remainder. Whole-boundary meals retain any fractional hunger left after relief and clear it when relief exceeds the accumulated amount. This owner primitive does not yet advance needs, decisions, or damage at arbitrary world times.

`progress_hunger` validates the complete batch before applying independent updates in parallel. It does not apply meals, starvation damage or decisions; `settle_elapsed` applies boundary effects without adding hunger again. The caller owns world time, decisions and draft invalidation.

`hunger_threshold_after` computes an individual's exact earliest whole-millisecond threshold from their saved remainder. `next_hunger_threshold` reduces the earliest deadline across people in parallel and validates every record. These are scheduling inputs; they do not themselves publish damage or introduce new world boundaries.

With the remainder present on every person, the mixed campaign benchmark with market, work requests, and local visits took median 392 ms with one worker and 279 ms with six for 10,000 initial people and 12 boundaries in one run; the final fingerprints matched. These timings are workload-specific and show sublinear whole-world scaling.

Meal seekers can choose an actual portion from regional stock or an observed named local store. Store sources are grouped by region and good, so selecting among many stores does not scan every store for every person. Transfers remain subject to physical arbitration.

Directed routes can have authored half-open closure intervals. A valid departure batch resolves physical closures independently, records each traveler's own bounded observations, and leaves already-started journeys unchanged. Observations include their time and are not automatically broadcast. Optional personal routing uses only an individual's reports and their estimated travel times; a route attempt updates whether it is open without replacing that estimate with hidden physical information.

Journeys retain exact fractional progress and can report an arrival deadline. A finished journey can be saved as pending until arrival is published; the traveler remains in transit until settlement. Travelers on the same route are colocated only when their recorded progress matches. These are owner-level facilities; whole-boundary economy advancement still publishes arrivals at a boundary, and partial-boundary world advancement is not yet supported.

`Travel::progress_elapsed` validates a sorted batch before progressing independent journeys in parallel. It leaves incapacitated travelers in place and does not publish arrivals; `settle_elapsed` publishes completed arrivals without adding another interval of travel. The caller coordinates world time and draft invalidation.

The existing `traveling_social_benchmark` accepts `LEGACY_INTERVAL_PROGRESS=1` to time only the travel and hunger counter batches. For 100,000 people, one run took 5.24 ms with one worker and 1.28 ms with six, with matching state fingerprints. This excludes settlement and all other world systems.

The report owner can merge batches of already validated received reports, resolving each person's bounded history in parallel while retaining observation time and immediate source. The ignored `route_report_batch_benchmark` accepts `LEGACY_PEOPLE`, `LEGACY_WORKERS`, and `LEGACY_HOT=1` for a single recipient.

An optional `immortal` physiology on a person specifies a positive `recovery_per_tick`. Their existing body and possessions remain authoritative when health reaches zero; they are incapacitated, recover on later boundaries, and do not accrue hunger, metabolize nutrients, or suffer nutritional deficiency. A route journey pauses during incapacitation and resumes after recovery. They remain susceptible to ordinary injury and can be unable to act. The family lifecycle treats them as outside the mortal cohort and reserves their identity when allocating births.
