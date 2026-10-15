# Legacy

Legacy is a headless simulation game. The workspace has one simulation host and small crates that own individual mechanics. The game design remains in [docs/plan.md](docs/plan.md).

This is a deliberate rewrite of the earlier implementation. The working mechanics run in explicitly labeled fixtures. The previous gameplay was not ported wholesale, and the complete campaign described in `docs/design/` remains unfinished. Old APIs, saves, scenarios, and tests are incompatible.

## Run and test

```sh
cargo run -p game-cli -- --list
cargo run -p game-cli -- --scenario kernel-fixture --seed 1
cargo test -p game-agriculture
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo bench -p game-sim --bench kernel
```

The CLI reads commands from stdin or `--script <path>`. `run` takes seconds; authoritative times use milliseconds. For the kernel fixture, this script moves the ruler to the farm:

```text
status
move 2
run 120
status
quit
```

`--content <directory>` selects a content pack; repeated `--mod <directory>` arguments add overlays. [The content guide](content/README.md) explains runtime definitions, schemas, and extension without rebuilding.

`look` observes nearby roads and resources; `observe-patch <id>` inspects a local patch; `tell <person-id> <fact-key>` shares one remembered fact with someone at the same place.

`command <JSON>` submits any [public command](crates/game-api/src/lib.rs). `save <path>` and `load <path>` persist the session. `debug` explicitly exposes privileged state; normal observations use the current ruler's facts and witnessed events.

## Boundaries

**Features:** Each crate owns its state, requests, rules, cancellation, content schemas, and persistence validation. Hosted features export `plugin(&mut Builder)`. Pure calculations such as routes and inheritance need no host. Feature tests compose their required dependencies and never import the application.

**Interaction:** A caller submits the receiving feature's typed request. A lower feature emits an event when an upper feature needs to react. Subscribers register in the upper feature, which keeps the dependency graph acyclic. When a lower feature needs an upper fact, it defines a typed request that the upper plugin serves. Read-only host queries reject writes, events, scheduling, and ID allocation. Registered request handlers and event subscribers may mutate only resources owned by their plugin; the host enforces this at runtime. Requests execute in one transaction; errors discard writes, allocated IDs, notifications, and scheduled work. Nested `attempt` calls provide a rollback boundary for recoverable failures.

**Content:** One namespaced `content/` store supplies runtime definitions. `game-content` owns discovery, loading, overlays, and provenance; the receiving features own validation and resolved snapshots. Starting scenarios, traits, objects, crops, recipes, and rule profiles are data.

**Assembly:** `game-assembly` installs plugins, binds player commands, composes observations, and exposes the simulation lifecycle. The current ruler is supplied by the binding rather than accepted as a client-selected actor. Retries are keyed by the original public command, including across ruler death and save/load.

**Host:** `game-sim` stores typed resources, orders due requests, commits transactions, and saves complete state. There is one clock and one canonical action record per actor. CPU workers evaluate numeric batches against immutable inputs; arbitration and commit remain ordered.

**Growth:** [Architecture tests](crates/game-assembly/tests/architecture.rs) enforce dependency direction, prohibit cross-owner mutation and duplicate kernels, and cap source size at 600 lines per feature crate, 650 for knowledge, 850 for shared storage, and 1,100 for the generic host. Split a growing mechanic into another named owner rather than moving it into the assembly.

See the [crate index](crates/README.md) for responsibilities and dependencies and [migration note](MIGRATION.md) for the replacement boundary.

## Evidence and limits

Feature tests exercise timed movement and work, physical transfers, reservations, access and theft, needs and mortality, local facts and reports, learning and choice, committed food plans across travel and gathering, promises and default, institution rules, enforcement, construction, deliveries, parties, succession, guardian confinement, and invocation consent. Composed tests check passive observations, public command retries, CLI equivalence, worker counts, irregular advances, and continued execution after saving.

These tests establish their asserted fixture behavior. They do not establish complete autonomous enterprises, household care and childhood agency, full institutional invention and political procedures, regional trade, final tactical rules, or the final past-life benefit and recovery model. Those requirements still belong to the design documents and must be implemented in their owners.

[Performance benchmarks](tools/performance/README.md) exercise storage, autonomous survival, queued input, and save/load. Numeric state uses bounded dense leaves; sparse records share unchanged values between transactions. Point updates and nested rollback no longer clone population tables. These fixtures measure the implemented mechanics; the complete late-game economy and long-history workload remain to be built.
