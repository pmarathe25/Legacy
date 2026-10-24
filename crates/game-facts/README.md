# game-facts

`game-facts` stores per-person remembered facts by combining shared immutable local scenes with personal evidence stamps and cold historical memories.

**Why it matters:** Hundreds of people in the same area observe the same roads, patches, and inventories; sharing one immutable `Scene` payload across observers avoids duplicating identical fact maps while keeping each person's observation time, source, and stale memories distinct.

## Responsibilities

- **Shared visible scenes (`Scene<V>`):** Stores sorted fact keys and reference-counted payloads for an observable place, reusing unchanged allocations across revisions.
- **Personal fact stores (`Facts<V>`):** Pairs an observer's current `Scene` and personal evidence stamp (time, source, confidence) with a persistent `game-storage::Table` of cold historical facts and explicit overrides.
- **Zero-copy views (`FactRef`):** Provides borrowed `get`, `iter`, and `range` queries that merge active scene entries with cold overrides in key order.
- **Namespace revision tokens:** Tracks process-local payload revision markers per key prefix (`same_namespace`) so downstream queries can skip rescanning unchanged fact categories.
- **Key-set tokens:** `same_keys` proves membership is unchanged across snapshots, including scene demotion and payload/metadata updates. Additions and removals invalidate the token. Consumers still compare keys when tokens differ.

## Boundaries

- **Representation vs. perception:** Owns the data structure and serialization for remembered facts; [`game-knowledge`](../game-knowledge/README.md) decides what a person can perceive, when scenes refresh, and how spoken reports propagate.
- **Process-local tokens:** Version and namespace revision tokens accelerate in-memory queries but are never serialized or exposed to gameplay rules.

## How It Works

- **Stamp-only scene refresh:** When an observer re-observes an unchanged scene, `Facts` updates a single personal timestamp/source stamp instead of touching every visible entry.
- **Cold history Demotion:** When an observer moves or a key leaves the visible scene, previously seen entries are frozen into the observer's cold table with their last-seen stamp intact.
- **Override precedence:** Direct inspections (`ObserveStore`, `Tell`) write an explicit override that supersedes the ambient scene until a newer scene observation replaces it.

## Game Integration

- **Plugin / Entry Point:** Pure library crate; registers no `game-sim` plugin.
- **Requests & Events:** None.
- **Callers:** Used by [`game-knowledge`](../game-knowledge/README.md) to back each person's subjective memory and by [`game-food-memory`](../game-food-memory/README.md) to detect inventory payload changes.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-storage`](../game-storage/README.md).
- **Testing:** `cargo test -p game-facts` verifies shared scene reuse, scene transitions, overrides, namespace revision tokens, and canonical map serialization.

Compact shared evidence dictionaries are owned by
[`game-snapshot::evidence`](../game-snapshot/README.md). `Facts` retains its expanded
map serialization and runtime memory behavior; archive encoding and restoration
remain independent of perception rules.
