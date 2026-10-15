# Plan: evaluate ECS storage for Legacy

**Recommendation:** Evaluate standalone `bevy_ecs` as the leading candidate for frequently updated entity state. Compare it with dense custom tables and the small Legion candidate required by the [performance design](../docs/design/performance.md). Keep one production implementation after the comparison.

The intended architecture combines compact entity storage with dedicated sparse records and immutable content definitions. Feature crates continue to own their mechanics and tests. Sharing an ECS world does not require moving gameplay into a large simulation or assembly crate.

This is a proposed engineering direction. No representative benchmark currently establishes a winning storage implementation.

## 1. Address the current copying cost

The current host in [game-sim](../crates/game-sim/src/lib.rs) stores each feature's resource behind a type-erased entry. On the first write in a transaction, [transaction.rs](../crates/game-sim/src/transaction.rs) clones the entire resource.

For example, changing one person's hunger clones the complete `Bodies` map. If many people update through separate transactions, copying can grow quadratically with population. The physiology batch also copies bodies into temporary vectors before writing results back.

Replace whole-table cloning with staged changes to affected records or bounded chunks. This is required whichever storage implementation wins. Wrapping the existing maps in ECS resources would preserve their copying and traversal costs.

The existing [kernel benchmark](../crates/game-sim/benches/kernel.rs) measures a synthetic numeric vector and two stock claims. It remains useful for host correctness, but cannot select storage for the full game.

## 2. Divide state by its access pattern

| State | Proposed representation | Reason |
| --- | --- | --- |
| Bodies, needs, location, action progress, frequently accessed skill state | ECS components or dense indexed columns | Repeated traversal over many individual records |
| Relationships, knowledge, agreements, claims, memberships, institutions | Dedicated sparse tables and indexes | Irregular lookups and graph-shaped data |
| Detailed memories, descriptions, historical records | Separate cold storage | Bulk numeric updates should not load them |
| Crop, object, trait, recipe, and scenario definitions | Immutable content catalogs | Definitions are shared data loaded without recompiling |
| Clock, due work, command receipts, pending choices | Host-owned scheduling and session state | These enforce simulation contracts independently of entity layout |

Content instances reference stable definition keys or IDs. Adding a crop, object, or personality trait does not create a Rust component type. Its properties belong in the definition and instance values.

Use stable component composition for ordinary updates. Hunger changes, action switches, and trait values generally update fields rather than adding and removing components. Evaluate sparse-set storage for optional components that change membership frequently.

Bevy's table storage favors iteration, while sparse-set storage favors component insertion and removal. Component combinations determine archetypes, and structural changes move entities between them. These are tradeoffs to exercise in the workload, not evidence of a speedup for Legacy. See the official [storage documentation](https://docs.rs/bevy_ecs/latest/bevy_ecs/component/enum.StorageType.html) and [archetype documentation](https://docs.rs/bevy_ecs/latest/bevy_ecs/archetype/index.html).

## 3. Preserve feature boundaries and game contracts

- Each feature defines its components or records, rules, requests, systems, and persistence validation. Its tests construct only the required dependencies.
- The assembly installs features and binds public commands and observations. It contains no component-processing loops or gameplay rules.
- A component or record has one mutation owner. Register and verify system write access against those owners; cross-feature effects still use receiver-owned requests. Avoid giving feature code unrestricted mutable access to the shared world.
- Persistent `PersonId`, `InventoryId`, and other game IDs remain independent of ECS entity handles and dense row positions. Saves, randomness, and clients use the persistent IDs.
- The simulation clock and due-work queue remain authoritative. ECS schedules execute ready phases; rendering frames do not advance gameplay.
- Coordinate worker pools so ECS scheduling and numeric batches share a bounded CPU budget.

Standalone Bevy ECS supplies storage, queries, and scheduling without requiring Bevy's renderer. It does not replace the game's ownership, conservation, consent, or replay rules. See the [Bevy ECS documentation](https://docs.rs/bevy_ecs/latest/bevy_ecs/).

## 4. Retain atomic, deterministic commit

Use the same execution contract for every candidate:

```text
consistent read view
→ evaluate independent batches
→ emit intentions and record changes
→ resolve shared-resource conflicts in stable order
→ validate and commit accepted changes
→ publish observations and meaningful events
```

Stage only affected records, changed components, or bounded next-state chunks. Validate transfers, reservations, and structural operations before applying the accepted commit. A failed command or worker evaluation leaves the last committed state intact, including IDs, due work, and events.

Order contested operations, births, ID allocation, and events by game-defined keys. ECS iteration order, physical row order, and thread completion order do not decide outcomes. Births and deaths apply structural changes at explicit boundaries.

Do not clone the entire ECS world to obtain rollback. Keep the existing public guarantees while replacing the current transaction storage mechanism. Reuse the same pure rule functions across candidate implementations.

## 5. Build one representative comparison harness

Implement small workload adapters for standalone Bevy ECS, Legion, and dense custom storage. Use the current implementation as a correctness reference where its behavior is applicable. Keep the adapters limited to the shared workload; do not build three complete games or a permanent universal storage layer.

The workload includes:

1. Needs and passive progress across individual people.
2. Candidate scoring using personal knowledge, plus sparse relationship lookups.
3. Disjoint and highly contested inventory transfers and reservations.
4. Travel arrivals and shipments between areas.
5. Births, deaths, component membership changes, and stable ID mapping.
6. Committed observation extraction and save/load followed by continued execution.

Use identical definitions, rules, seeds, logical operation order, and observation output. Exercise 15, 1,000, and 10,000 people, plus the design's 20,000-person stress case. Vary worker counts across 1, 2, 4, and the available bounded budget. Include both distributed activity and a shared warehouse that exposes serial arbitration costs.

Measure release builds with repeated runs. Print fixture counts and hardware/build metadata alongside:

- End-to-end throughput and phase timings, including packing and commit.
- Command and pause latency, including p95 and p99.
- Allocations, peak memory, working buffers, and retained observations.
- Birth/death and structural-change costs.
- Worker scaling and time spent in serial coordination.
- Save/load costs and canonical state/event hashes.

Assert equal logical outcomes across candidates, worker counts, irregular advance sizes, save/load, and deliberately perturbed evaluation completion order. Benchmark processes must check records and conservation; successful exit alone is insufficient.

## 6. Select and replace the storage implementation

Favor Bevy ECS if it meets the [provisional performance budgets](../docs/design/performance.md#measure-the-whole-simulation), supports clear owner boundaries and deterministic commit, and reduces infrastructure maintenance. Prefer dense custom storage if it provides a material workload advantage or substantially simpler transaction integration. Include Legion's measured results in the same decision.

Record the choice and relevant measurements briefly beside the retained benchmark. No separate performance report is required. Remove losing adapters and dependencies after selection.

Replace the affected storage and transaction path directly. There is no backward-compatibility requirement and no requirement to retain an interim production path. Keep feature-facing behavior tests and add assertions for the selected storage's boundary cases.

## Completion criteria

- One production storage implementation remains, with no alternate kernel or duplicate authoritative state.
- Updating one record does not clone an entire population table.
- Feature crates build and test without the assembly, and owner boundaries remain enforced.
- Stable IDs, quantity conservation, invalid-command rollback, passive queries, pending choices, and accepted control-loss intervals retain their tested semantics.
- Worker counts, chunking, and save/load preserve canonical state and meaningful events.
- Representative measurements establish which provisional budgets are met and which remain unresolved.
- Formatting, strict lint checks, and relevant workspace tests pass.
