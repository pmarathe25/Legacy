# game-packed

`game-packed` provides dense stable-ID component arrays with paged numeric indexes, nested row-undo journals, and parallel bulk update support.

**Why it matters:** Hot simulation components (positions, bodies, minds, property) are read and mutated thousands of times per tick; in-place row mutation with undo journaling avoids tree-cloning overhead while preserving full transactional rollback.

## Responsibilities

- **Dense component storage (`Packed<K, V>`):** Stores values in contiguous physical rows while indexing them by stable `u64`-mapped game IDs.
- **Nested undo journals:** Supports `begin`, `commit`, and `rollback` savepoints that record at most one backup per row per scope.
- **Paged ID index:** Groups IDs into 256-entry pages that adapt between compact sorted pairs (when sparse) and direct slot lookups (when dense), with optional `hash-index` and `compact` feature flags.
- **Bulk parallel updates:** Exposes `update` to process independent physical rows across a bounded Rayon pool and `changed_since` / `changed_pairs_since` for incremental validation.
- **Selected disjoint updates:** `update_selected` rejects duplicate or missing IDs before mutation and journals each selected row. The caller chooses a measured parallel threshold and supplies a row-local callback over immutable inputs; parallel execution order is unspecified. The parallel path scans the physical rows and is not automatically faster for sparse selections. A surrounding owner transaction remains responsible for validation, effects and rollback.

## Boundaries

- **Container only:** Owns physical row layout, index maintenance, and undo logs; owning feature crates define component fields and validation rules.
- **Deterministic external order:** Physical row slots are private to prevent swap-remove order from leaking into gameplay; public iteration and serialization always follow sorted ID order.

## How It Works

- **In-place mutation with journal marks:** When no external reader holds a snapshot clone, writes mutate rows in place and push old values to the active savepoint journal.
- **Nested savepoint retention:** Inner `attempt` commits merge their undo records into the outer transaction scope so a later outer failure still restores the initial state.
- **Lazy copy-on-write on shared readers:** Cloning shares the underlying data revision; only the first write while a reader clone is alive copies the row/index buffers.

## Game Integration

- **Plugin / Entry Point:** Pure library crate; hosted resources wire `Packed` into `game-sim::Resource` checkpoint hooks (`checkpoint`, `commit`, `restore`, `validate_changed`).
- **Requests & Events:** None.
- **Callers:** Used by [`game-sim`](../game-sim/README.md), [`game-soma`](../game-soma/README.md), [`game-characters`](../game-characters/README.md), [`game-transport`](../game-transport/README.md), [`game-property`](../game-property/README.md), and [`game-knowledge`](../game-knowledge/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md).
- **Testing:** `cargo test -p game-packed` (and `--features hash-index,compact`) verifies nested rollbacks, compaction, sparse/gappy IDs, worker panics, and wire equivalence.
