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
physical settlement. Older saves without the
field retain their original quantum-based semantics. Interval snapshots also
retain absolute milliseconds at the current boundary; an older snapshot
derives this value from its original tick and quantum when loaded. This clock
does not by itself convert other owners' tick-based state when a quantum
changes. This does not yet convert
starvation damage, recovery, nutrition, fatigue, work and travel rates, family
and animal clocks, ecological renewal, disease, or social randomness. In
particular, a short physical quantum must not be advertised as a faithful
minute-world until those owners have independent elapsed-time rules.

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
