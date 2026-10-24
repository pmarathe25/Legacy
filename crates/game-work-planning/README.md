# game-work-planning

Personally perceived stock goals, goal ranking and bounded recipe/gathering search.
`game-craft-planning` owns the pure material search; `game-livelihoods` owns scheduled
work, execution, failure learning and achievement. Existing livelihood Goal paths
remain reexports. A cached plan never bypasses action validation or learning.

Results are reused only for identical goals, actor area/seed, expressed intelligence,
personal trait/value revisions, recipe/object catalogs, known patch payloads and
ordered perceived inventory capacities/stocks. Metadata-only refreshes and unrelated
actors cannot invent new supplies. Weak versions retain no world snapshots. The
process cache is bounded by entries and estimated data bytes.

The optional queue-preparation pilot separates `search::capture` from
`search::Inputs::run`. Each worker receives a private read view of patches,
perceived supplies, numerical traits and immutable catalogs; the pure search has
no transaction or dispatch capability. `prepare` skips already cached queries,
searches cold inputs across actors, and retains only the bounded plan and weak
versions. `choose` recaptures and validates all dependencies at its ordinary call
position, after observations and earlier effects; stale preparations recompute.
Preparing a remembered plan never reserves or consumes a physical supply.

With `LEGACY_PROFILE=1`, `work preparation already cached`, `work prepared search`,
`work preparation parallel batch`, `work preparation reused`, and
`work preparation stale` distinguish existing reuse, new speculative work,
parallel opportunities, accepted results, and invalidation. Profiling adds locking
and must be disabled for timing comparisons.
