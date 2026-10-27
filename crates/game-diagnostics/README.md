# game-diagnostics

`game-diagnostics` provides optional process-local wall-clock timing spans and failure counters for profiling simulation features.

**Why it matters:** Developers need visibility into per-feature CPU time and rejected-plan reasons during long history runs without altering deterministic simulation state, RNG sequences, or save files.

## Responsibilities

- **Feature timing spans (`span`):** Measures inclusive wall-clock nanoseconds spent in named simulation phases when `LEGACY_PROFILE=1` is set in the environment.
- **Failure counting (`failure`):** Tallies rejected autonomous plans or requests by reason string and logs each distinct reason once.
- **Snapshot reporting (`snapshot`):** Exports accumulated timing and failure totals for benchmark and history summaries.

## Boundaries

- **Strictly non-authoritative:** Owns only process-local atomic counters; never read by gameplay logic, serialized into saves, or included in state hashes.
- **Zero overhead when disabled:** When `LEGACY_PROFILE` is unset, `span` and `failure` short-circuit immediately via a cached flag.

## How It Works

- **RAII span guards:** Entering `span("name")` records `Instant::now()` when profiling is enabled and adds elapsed nanoseconds to a global table on drop.
- **Inclusive accounting:** Nested spans record their full duration independently, so parent and child totals overlap rather than summing to 100%.

## Game Integration

- **Plugin / Entry Point:** Pure utility crate (`span`, `failure`, `enabled`, `snapshot`, `reset`); registers no `game-sim` plugin.
- **Requests & Events:** None.
- **Callers:** Instrumented across hot feature crates (`game-characters`, `game-provisioning`, `game-knowledge`, `game-soma`, `game-botany`, `game-livelihoods`, etc.) and read by `tools/history`.

## Dependencies & Testing

- **Dependencies:** None.
- **Testing:** `cargo test -p game-diagnostics` checks span accumulation and counter resets.

The optional `allocations` feature accepts a process-local monotonic allocation
counter. With `LEGACY_PROFILE=1`, spans report inclusive allocation deltas alongside
time. No simulation resources, decisions, or saves contain this telemetry. Other
threads can contribute to a process-wide counter; use one worker for attribution.

## Local interaction diagnostics

Build `game-history` with `--features interaction-profile`, and set
`LEGACY_PROFILE=1` only for the diagnostic replay. Ordinary builds erase entire
`interaction_profile!` blocks, including their arguments and loops. The profile
feature also checks the environment flag before executing each block.

The small `count(key, amount)` interface adds quantities to the existing
`summary.json` `profile[key].calls` field; counter entries have zero nanoseconds.
The batch macro records actual parallel/serial owner invocations and row counts.
This reports taking the Rayon branch, not measured concurrent CPU occupancy.

`interaction.candidate.*` counts described care/self selections and invalidations;
`interaction.selection.ordered_or_ineligible` counts selections using the ordinary
path. `interaction.admitted.batch_size.*` counts batches after arbitration, with
an additional admitted-person total. `interaction.denied.*` separates stock,
participant and actor contention. `interaction.wave.*` counts attempted and
successful waves and their accepted/refused care/self offers. Errors appear in
the existing `failures` object with the prefix `local interaction wave:`.
`interaction.fallback.*` distinguishes invalidated review, ordered execution and
arbitration denial. Owner counters cover all calls to those batch APIs, including
attempts later rolled back; nutrition-disabled and empty nourishment are explicit.

`interaction.ordered.family.*` splits selected ordinary/ineligible proposals into
bounded stable prefix buckets (including individual information ask/answer/read/
decline families); unknown prefixes use `other`. Separate `ordered.continuation`
and `ordered.new` counters identify continuations. No person or request identities
become metric keys. `interaction.deferred.invalidated` counts selections discarded
after the meal phase and before ordered execution; it includes body, action,
plan, control, urgent and provider changes rather than asserting a single cause.

`interaction.guard.*` identifies the first failed deferred guard (mind, urgency,
control or body) using the same short-circuit order in every build. Changed
actions now use `interaction.deferred.changed_action`; retained-cursor barriers
count `interaction.review.retained_occupied` and `retained_interrupted`.
Typed meal refusals now use `interaction.review.retained_refusal` when resuming
remaining alternatives with the same retry budget.
`interaction.refresh.*` counts the interaction driver's refresh entry points;
`interaction.review.replan_owner_rejection` counts the separate failed-action
replan path for reused older option memory; `retained_owner_rejection` counts
retries of remaining frame alternatives. `interaction.review.prepared`, `live_fresh` and `live_unprepared`
distinguish accepted whole-actor drafts from live collection. These last counters
cover all character reviews, so compare the same model and checkpoint.

With whole-actor batching, only complete decisions are cached, not per-provider
rows. A live collection then increments `frame_missed` for each absent provider
row even if nothing was invalidated. Use review/refresh counts to interpret this
number rather than treating it as a count of lost populated cache entries.

All diagnostics are nontransactional attempt telemetry. A successful nested wave
can still be rolled back by an enclosing transaction. Do not interpret these as
committed journal counts or use an instrumented run as a throughput measurement.
