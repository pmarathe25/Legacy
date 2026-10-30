# Compact conflict planning

`Plan::compile` freezes encounter membership, orders, defense, and damage into canonical target groups and incoming attacker IDs. The fused overlay owner applies bounded order changes through copy-on-write edge masks and defense pages; encounter admission and overlay compaction rebuild topology. `Plan::prepare` reads sorted committed people and returns a private `Round`: one validity bit per potential strike, saturated target damage, and a shared immutable topology generation. Per-attacker event rows are expanded only on demand, in `(target, encounter, attacker)` order. `Round::events` also streams receipts without allocating a full event vector. Old receipts remain correct after a replacement plan is compiled or dropped.

Availability is evaluated in fixed 2,048-edge tasks, allowing a single hot target to use all workers. Damage counts bits with parallel reductions for long target ranges. Saturating the physical total never removes individual receipts. This preserves current simultaneous attack, defense, and withdrawal semantics; a retreating target can still be struck this round.

The example compares existing conflict preparation plus damage reduction with compact planning/reduction. Compilation and receipt expansion are reported separately. It does not measure full ticks or roster cleanup: the existing preparation also clones the roster, while compact preparation retains its immutable source.

The fused host now integrates this plan through `game-parallel-fused-conflict::packed`. It reads committed physical blocks directly, applies damage to unpublished post-needs health, and stages changed encounters after lifecycle effects. Casualties and retreats create filtered edge masks and updated target-defense values sharing the original immutable topology. Unchanged rosters are not cloned. The bounded order overlay patches validated commands without rebuilding the base topology; prior receipts retain their own generation. Failed or discarded drafts publish neither roster edits nor receipts. Export expands exact attributed events into the editable world's format. Current position lookup avoids stale cached block offsets when births, departures, or arrivals rearrange storage.

`prepare_with_filter` accepts an optional round-local `(attacker, target)` eligibility predicate for range or visibility. A failed predicate never disables the edge in future rounds. `filtered` is specifically for permanent lifecycle removals and additional defense; arbitrary replacement attacks are represented by game-parallel-conflict-overlay; its bounded compaction requests recompilation.

## Local measurement, 2026-10-28

Release build, one million people, medians after one warmup. The owner-only comparison used five samples; the full fused benchmark used three. Times are milliseconds. Initial import/compilation and final export are excluded from tick timing and reported separately by the examples.

| Full fused workload | 1 worker | 2 workers | 4 workers | 6 workers |
| --- | ---: | ---: | ---: | ---: |
| Stable spread targets | 30.98 | 18.76 | 14.42 | 13.58 |
| Stable two hot targets | 9.28 | 5.50 | 4.06 | 4.13 |
| Spread casualties | 35.61 | 22.19 | 18.01 | 16.06 |
| Hot-target casualties | 26.65 | 17.47 | 14.60 | 14.49 |
| Single order edit + spread round | 214.14 | 168.71 | 145.76 | 137.48 |
| Single order edit + hot round | 135.67 | 118.71 | 111.00 | 111.23 |

Casualty rounds include actual roster replacement and filtered-generation creation. Spread casualties kill every participant; hot casualties kill two targets and turn the surviving attackers into defenders. Each casualty sample starts with a fresh imported world so inactive later rounds do not dilute the result. These historical command samples include command validation and full topology recompilation. Full initial import/compilation took 0.49–0.76 seconds. The subsequent overlay measurements below supersede command invalidation costs. These results do not establish near-linear scaling.

The owner-only benchmark reduced spread planning/reduction from 42.00 to 13.72 ms at one worker and from 25.25 to 7.00 ms at six workers. Hot-target planning/reduction changed from 38.61 to 1.36 ms and from 20.37 to 1.10 ms respectively. Topology compilation cost 106–184 ms; receipt expansion cost 10–17 ms. Excluding those costs is appropriate only for unchanged cached rounds, not arbitrary gameplay workloads.

Reproduce with `cargo run --release -p game-parallel-conflict-packed --example conflict_compare -- 6 1000000 hot 5` or `cargo run --release -p game-parallel-fused-physical --example fused_combat_rounds -- 6 1000000 hot casualty 3`.

The [bounded order overlay](../game-parallel-conflict-overlay/README.md) now reduces million-person one-command rounds to 34.497/22.317/16.148/14.787 ms (spread) and 11.955/6.537/4.326/4.095 ms (hot) at 1/2/4/6 workers. Immutable topology and lazy attributed rounds now live in `game-parallel-conflict-receipts`; paged generation columns preserve discarded drafts and prior receipts across sparse edits.
