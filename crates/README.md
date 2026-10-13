# Crate boundaries

Domain crates own game records and the rules that change them. `game-assembly` composes the playable simulation, registers component schemas and systems, and binds commands and observations. `game-world` supplies feature-neutral storage and publication; `game-sim` supplies the clock, deadlines, and bounded execution.

**Content:** Opening assets, crops, recipes, environment data, and human definitions are authored in TOML. Opening customs are validated definitions resolved by `game-openings` into institution, property, and agreement types. Saves retain the selected definitions for deterministic continuation; see [`content/README.md`](../content/README.md).

## Production workspace dependencies

Each row lists direct production dependencies declared by that crate's current `Cargo.toml`. The table covers the 35 workspace crates. An arrow means consumer → provider; `?` marks an optional dependency. Third-party and development dependencies are omitted.

| Crate | Workspace providers |
| --- | --- |
| `game-agreements` | `game-cargo`, `game-core`, `game-ecs`, `game-logistics`, `game-property`, `game-transport` |
| `game-agriculture` | `game-content?`, `game-core`, `game-ecs`, `game-land` |
| `game-api` | `game-agreements`, `game-agriculture`, `game-armies`, `game-characters`, `game-conflict`, `game-core`, `game-ecology`, `game-health`, `game-invocation`, `game-lineage`, `game-missions`, `game-physiology`, `game-projects`, `game-property`, `game-realm`, `game-society`, `game-transport` |
| `game-armies` | `game-conflict`, `game-core`, `game-ecs`, `game-missions` |
| `game-assembly` | `game-agreements`, `game-agriculture`, `game-api`, `game-armies`, `game-cargo`, `game-characters`, `game-conflict`, `game-content`, `game-core`, `game-ecology`, `game-ecs`, `game-environment`, `game-genetics`, `game-health`, `game-invocation`, `game-land`, `game-lifecycle`, `game-lineage`, `game-logistics`, `game-missions`, `game-openings`, `game-physiology`, `game-projects`, `game-property`, `game-realm`, `game-sim`, `game-skills`, `game-society`, `game-spatial`, `game-transport`, `game-workshop`, `game-world` |
| `game-cargo` | `game-core`, `game-ecs`, `game-logistics` |
| `game-characters` | `game-core`, `game-ecs`, `game-genetics?`, `game-logistics`, `game-spatial` |
| `game-cli` | `game-api`, `game-assembly`, `game-content`, `game-session` |
| `game-conflict` | `game-cargo`, `game-core`, `game-ecs`, `game-physiology`, `game-skills`, `game-spatial`, `game-transport` |
| `game-content` | `game-core` |
| `game-core` | — |
| `game-ecology` | `game-core`, `game-ecs`, `game-land`, `game-logistics` |
| `game-ecs` | `game-core` |
| `game-environment` | `game-content?`, `game-core`, `game-ecs` |
| `game-genetics` | `game-core`, `game-ecs` |
| `game-health` | `game-core`, `game-ecs` |
| `game-invocation` | `game-cargo`, `game-characters`, `game-core`, `game-ecs`, `game-society` |
| `game-land` | `game-core`, `game-ecs` |
| `game-lifecycle` | `game-core`, `game-ecs`, `game-genetics?` |
| `game-lineage` | `game-core`, `game-ecs`, `game-logistics`, `game-transport` |
| `game-logistics` | `game-core` |
| `game-missions` | `game-agreements`, `game-cargo`, `game-characters`, `game-conflict`, `game-core`, `game-ecs`, `game-logistics`, `game-spatial`, `game-transport` |
| `game-openings` | `game-agreements`, `game-content`, `game-core`, `game-ecology`, `game-property`, `game-society` |
| `game-physiology` | `game-core`, `game-ecs`, `game-genetics?` |
| `game-projects` | `game-agreements`, `game-cargo`, `game-core`, `game-ecs`, `game-logistics`, `game-workshop` |
| `game-property` | `game-cargo`, `game-core`, `game-ecs`, `game-logistics` |
| `game-realm` | `game-agreements`, `game-cargo`, `game-characters`, `game-core`, `game-ecs`, `game-missions`, `game-society` |
| `game-session` | `game-api` |
| `game-sim` | `game-core` |
| `game-skills` | `game-core`, `game-ecs` |
| `game-society` | `game-agreements`, `game-cargo`, `game-characters`, `game-core`, `game-ecs`, `game-missions`, `game-property` |
| `game-spatial` | `game-core`, `game-ecs` |
| `game-transport` | `game-core`, `game-ecs`, `game-spatial` |
| `game-workshop` | `game-cargo`, `game-content`, `game-core`, `game-ecs`, `game-skills` |
| `game-world` | `game-core`, `game-ecs`, `game-sim` |

