# Retire obsolete food-expiry appointments

Status: correction implemented and validated: 497 Rust tests, 24 Python history
tests, formatting, and strict Clippy passed before the subsequent feature work.
Native profile-guided release comparisons are complete. Evidence stays under
`target/history/centuries-2026-10-18/`.

The varied world's day-193 checkpoint contains 2,689 physical food batches but
1,345,174 expiry dates and 1,370,606 calendar targets. Consuming or moving food
does not retire its old appointment. Temperature refreshes add rescaled dates
while retaining the previous ones. Equal-expiry batch consolidation would remove
only 140 rows, so it does not address the main excess.

Preservation should retain one callback per live date and remove an inventory/good
target when its last batch at that date disappears. Shared dates must remain while
other food still depends on them. Retiming must replace the old appointment.
The 128-target work limit must continue scheduling any unfinished expiry work.

Track callback handles in preservation. Earlier saves need a one-time rebuild
from actual batches, cancelling the owner's obsolete callbacks. A typed,
owner-checked kernel operation can cancel that request type, including staged
callbacks, through ordinary transactional cancellation. Consolidating duplicated
kernel helpers keeps the existing 1,250-line budget; preservation remains below
its 600-line budget.

Tests must cover partial and complete consumption, transfers, shared dates,
temperature changes, failed operations, legacy saves, cross-worker replay, and
more than 128 simultaneous targets. Kernel tests must cover ownership, staged
work, unrelated callbacks, and rollback.

Keep food quantities and expiry times intact during calendar rebuilding. Preserve
character genes, ages, names, relationships, and the prior journal. Scheduling
allocations can change future identifiers and random trajectories, so this
requires an explicitly labeled model phase rather than an exact optimizer upgrade.
Measure the one-time legacy migration separately from subsequent replays; report
both peak RSS and simulation time before claiming the requested reductions.

## Measured outcome

The dense varied world's legacy load reduced peak RSS by 28.26% and wall time
by 3.36%. Subsequent ten-day replays from a shared cleaned checkpoint reduced
peak RSS from 434,988 to 169,144 KiB (61.12%), while taking 6.38% longer. Smaller
worlds' subsequent thirty-day replays were roughly 3.8–5.6% slower and used
roughly 0.5–1.4% more peak RSS. The broad runtime/memory targets are unmet.

Full per-case evidence is in
`target/history/centuries-2026-10-18/expiry-comparisons/report.json`.
Ongoing histories use explicitly labeled model phases, retain the original
people and journal prefix, and do not establish 200-year or twenty-generation
outcomes yet. This implementation retires obsolete appointments; it is not a
claim of a general speed improvement.

## Final verification

Implementation verified on 2026-10-18: 525 Rust behavior tests passed across
`cargo test --workspace --no-fail-fast`; the documentation build passed after
rebuilding stale Rust artifacts with `cargo test --workspace --doc`. The 24 Python
history tests, 5 Python performance-tool tests, formatting, and strict workspace
Clippy also passed. Source budgets and feature ownership remain enforced.

Physics simulations covered 20,000 networks, 5,000 loaded trials, and 256 finite
apparatus trials, including 52 interruptions and 64 cross-worker replays. They
exposed and verified corrections for overflowing load-capacity probes and ongoing
work retaining a superseded agility rate. Compact evidence is under
`target/history/plans-physics-2026-10-18/`.

The ongoing population studies and broad performance targets remain separate,
incomplete empirical work. These implementation and physical-invariant checks
do not establish 200-year outcomes or twenty-generation selection effects.
