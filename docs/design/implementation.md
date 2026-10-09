# Implementation stages and verification

The deliverable is the complete game mechanics running headlessly, with a CLI and public API that a graphics engine can use later. Graphics is excluded from this implementation scope; combat, player interactions, saves, and campaign rules are included. Optional GPU acceleration can follow measurement later, but the architecture must allow it now.

Build through small, testable stages. Expose each player-facing behavior through the API and CLI when it is added. A stage can use explicitly named fixtures to isolate behavior; fixtures must not become hidden universal social rules. A placeholder resolver does not implement the final mechanic.

Code and executable tests establish implemented behavior. The scenarios below describe tests to write with each feature, not reports or checklists to produce afterward. Use existing tests and code to find the next unfinished behavior on subsequent runs; do not maintain a parallel completion ledger. There is no handoff-document requirement.

Read the [question timing](decisions.md#when-to-ask) for the feature being implemented. Ask about unresolved intent when it becomes relevant and continue independent work. Choosing internal types, dependencies, algorithms, test values, or file organization does not require a new design discussion.

## First coding task

Create a Rust workspace containing `crates/game-api`, `crates/game-sim`, and `crates/game-cli`. Use those package names so the commands below work. Put public IDs, time, command/result, and observation types in the API crate; behavior and storage in the simulation crate; parsing and display in the CLI. Introduce other modules only when they contain working behavior.

Build this executable slice before extending the design or implementing a full subsystem:

1. Add typed IDs, integer time, scenario construction, and simple typed tables as an initial reference implementation. This provides a baseline for the storage comparison below, not a final framework selection.
2. Add a deterministic `kernel-fixture`: a village, farm, and forest; bidirectional village–farm and village–forest links taking two and three minutes respectively; a ruler, two guardians, and twelve citizens. Keep fixture rates and initial possessions in the fixture. This is test data, not a player starting scenario.
3. Implement a typed ruler movement command, debug inspection, and time advancement. Moving to a disconnected or nonexistent area fails without changing the clock, action, or ID allocator. Arrival occurs at the scheduled time; inspecting the world does not advance it.
4. Add one independent per-person numeric update and a contested inventory transfer to exercise parallel evaluation and ordered commit. Give participants explicit access in this fixture. Generate competing intentions through test setup, not a player command that orders arbitrary citizens. Simple fixture rates suffice; hunger, autonomy, and social permission follow in later stages.
5. Run the same rules with one worker and a bounded worker pool. Keep evaluation outputs separate from shared mutation; coordinate competing transfers. Add a small `kernel` benchmark using this same code. Expand it during the comparison, without building production social systems just to time them.
6. Expose `status`, `run <duration>`, and `move <area-id>` through the CLI, plus clearly marked debug inspection. A script must be able to run the fixture without interactive input. The CLI calls the same API as the tests.

The first tests should exercise observable behavior:

| Test idea | Required assertion |
| --- | --- |
| Movement consumes elapsed time | Starting in the village, at 119,999 ms the ruler is in transit to the farm; at 120,000 ms they are at the farm. No work is credited during travel. |
| Invalid commands are inert | An invalid destination preserves canonical state, time, pending action, and the next allocated ID. |
| Competing transfers conserve stock | Two actors request the same 1,000 fixed-point units. Exactly one receives them, the source empties once, and total stock stays 1,000. |
| Chunking and workers preserve outcomes | At a common target time, compare one large advance with irregular smaller advances, one worker with several, and reversed job completion. Logical state and meaningful events match. |
| Observation is passive | Additional queries neither change decisions nor add simulation time or gameplay events. |

These may share one integration-test fixture. Assert the affected records, quantities, times, and events; a process exit or a printed success message alone does not prove the behavior.

The initial slice should support these commands from the repository root once implemented:

```sh
cargo test --workspace
cargo run -p game-cli -- --scenario kernel-fixture --seed 1
cargo bench -p game-sim --bench kernel
```

These commands describe the first deliverable; they do not run in the current documentation-only repository. No graphics setup, real campaign content, GPU implementation, or fully modeled government is needed to begin.

## 0. Choose storage using the executable slice

Use the slice's rules and assertions to compare standalone Bevy ECS, Legion, and custom typed storage with explicit scheduling. Add synthetic branching decisions, sparse relations, shared-resource arbitration, transfers, creation/removal, and observation extraction to the common benchmark. Synthetic operations exercise access patterns without choosing unanswered gameplay rules.

Exercise 1, 2, 4, and available workers at 15, 1,000, and 10,000 people, including distributed work and a highly contested stockpile. Compare correctness, throughput, tail latency, memory, and implementation complexity. Have the runner print fixture counts, seed, worker count, build information, and timings; supply hardware metadata with local benchmark runs. There is no separate architecture report to write.

Select one production implementation using these results. Retain the shared correctness tests and useful benchmarks, then remove losing experimental implementations and dependencies. A concise code comment can explain a surprising choice. Keep this comparison to one workload harness and small candidate adapters; do not build three engines or retain a universal storage abstraction.

**Tests and benchmark:** All candidates produce the same logical results under contention and parallel evaluation. The selected implementation runs headlessly and supports the batch and transaction boundaries in [performance](performance.md). Full-game budgets will be tested as real mechanics replace synthetic work; this initial benchmark cannot prove late-game performance.

## 1. Deterministic concurrent simulation kernel

Extend the API, simulation, and CLI workspace boundaries in [client interface](client-interface.md). Add typed IDs, fine-grained integer time, rules, scenario construction, commands and choices, action state, deterministic event sequencing, committed observations, and the advancement contract in [simulation](simulation.md).

Implement a one-worker reference mode and bounded concurrent execution from the outset. Establish phase dependencies, consistent read views, owned output buffers, deterministic ID assignment and randomness, and shared-resource commit rules. Include due-work queues, indexes, profiling counters, and a benchmark runner. Do not defer concurrency to the late-game stage.

Extend the first coding task rather than starting a second engine. Keep the guardians' confirmed physiology distinct immediately. Add versioned save/load of committed state here so later features extend a tested persistence path; choose the encoding as an engineering decision. Keep scenario construction separate from storage, with a stable scenario ID and seed, so multiple selectable starts can be added as playable content becomes available.

**Tests:** Invalid commands leave state unchanged. Time chunking, worker counts, and perturbed completion orders produce equivalent canonical state and events. IDs never repeat. Simultaneous claims cannot double-consume goods. Save/load followed by continued execution matches an uninterrupted run. Commands, pause, and queries remain responsive during accelerated advancement; the benchmark measures CPU and memory costs.

## 2. Physical survival, work, and transport

Add physical storage, carried inventories, capacity, needs, rest, food consumption, spoilage, health, injury, mortal death, travel, loading and unloading, and basic production. Start with food and timber plus explicit test tools and inputs as needed; introduce seed when crop phases arrive.

Use existing accessible food in survival fixtures so lawful access and sharing can be tested independently in stage 4. Implement physical attempts and access hooks without assuming every person has a universal entitlement.

**Tests:** People survive with supplies they can reach and consume; inaccessible abundance does not prevent deprivation. Travel and work take actual time. Interruptions preserve cargo and consumed inputs. Guardians do not starve and recover from the fatigue and injuries modeled here. Bodily destruction and regeneration are completed in stage 7.

## 3. Autonomous people and knowledge

Implement personality, values, directed relationships, local observation, reports, skills, utility scoring, commitment, goals, template-based plans, and plan revision. Add household coordination, dependents, caregiving, development, aging, and births.

Begin with a small action and goal repertoire, then extend through later stages. Candidate generation must include self-directed action, not only advertised employment.

Add learned attachments to modeled practices, groups, and ideals, with separate shared concept definitions and individual values. Test that learning from an experience changes a later choice with the physical alternatives held constant. Broader proposal generation and precedent reasoning follow in stage 6.

**Tests:** People with different needs, beliefs, knowledge, relationships, and goals make explainable different choices. They revise failed plans, care for dependents when motivated and able, and do not oscillate between actions without reason. NPCs cannot use an unknown fact until they observe it or learn it through communication; normal player inspectors cannot expose NPCs' private reasoning. Stale knowledge can produce a failed attempt and subsequent learning.

## 4. Property, bargaining, and social conflict

Add claims, custody, permissions, sharing, requests, offers and counteroffers, agreement state, labor terms, accrued obligations, delivery, compliance, default, theft, intimidation, reporting, and a bounded initial conflict resolver.

Resolve basic injury and coercion before representing enforcement as a completed system. A minimal conflict fixture can prove data flows before tactical design is settled, but does not constitute completed conflict gameplay.

**Tests:** Shared, exclusive, and coerced access arise from configured people and arrangements rather than a selected government mode. Theft preserves quantities and can remain undiscovered. Default creates unresolved obligations rather than automatic payment. A stronger person can still face collective resistance.

Exercise interference with an existing reservation: a physical taking or destruction changes stock once, invalidates the affected reservation, and reaches the waiting carrier as information only through observation or communication.

## 5. Projects and a complete enterprise

Add land and use claims, input recipes, procurement, financing in kind, recruitment, deliveries, construction, crop phases, environmental inputs, resource depletion and renewal, production attribution, accounts, wages, output shares, maintenance, and enterprise continuation or failure.

Implement the [farm scenario](economy.md#first-complete-farm-example) under several starting arrangements. Include a private farmer, cooperative contributors, and a coercive sponsor in separate scenarios; these test the same mechanics with different decisions.

**Tests:** An autonomous citizen can identify an opportunity, negotiate support, obtain real resources and labor, complete or fail a project, and operate an enterprise. A rejected proposal transfers nothing. Shortages, unmet wages, blocked deliveries, spoiled supplies, and disputed ownership have observable consequences.

This is the first complete economic milestone. Use playtesting and behavior tests to expose unintended stable outcomes before extending its scale.

Once production works, revisit whether to add technological invention within a concrete domain such as farming or metallurgy. That capability remains deferred; ordinary production and learning do not depend on it.

## 6. Institutions and political continuity

Add institution founding, membership, offices, decision procedures, policies, disputes, adjudication, funded enforcement, delegation, public finance, succession rules, opposition, and removal from power.

Connect requests and directives to actual communication and authority. Add mayors and governors as mandate-holding people rather than remote player-controlled interfaces.

Implement [bounded social invention](society.md#bounded-social-invention): problem-driven proposal construction, adoption or refusal, amendment from observed consequences, and judgments using structured precedents. Keep candidate search and case retrieval bounded and deterministic; extend the shared benchmark with proposal bursts and growing concept/case history.

**Invention tests:** An NPC proposes an executable arrangement absent from the authored institution list; participants adopt or reject it and their resulting actions affect real goods, duties, or authority. A known relevant precedent changes a judgment about a new case, while an unknown case cannot influence it. Save/load and worker-count changes preserve the resulting concepts, values, rules, and decisions. New names or prose alone do not satisfy these tests.

**Tests:** People can establish and change arrangements without the player creating every institution. Unsupported laws are ineffective. An enforcement order can fail. Deposition removes practical powers while preserving direct character control and persistent world state.

## 7. Full reincarnation and childhood loop

Add lineage history, selection from actual post-death births, the guardians' innate location connection and travel to the successor, player control of the newborn, spectating, upbringing, mentorship, and possible return to political life. Let guardians assess readiness for the adult relationship and evaluate or refuse orders while retaining loyalty and unwavering dedication to their mission.

Implement guardian recovery through bodily destruction, regenerating in place without escaping effective imprisonment. Integrate estates, institutions, other households, and agreements during the entire transition; do not freeze the realm or force a birth to ensure succession.

**Tests:** Play a full death, eligible birth, guardian journey, upbringing, and readiness sequence. Recognition does not restore office or property. Test a world with no eligible births, a child dying before the guardians arrive, guardians unable to reach an instinctively located successor until adulthood, regeneration inside a prison, and refusal of an adult order without abandonment of the mission. World changes and actual developmental history persist throughout.

## 8. Past-life access and costs

Add preserved knowledge and capabilities, selected active lives, combination effects, duration-dependent burden, recovery, all selected cost types, explicit previews and warnings, scoped consent, and the rare permanent invocation penalties of very prolonged use. Implement committed loss-of-control intervals with autonomous behavior and no player dismissal or protective consequence caps.

**Tests:** Useful regular use is possible. More lives and longer use increase costs. While the player retains control, fast-forward stops before unaccepted costs and dismissal prevents a newly warned cost. During accepted loss of control, new costs and harmful outcomes proceed without another consent stop; player actions and dismissal remain unavailable. Save/load and different advance sizes preserve the interval and its outcomes, and ordinary warnings resume afterward. Test government, teaching, labor, and social interaction here; exercise the same contracts with full combat in stage 10.

## 9. Wider economy and realm

Add multiple settlements, trade routes, markets, accepted currencies, credit and custody institutions, migration, regional scarcity, provinces, mandates, diplomacy, and external societies.

Extend transport and information delivery before enabling distant management. Extend the performance fixtures already maintained since stage 1 to multi-year and regional activity; preserve individual identity and defined causal behavior. Ten thousand people on a typical desktop is the late-game target.

**Tests:** Supply disruptions, prices, travel, reports, and local decisions produce regional differences. Institutions function during ruler absence and childhood. A trade route's loss affects real participants and goods.

## 10. Complete missions and warfare without graphics

Implement turn-based combat with fixed time per shared round and procedurally generated maps with continuous positions, resolving the remaining initiative, action, movement, and attack rules first. Add party preparation, field orders, expeditions, recruitment, units, logistics, large conflicts, capture, retreat, and persistent mission consequences. Replace early conflict fixtures with the full resolver while preserving social and physical contracts.

Expose tactical positions or engagements, available actions, timing, and results through the API and a text client. Extend the existing text interactions with party and combat orders, and integrate management, negotiations, and cost warnings during missions. A graphical interface must not be needed to choose an action or resolve its consequence.

**Tests:** The party leaves a functioning realm, resolves a mission entirely headlessly, and returns to persistent consequences. Death and defeat continue through the appropriate systems. Normal client actions express actual available authority and capability rather than debug powers. Mixed combat and economic workloads meet their agreed latency and throughput budgets.

Test generated-map persistence, damage carried back to the world, round time independent of participant count, concurrent encounters sharing one clock, timely reinforcements under batched advancement, and warning/save/load interruption within a round. Combat actors cannot simultaneously supply ordinary workplace labor.

## 11. Connected campaign play

Implement open-ended sandbox continuation and multiple player starting scenarios, with explicit selection and seeded random selection. Offer only scenarios supported by implemented mechanics. Test invalid configuration rejection and reproducible random selection. Scenario content can grow as systems become playable. Loss of office or a realm need not end play; support succession gaps without generating replacement children.

Preserve room for later story conditions and long-term world changes through the existing state, rules, event scheduling, and persistence boundaries. Actual overarching story content is deferred. Do not add a generic narrative framework as a prerequisite for the sandbox.

Write connected tests spanning livelihoods, institutions, missions, deposition, death, birth, upbringing, and subsequent incarnations. Check adverse cases as well as successful paths. Save/load and replay must work across those transitions. Resolve remaining questions before implementing their dependent behavior.

Exercise the renderer-independent contract through API integration tests and the CLI. Vary observer frequency, advancement chunking, and client attachment while replaying the same commands. Verify actionable choices, knowledge-filtered state, movement/combat observations, and correct stopping at warnings.

**Tests and benchmarks:** The complete mechanics run without windows or a GPU, support approximately 10,000 individual people within the measured desktop budgets, and expose everything required by a later graphical client. No foundational rule remains implemented only in a test fixture, placeholder, or presentation callback.

The next integration project can add a graphics engine, visual assets, animation, and graphical interaction using this boundary. Those additions do not own combat, simulation time, citizen reasoning, or other game rules. The API, its tests, and the working CLI are sufficient integration examples; do not create a graphics handoff package.

## Saves, performance, and validation throughout

Extend the kernel's save/load tests as features add state: pending actions, contracts, cargo, knowledge, search state, regeneration, burden, consent, and warnings. A loaded world must continue like an uninterrupted world.

Maintain performance testing at every stage under [performance](performance.md), including worker-count equivalence, shared-resource contention, responsiveness, and memory growth. An ECS or multithreaded implementation must earn its speedup in the complete workload. Optional GPU implementations must also pass CPU-equivalence tests and show an end-to-end benefit.

Use three test levels:

- Small domain checks for conservation, access, progress, state transitions, death, and warnings.
- Deterministic integration scenarios for complete causal chains and adverse cases.
- Longer behavior and performance runs comparing distributions, bottlenecks, and unintended stable outcomes.

Do not assert exact citizen action counts when a behavior test only needs a directional effect. Use controlled fixtures to compare changed incentives or beliefs. Keep exact replay assertions for determinism and transaction correctness.

Keep fixture rules and seeds in test/benchmark code; print the seed when a generated case fails. Store no manually maintained acceptance matrix or copied result summaries. Tests should exercise a meaningful behavior and failure case, not mirror every internal helper. Extend existing scenarios and benchmarks where that proves a new feature; no separate test file, benchmark suite, or report is required for each feature.

Tune rates and thresholds only after the related semantics are stable. The supported late-game target is approximately 10,000 individuals on a typical desktop; refine the provisional hardware and throughput budgets against measurements and the pacing decision.

## CLI and inspectors

Proposed command groups:

```text
status / people / person <id> / areas / routes
goods / storage <id> / claims / shipments
tasks / goals <person> / projects / enterprises
offers / agreements / obligations / disputes
institutions / offices / policies / relationships
lineage / guardians / past-lives / burden
events / explain <person-or-record>
run <duration> / pause
act <ruler-action> / order <guardian> <action> / cancel <intent>
request ... / offer ... / counter ... / accept ... / reject ...
invoke <life-ids> / dismiss / acknowledge-cost <warning-id>
save <path> / load <path>
```

Exact grammar is an implementation choice. Resolve stable IDs rather than relying on names being unique. Debug-only mutation commands must be explicitly separated from player commands and excluded from ordinary play.
