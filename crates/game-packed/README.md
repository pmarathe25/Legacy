# game-packed

Dense component rows use stable numeric IDs, direct indexed reads, and nested undo journals. Gameplay and validation remain in the owning feature. The crate has no feature or application dependencies.

`begin`, `commit`, and `rollback` delimit strictly nested savepoints. A row is backed up once per savepoint. Successful nested work retains its undo until the outer transaction commits. Insertions, compacting removals, and bulk updates restore both values and ID indexes. `erase` avoids cloning a removed value solely to return it.

`update` runs independent numeric work over physical rows using the caller's bounded Rayon pool. A bulk savepoint keeps an old row buffer; the default reuses that buffer after commit. `changed_since` and `changed_pairs_since` let the owner validate touched rows before committing.

Physical slots are private. `iter`, `keys`, and serialization use stable numeric ID order. `rows` exposes physical traversal only for independent work; collect and sort IDs before applying ordered effects. Loading rebuilds the index and rejects duplicate IDs.

The default index groups IDs into 256-entry pages. Sparse pages store sorted pairs; occupied pages use direct slots. Hysteresis avoids repeated conversion during churn. Storage scales with occupied pages, including IDs near `u64::MAX`; it never allocates by the maximum ID. A `Key` must map uniquely and in order to a number.

The `hash-index` feature substitutes a per-row hash index plus ordered keys. It is useful for comparing extremely sparse ID distributions. The `compact` feature uses smaller capacity increments and releases bulk undo buffers. These policies affect layout only; saves and gameplay are identical. See the [measured tradeoffs](../../tools/performance/ARCHITECTURE.md).

Cloning shares an immutable data revision and starts an empty journal. A retained reader stays unchanged, including across removal, rollback, and loading. The first subsequent write copies the complete shared row/index revision. Without retained readers, writes mutate unique rows in place. Thus frequent complete-world snapshots can trade away much of the benefit; ordinary borrowed reads and small public observation DTOs do not create those snapshots.

Hosted resources must implement the transaction checkpoint hooks: capture a journal mark and non-row metadata, validate changes, restore both on failure, and commit the mark on success. The generic host's default resource checkpoint clones the resource; that remains correct for a packed resource but forces full copy-on-write and loses the fast path. Storage commit/restore hooks must perform infallible housekeeping and must not run gameplay.

Run `cargo test -p game-packed` and `cargo test -p game-packed --features hash-index,compact`. Differential tests cover nested successful/failed scopes, physical compaction, bulk worker panics, gappy IDs, retained readers, and wire equality with an ordered reference.
