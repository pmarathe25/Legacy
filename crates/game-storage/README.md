# game-storage

`game-storage` provides persistent copy-on-write B-tree tables, sparse shared records, and deadline agendas for simulation resources.

**Why it matters:** Transactional rollback and save validation need cheap snapshots, incremental change detection, and deterministic key-ordered iteration even when entity IDs have large gaps.

## Responsibilities

- **Dense persistent tables (`Table<K, V>`):** Stores small numeric records in contiguous leaves (up to 64 rows) and branches (up to 32 children) with copy-on-write path cloning.
- **Sparse shared records (`Records<K, V>`):** Wraps heap-owning per-entity records (such as histories or relationship maps) in individually reference-counted pointers so mutating one entity copies only that entity's value.
- **Deadline queues (`Agenda<K, T>`):** Tracks one deadline per key and derives an ordered `(time, key)` index for due-work scans.
- **Incremental diffs & cache tokens:** Exposes `changed(previous)`, `changed_pairs(previous)`, and process-local `version()` tokens for fast post-transaction validation.

## Boundaries

- **Storage mechanics only:** Owns tree layout, path copying, and parallel leaf staging; feature crates own record schemas and gameplay invariants.
- **Cold/sparse counterpart to `game-packed`:** `game-storage` handles persistent trees and sparse records, whereas [`game-packed`](../game-packed/README.md) handles hot dense component arrays with undo journals.

## How It Works

- **Copy-on-write snapshots:** Cloning a `Table` or `Records` shares its root pointer in $O(1)$ time; subsequent mutations clone only the touched branch path and leaf.
- **Aligned diff scans:** `changed` and `changed_pairs` skip identical shared subtrees by pointer identity, visiting only leaves modified since the checkpoint.
- **Parallel leaf updates:** `Table::update` stages disjoint leaves across a bounded Rayon thread pool and aborts the transaction if any worker panics.
- **Canonical serialization:** Saves emit logical `(key, value)` pairs in sorted key order, never exposing internal tree branching.

## Game Integration

- **Plugin / Entry Point:** Pure library crate; registers no `game-sim` plugin.
- **Requests & Events:** None.
- **Callers:** Used by `game-sim` and feature crates for cold/sparse resources, nested per-person tables, and deadline agendas.

## Dependencies & Testing

- **Dependencies:** None (foundational storage crate).
- **Testing:** `cargo test -p game-storage` runs differential tests against `BTreeMap` across splits, merges, snapshots, incremental diffs, and worker panics.
