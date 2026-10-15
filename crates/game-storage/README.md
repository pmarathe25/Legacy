# game-storage

Persistent ordered tables keep transaction snapshots cheap while preserving stable game IDs. This crate owns storage only; features own their records and validation.

`Table<K, V>` stores numeric records in contiguous leaves of at most 64 rows. Branches hold at most 32 child references. Cloning a table shares its immutable root. A write copies the path and affected leaf; splits trim spare allocation capacity, and deletion merges adjacent nodes when they fit. Storage scales with occupied records, including when IDs have large gaps.

`Records<K, V>` uses the same table with individually shared values for sparse, heap-owning records. Editing one person's knowledge copies that person's map, without copying other people's facts. Ordinary per-record collections can remain small standard maps.

`get`, `insert`, `remove`, `entry`, ordered scans, and `changed(previous)` support owner requests and incremental validation. `changed` conservatively includes numeric rows from touched leaves; sparse records filter unchanged shared values. Removed records are absent, so cross-record constraints involving deletion need owner checks. Save loading validates complete resources.

`update` stages disjoint leaves with the host's bounded Rayon pool. Small tables run inline. The callback reads its key and changes its own row. Shared-resource arbitration and structural changes remain outside this numeric phase. A worker panic discards the transaction.

Serialization writes logical records in key order and excludes tree layout. Loading rebuilds the table and rejects duplicate keys. Physical placement and worker completion order never assign identity or event order.

Run `cargo test -p game-storage`. Differential tests compare splits, merges, deletion, snapshots, and pagination against an ordered reference. Other tests verify bounded copies, incremental change detection, worker equality, and panic isolation. See the [performance harness](../../tools/performance/README.md) for the storage comparison and current workload measurements.
