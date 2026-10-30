# Game design and implementation plan

The goal is a complete, playable sandbox simulation of people, societies, and a reincarnating ruler, with graphics deferred. Implement the agreed mechanics in a modular Rust backend with a CLI and a public interface that a graphics engine can use later. Preserve room for an overarching story or long-term world change later. The original settlement prototype becomes the first economic milestone rather than the project's final scope.

**Readiness:** Ready to begin implementation. The kernel, physical simulation, and performance comparison have a concrete starting task below. The full game still needs answers about combat, reincarnation details, social procedures, and other player-facing rules. Ask about those when their dependent work approaches; continue independent implementation. Do not silently turn an unanswered question into a game rule.

## Start here

1. Read this overview, then [the first coding task](design/implementation.md#first-coding-task). The repository currently contains design documents only; create the Rust workspace there described.
2. Implement the small executable fixture, commands, advancement, and tests in that task. Start with simple typed storage as a comparison baseline, and introduce bounded parallel execution in the same initial slice.
3. Compare Bevy ECS, Legion, and custom storage with that runnable workload before expanding the production engine. Keep one implementation after the comparison; retain useful tests and benchmarks.
4. Continue through the [implementation stages](design/implementation.md#1-deterministic-concurrent-simulation-kernel), reading the relevant domain document as each feature is added. The [open questions](design/decisions.md#when-to-ask) identify which later behaviors need discussion.

Do not begin by generating more plans, a generic engine framework, or scaffolding every future system. The first result should be runnable code with a test that exercises a real state transition.

## Working agreement

Code and executable tests are the only source of truth for implemented behavior and completion. These documents explain design intent and guide future work; they are not a second implementation specification or progress ledger. Preserve the user's confirmed requirements when fixing disagreements between intent and code.

Prove an acceptance criterion with a test exercising the feature's behavior, through the public API when appropriate. Shared integration scenarios can cover several criteria. Performance claims belong in runnable benchmarks. No separate handoff artifacts, acceptance reports, evidence bundles, sign-off checklists, or per-feature documentation updates are required. Add a short code comment when a non-obvious choice needs explanation; update these notes only when that helps future work.

## The game loop

The player lives as one incarnation of the ruler. They work, learn, build relationships, invoke earlier lives, and try to influence or govern a society of autonomous people. Their influence depends on ability, resources, relationships, institutions, and recognition; people can reject or depose them.

The realm supplies the ruler and companions with food, equipment, knowledge, labor, and allies. Their personal actions and missions change the realm's people, resources, safety, trade, and politics. Time continues for both modes within one persistent world.

When the ruler dies, a successor can be chosen from subsequent births. Without new births, there is no new ruler. The player controls the successor from birth, spectates the guardians' journey toward the instinctively located child, and influences their upbringing through interaction and choices. Guardians decide when the ruler is ready to leave mentorship. Society continues during the intervening years.

## Confirmed design commitments

- Citizens are individuals with needs, skills, beliefs, personality, relationships, dreams, goals, and plans. Ordinary citizens decide whether to comply with requests and obligations.
- Players choose among multiple starting scenarios or request random selection. Individual scenario contents can be developed as their mechanics become playable.
- Ordinary player views show the ruler's knowledge and received reports. Guardian-search spectating is an explicit exception; observing does not give NPCs extra knowledge.
- Government, food access, property arrangements, employment, and other social patterns emerge from people's individual behavior. Sharing, bargaining, refusal, theft, coercion, and enforcement all belong in the design; cooperation can result from fear or obligation as well as willingness.
- Social invention uses bounded personal models: people develop practices, acquire values grounded in modeled experience, and reason from structured precedents. Institutions are descriptions of recurring individual behavior, not simulation entities or predefined rule systems. Agreements are personal records and do not compel compliance. Technological invention is deferred for evaluation after production exists.
- Goods have physical locations and must be carried or delivered. Ownership, custody, access, and possession are separate.
- The player controls the ruler, except during an explicitly accepted loss of control. Guardians are loyal to each ruler and unwaveringly dedicated to their mission across lives, but may disagree, warn, and refuse orders. They decide readiness to leave mentorship.
- Guardians locate the ruler through an innate connection. They are immortal and do not suffer hunger; injuries and even destroyed bodies recover with time. Regeneration occurs in place and does not defeat imprisonment.
- Past-life use is a deliberate tradeoff involving physical strain, identity effects, emotional instability, lifespan loss, and temporary loss of control. While the player retains control, warn before a new cost and offer dismissal or acceptance. Accepted loss of control allows all ensuing consequences for the warned duration, without an escape or protective cost cap.
- Regular past-life use should be viable. Direct permanent invocation penalties are very rare and reserved for very protracted use; actions during loss of control can still cause ordinary lasting harm.
- Named areas and travel times represent geography; exact movement across a tile grid is not a requirement.
- Combat is turn-based on procedurally generated maps with continuous positions. Each shared round advances a fixed amount of world time; the strategic world retains its area-and-travel model.
- Access to past-life information requires active invocation. Invocation does not automatically copy memories into the current life; deliberate notes and teaching can preserve information through ordinary means.
- The mechanics must run headlessly, including combat, player choices, reincarnation, and saves. The graphics engine is the deferred part, not a prerequisite for completing these systems.
- Target approximately 10,000 individually simulated people in the late game on a typical desktop. Concurrency and performance testing begin with the simulation kernel.
- Keep numeric compute separable so selected workloads can use optional GPU acceleration later. GPU hardware must not be required for the complete game mechanics.

## Design documents

| Document | Purpose |
| --- | --- |
| [Decisions and open questions](design/decisions.md) | Confirmed intent, remaining questions, and when each needs an answer. |
| [Characters and decisions](design/characters.md) | Needs, health, development, households, personality, knowledge, goals, and autonomous action. |
| [Society and government](design/society.md) | Social beliefs, property recognition, institutions, offices, laws, enforcement, coercion, delegation, and political continuity. |
| [Economy and work](design/economy.md) | Physical goods, storage, hauling, labor, production, agreements, trade, projects, and enterprises. |
| [Ruler and guardians](design/ruler-and-guardians.md) | Reincarnation, player continuity, upbringing, recovery, past-life powers, and informed acceptance of costs. |
| [Realm and missions](design/realm-and-missions.md) | Management, places, expeditions, combat requirements, diplomacy, information, and interface responsibilities. |
| [Simulation architecture](design/simulation.md) | Backend boundaries, identities, time, commands, actions, determinism, conservation, events, and persistence. |
| [Performance and concurrency](design/performance.md) | Desktop targets, ECS/custom-system evaluation, data layout, parallel execution, deterministic transactions, benchmarks, and optional GPU compute. |
| [Client and graphics interface](design/client-interface.md) | Complete headless play, public commands and observations, responsive session control, choices, and a later graphics engine. |
| [Implementation stages](design/implementation.md) | First coding task, build order, dependencies, feature tests, inspection, and benchmarks. |

Detailed documents describe planned behavior. Their acceptance scenarios are test ideas to implement alongside each feature, not checklists to maintain separately from the tests.

## How the systems connect

**A citizen establishes a livelihood:** A personal goal leads to a plan. The citizen identifies land, inputs, tools, and help; negotiates access and support; arranges labor and deliveries; and attempts a project. If it succeeds, an enterprise maintains production and obligations. Success, failure, cooperation, and default change relationships and later decisions. A private farm is one possible arrangement, alongside household, communal, governmental, and other forms.

**A society develops institutions:** Repeated cooperation, disputes, exclusion, and force create patterns that can be described as institutions. Each person's choices, relationships, expectations, and actions explain whether a practice persists or changes. Fear, obligation, and enforcement can influence cooperation, and anyone can abandon a promise. There is no institutional entity that creates authority or compels behavior.

**The ruler leaves on a mission:** Actual people and goods leave local work and defense. Delegates and citizens continue making decisions. Travel and encounters consume time and supplies; losses and achievements persist in the same economy and political world. Returning does not restore the pre-mission state.

**The ruler dies:** The individual dies, while institutions, property disputes, obligations, history, and the reincarnation lineage continue. The guardians seek a later-born successor. The player experiences that child's life while other people build the intervening history. Recognition by the guardians does not automatically grant the new person political office or their predecessor's assets.

Succession uses actual births; no new child is created solely to guarantee a successor. A gap without a ruler can therefore last indefinitely.

**The ruler invokes past lives:** Chosen lives supply knowledge or capability while building burden over time. While the player retains control, a new cost prompts a warning and a choice before time advances through it. Accepting loss of control explicitly suspends that protection for the warned duration: autonomous behavior and its consequences proceed. Ending invocation when control permits it prevents further exposure but does not erase consequences already incurred.

## Engineering direction

Separate the public API, authoritative simulation, session host, and clients. Use typed IDs, integer time and quantities, deterministic operations, and explicit transactions. Evaluate standalone Bevy ECS, Legion, and a custom game-specific system against the same representative workload before choosing storage and scheduling. The original standard-library-only restriction does not govern the full-game architecture.

Workers evaluate independent batches against consistent views; shared interactions resolve before committed state is published. Store frequently processed data compactly, index sparse relationships and opportunities, schedule work when due, and measure contention and serial commit costs. Correctness must hold across worker counts and time-advance request sizes.

Keep pure numerical batches behind small compute interfaces with a CPU implementation. GPU compute can later implement suitable batches without taking over world ownership or requiring a rendering engine. Adopt acceleration only when complete timings, including transfers and coordination, show a benefit.

Physical rules apply to everyone, with explicit exceptions for physiology and abilities. Social permission is distinct from physical possibility: an unauthorized taking can be attempted, while a lawful agreement cannot teleport goods or compel a person without a mechanism.

Build small deterministic scenarios first. The first complete economic milestone includes autonomous goals, negotiated support, actual labor, carrying and delivery, production, obligations, default, and consequences. Then extend institutions, the full life cycle, powers, the wider realm, and missions according to the [implementation stages](design/implementation.md).

Completion means all mechanics are playable and tested through the headless interface at the target scale. Temporary fixtures and unresolved mechanics cannot be passed off as complete because graphics is deferred. The [implementation stages](design/implementation.md) cover the required systems; passing feature tests and runnable benchmarks establish what works.

## Next design work

The sandbox direction, starting-scenario model, ordinary knowledge boundary, bounded social invention, broad combat model, and core guardian/succession rules are settled. No further scope decision is needed before the first coding task. Detailed social procedures, setting and scenario content, successor selection among eligible births, childhood pacing, past-life behavior, and tactical action rules can be resolved with their supporting systems. The [question timing](design/decisions.md#when-to-ask) states when each answer is needed and which work can proceed first.

Numeric tuning follows those choices. We can build the deterministic foundation while discussing them, but should not conceal unresolved gameplay behind arbitrary defaults.
