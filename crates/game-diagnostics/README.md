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