The application depends on a broad set of feature crates to compose one playable simulation. Feature-to-feature dependencies are direct when one owner consumes another owner's typed records or effects. For example, Armies uses Missions and Conflict; Society uses Agreements, Property, Characters, Cargo, and Missions; Missions uses Conflict and Transport. Dependency tests prevent feature crates from importing the assembly and keep the selected ECS engine private to `game-ecs`.

## Shared storage and transaction boundary

**Storage:** `game-ecs` registers plain Rust component types by schema key and stores rows under stable game IDs. Its selected ECS engine stays behind the adapter. Features own their row types and validation; runtime structs generally retain rules or cursors while entity records live in the shared store.

**Opening reads:** [`ReadSet`](game-ecs/src/reads.rs) can retain selected present or absent rows, entity presence, and scoped component membership. It validates those facts before staging. After an owner appends its edits, `ReadSet::bind` returns a `ReadGuard` containing the exact admitted projection and writes. Later validation rejects changed opening facts, altered projected results, unadmitted edits, or a different batch. `Batch::projected` provides indexed row projections; membership witnesses protect the selected set while allowing declared additions or removals.

**Publication:** [`Transaction`](game-world/src/lib.rs) carries a host opening, one shared batch, owner guards, logical time, ID and event budgets, events, and revision changes. `GenericHost::commit_transaction` checks the host opening and budgets, validates all owner guards, then applies the component batch before advancing the clock and counters and publishing events. A failed check leaves canonical state and publication cursors unchanged. Owners validate their own rules and bindings before the host commits.

**Composition:** Assembly supplies factual adapters, binds public commands and views, and orders prepared owner effects in a transaction. Each receiving feature owns its reactions. Global intervals prepare their ordered work in a preview host and publish the resulting rows, clock, counters, and events atomically. Owner tests exercise the same entry points without constructing the playable assembly.

## Feature owners

**Armies:** `game-armies` stores `Army`, personal `Knowledge`, `Talk`, `Queue`, and `Approach` rows in the shared store. Its `World` retains rules and review timing. `Kernel` prepares relationship changes, while `Systems` owns invitation, participation, order, response, autonomy, and coordinated approach policy through factual `Ports`. The remaining adapter and scenario composition live in `game-assembly`. See [`state.rs`](game-armies/src/state.rs) and [`systems.rs`](game-armies/src/systems.rs).

**Social:** `game-characters` owns canonical personal interaction, participation, and development rows, plus admission and completion hooks. `game-society` owns institutional membership, proposals, judgments, enforcement cases, and policy decisions. Their interfaces exchange prepared effects and observations; local discovery, contact facts, and publication still require composition. See [`social.rs`](game-characters/src/social.rs), [`personal.rs`](game-society/src/personal.rs), and [`enforcement_systems.rs`](game-society/src/enforcement_systems.rs).

**Conflict and Force:** `game-conflict::World` retains combat rules; maps, encounters, fighters, and captivity use canonical component rows. `game-conflict::force::Runtime` retains force rules and its event cursor; yielding, restraint, and escape rows are also in the shared store. Combat and Force expose prepared owner operations and guarded effects. Assembly adapts body, location, cargo, and mission facts and sequences the resulting changes. See [`lib.rs`](game-conflict/src/lib.rs), [`force.rs`](game-conflict/src/force.rs), and [`policy.rs`](game-conflict/src/force/policy.rs).

**Enforcement:** `game-society` owns enforcement policy, case evidence, hearings, remedies, and dispatch preparation. Dispatch proposals use Mission and Agreement interfaces; confinement uses Conflict's physical effects. Enforcement records and personal knowledge are component rows, while `game-assembly` supplies factual ports and binds public events and activities. See [`enforcement.rs`](game-society/src/enforcement.rs) and [`enforcement_storage.rs`](game-society/src/enforcement_storage.rs).

**Person progress:** `game-skills` stores per-person `Skills` rows and converts factual activity and interaction intervals into guarded practice effects. `game-lineage` stores invocation lesson learning as a person component and owns its threshold, lesson, and guardian priority decisions. Characters owns canonical temperament and social development rows. Assembly gathers body, activity, and contact facts and sequences owner-prepared changes. See [`learning.rs`](game-skills/src/learning.rs), [`learning.rs`](game-lineage/src/learning.rs), and [`temperament.rs`](game-characters/src/temperament.rs).

## Execution and measured limits

The bounded executor reuses a fixed worker pool and merges keyed results in stable order. The interval transaction currently copies the shared store and evaluates guarded differences before publication; this serial work scales with attached rows and limits the benefit from more workers. Runnable local-system and complete-game benchmarks, their fixture boundaries, and measured limits are documented in [`game-assembly/README.md`](game-assembly/README.md). Local ECS or feature timings do not establish complete-game throughput.

For content files and the definition loading boundary, see [`content/README.md`](../content/README.md).
