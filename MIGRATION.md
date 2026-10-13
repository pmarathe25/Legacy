# ECS migration completion criteria

Complete the migration of the existing headless game to shared components and feature-owned systems. This file is temporary and can be deleted when every criterion below is met. New gameplay beyond the existing implementation is outside this migration.

## Crate responsibilities

- [x] `game-world` is a feature-agnostic host. Its production dependencies and source contain no game feature, command, scenario, ruler, trait, crop, or health policy. Feature-named modules such as `genetics.rs` and `health.rs` have left the crate.
- [ ] A separate game assembly crate registers component schemas and systems, connects typed feature interfaces, constructs scenarios, and binds public commands and views. It orders owner-prepared effects and invokes generic transaction infrastructure; domain algorithms, entity-update loops, scoring rules, and feature validation live in their owners.
- [ ] Every gameplay behavior has a feature-owned entry point that tests can exercise without the assembly. This includes candidate selection, scoring, progression, perception, reconciliation, and event reactions. Assembly adapters translate factual inputs and typed effects; they do not determine those policies or mutate feature state directly during play.
- [ ] Features communicate through small typed dependencies, traits, components, and effects. Integration that has a natural feature owner lives there. Shared contracts or a focused integration crate handle genuinely shared behavior; the assembly does not provide a required helper library for the features.
- [ ] Representative composed behaviors run in tests without importing the assembly. Receiving features own their reactions to typed effects or events. Ports supply facts or invoke another owner's operation; callbacks do not conceal assembly-owned decisions, thresholds, selection, or progression.
- [ ] Each domain crate owns its components, rules, validation, behavior, and prepared effects. It compiles and tests independently of the assembly crate. Tightly coupled behavior has a clear owner rather than duplicate implementations or circular dependencies.
- [ ] Interfaces expose plain Rust components, borrowed rows, small factual inputs, and typed effects. Genetics handles opaque inheritance; trait providers interpret genes. Comparable boundaries keep other features indifferent to their collaborators' internal representation.

## State and execution

- [ ] The host owns one shared component store. Entity state has one canonical owner and representation; feature runtimes retain rules, content, and system metadata rather than shadow entity maps or private ECS worlds.
- [x] Only `game-ecs`'s private adapter depends on the chosen ECS engine. Vendor entity handles, query internals, derives, and schedules do not appear in application or feature interfaces.
- [ ] Features share stable entity IDs and a common spatial model. Local systems consume local cells and explicit component joins rather than unrelated global feature state. Carrier resolution and movement use the same spatial rules across systems.
- [ ] Systems declare their ordering and run at appropriate deadlines, fixed intervals, or events. Independently keyed work can use the bounded executor; authoritative results are merged in stable order. Derived indexes and query caches rebuild safely after structural changes and loading.
- [ ] Prepared effects reject stale inputs and invalid configuration before publication. Failed commands and transactions preserve canonical state, allocation counters, and published events. Cross-feature effects are applied through owner interfaces.

## Content and proof

- [ ] Concrete authored content uses validated, human-readable data files. Adding an instance of existing mechanics, such as a crop or recipe, requires editing content rather than recompiling logic. Saves retain the definitions needed for deterministic continuation.
- [ ] Existing playable scenarios, commands, observations, autonomy, births, development, conflict, and succession continue to work. Saves round-trip, malformed saves fail safely, and resumed execution matches uninterrupted execution. Compatibility with old save versions is unnecessary.
- [ ] Executable tests cover owner behavior in isolation and representative cross-feature flows. Results agree across supported worker counts, reordered storage/jobs, and equivalent time slices; client observations and profiling do not alter simulation outcomes.
- [ ] Runnable benchmarks cover representative local systems and the composed game. They demonstrate bounded scheduling and parallel execution without introducing an unmeasured per-entity whole-population scan. Report measured performance limits honestly.
- [ ] Formatting, strict workspace Clippy, and the complete workspace test suite pass. `crates/README.md` contains the actual final dependency graph, and each crate has a concise README describing its purpose and interfaces. Changes are committed in reviewable increments.

## Assembly boundary review

For every production assembly module, identify why its code needs to remain at the application boundary. Registration, startup construction, public command/view binding, and transaction ordering are valid reasons. An algorithm mentioning several features is insufficient: assign its policy to a feature or a focused integration owner and pass the required facts through typed interfaces.

Apply this review to each function, not just its module name. Conditions that decide whether an actor agrees, which target is chosen, how work progresses, or how an event changes a character belong to feature owners. Assembly may resolve IDs, borrow the required rows, translate public request and response types, register handlers, and sequence prepared effects. Atomic application and publication are generic infrastructure. A new integration crate must own a specific interaction, not become a second assembly containing unrelated policies.

For example, genetics owns transmission and emits a prepared inherited genome; physiology and temperament own interpretation and their resulting component changes. A composed test must exercise those public interfaces with a small store, without constructing the playable simulation. Likewise, a material transfer must use Cargo's conservation and capacity checks together with Property's claim and provenance effects, rather than reimplementing either rule in an assembly handler.

Dependency tests must prevent features from importing the assembly. Owner tests must cover the public entry points that the assembly calls. The final source review must find no domain policy left in assembly handlers and no production dependency from the generic host to a feature. Extracting a crate or reducing its line count alone does not satisfy these criteria.

Features are assumed to be available. Runtime removal, compile-time feature selection, graphics, and old-version compatibility are not completion requirements.

## Current migration status

The migration remains incomplete. The criteria above are restored from the original migration document; completed owner slices do not establish completion of the composed game.

The shared owner transaction protocol is implemented in `game-world`. It provides typed requests, one writer per component, stale-input checks, provisional cross-owner effects, atomic publication, action coordination, and deterministic scheduling. Features own their rules and executable handlers. The application registers those handlers and translates public types.

The first public reference covers farming completion, command idempotency, save/replay, and weather progression across time slices. Both tests passed with `cargo test --offline -j2 -p game-assembly --test native_reference --message-format=json -- --nocapture`; test runtime was 0.07 seconds. This proof does not cover all existing scenarios or establish removal of legacy behavior.

Required work still includes:

- Route the existing public simulation through the owner implementations and remove the old feature behavior from assembly. A parallel native driver is an intermediate step.
- Finish Agreements judgments, exchange, duty choice, and Project proposal/acceptance integration.
- Finish Projects funding and procurement, Missions acquisition planning and validation, and Invocation selection and views.
- Complete Army workflows and preserve Enforcement policy visibility between actors within one review.
- Require every enabled feature's peer providers, complete owner save validation, and remove canonical legacy activity and scheduling records.
- Remove population-dependent transaction call limits that reject valid composed workloads.
- Run focused owner tests for the completed handlers, then perform the final assembly ownership review, composed tests, workspace checks, and performance measurements.

Use focused owner tests during extraction. Run broad integration checks and benchmarks after the executable paths are integrated and the obsolete implementations are removed. Commit verified slices as they are completed.
