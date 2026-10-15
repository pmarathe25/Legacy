# game-storage

Persistent ordered tables keep transaction snapshots cheap while preserving stable game IDs. This crate owns storage only; features own their records and validation.

`Table<K, V>` stores numeric records in contiguous leaves of at most 64 rows. Branches hold at most 32 child references. Cloning a table shares its immutable root. A write copies the path and affected leaf; splits trim spare allocation capacity, and deletion merges adjacent nodes when they fit. Storage scales with occupied records, including when IDs have large gaps.

`Records<K, V>` uses the same table with individually shared values for sparse, heap-owning records. A mutation copies one sparse value. Replacement and removal return a shared snapshot, avoiding a deep copy when the old value is discarded. Growing personal histories, relationships, and skill profiles use nested persistent tables so an update also stays bounded within one person.

Borrowed-key lookups, `insert`, `remove`, `entry`, ordered ranges, and `changed(previous)` support owner requests and incremental validation. `changed` conservatively includes numeric rows from touched leaves; sparse records filter unchanged shared values. Removed records are absent, so cross-record constraints involving deletion need owner checks. Save loading validates complete resources.

Empty tables allocate nothing. `modify` updates an existing row with one search and creates its default only when absent. `changed_pairs` supplies previous values by merging aligned leaves; structural changes fall back to keyed lookup. Validation avoids searching the previous tree for every unchanged neighbour. Callers of `changed` that only need current values skip old-value pairing entirely.

`version` returns a weak cache token. Writes invalidate it, including writes made without a strong snapshot. It keeps the allocation identity available for comparison without retaining records. Tokens are process-local and excluded from saves and game ordering.

`Agenda<K, T>` stores one deadline per key and derives its time index on load. Due scans visit time and key order; replacing or cancelling a deadline uses bounded table updates.

`update` stages disjoint leaves with the host's bounded Rayon pool. Small tables run inline. The callback reads its key and changes its own row. Shared-resource arbitration and structural changes remain outside this numeric phase. A worker panic discards the transaction.

Range scans seek to their lower bound. Traversal keeps one frame per tree level and allocates nothing for a single leaf.

Serialization writes logical records in key order and excludes tree layout. Loading rebuilds the table and rejects duplicate keys. Physical placement and worker completion order never assign identity or event order.

Run `cargo test -p game-storage`. Differential tests compare splits, merges, deletion, snapshots, and pagination against an ordered reference. Other tests verify bounded copies, incremental change detection, worker equality, and panic isolation. See the [performance harness](../../tools/performance/README.md) for the storage comparison and current workload measurements.
