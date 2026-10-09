# Decisions and questions for the design discussion

These notes distinguish confirmed intent from questions about future work. They are not a completion ledger: code and tests establish what is implemented. An unanswered question is not an approved mechanic.

## Confirmed in the discussion

| Topic | Decision |
| --- | --- |
| Scope | Implement the complete game mechanics in stages, playable headlessly. The graphics engine is deferred; other mechanics are not omitted because a renderer is absent. |
| Document structure | Keep all design documents under `docs/`, with `plan.md` as the entry point and detailed topics under `design/`. |
| Implementation evidence | Code and executable tests are the only source of truth for implementation and completion. Prove acceptance criteria with feature tests; require no handoff artifacts or separate acceptance reports. |
| Starting scenarios | Provide multiple starting scenarios. The player can choose one or request random selection; there is no single mandatory opening. |
| Player information | Ordinary play exposes the ruler's knowledge and received reports. Spectating the guardians' search is an explicit exception; NPCs still act on their own knowledge. |
| Modularity | A graphics engine must plug into the completed backend through public commands, choices, and observations. Authoritative rules must not depend on rendering. |
| Scale | Target approximately 10,000 individually simulated people in the late game on a typical desktop. |
| Concurrency | Design and measure CPU concurrency from the first simulation milestone, not as a late rewrite. |
| Future compute | Preserve the option of GPU compute through batch interfaces and suitable data layouts; actual GPU implementation can come later. |
| Storage and scheduling | Evaluate standalone Bevy ECS, Legion, and a custom system tailored to this game. No candidate is selected solely by its general reputation or a simple iteration benchmark. |
| Society | Government and food access emerge from people, beliefs, arrangements, and enforcement. Do not impose one universal property or sharing policy. |
| Social scope | Include sharing, refusal, employment and other labor arrangements, agreement compliance, theft, and coercion; do not stop at a minimal voluntary economy. |
| Social invention | Include invented institutions and rules, learned values grounded in modeled experience, and bounded reasoning from structured precedents. These operate within represented facts and actions; unrestricted human-like reasoning is outside scope. |
| Technological invention | Defer the decision until production exists, then evaluate a bounded domain. Ordinary production, recipes, and learning remain in scope. |
| Logistics | Include carrying and delivery. Resources must reach their destination physically. |
| Mortal ruler | The ruler can die and reincarnate. Do not substitute recoverable starvation for this lifecycle. |
| Successor | Choose from actual children born after the previous ruler's death. If there are no new births, there is no new ruler. |
| Guardian connection | Guardians can locate the ruler instinctively through an innate connection. Reaching them still requires physical travel and access. |
| Player continuity | The player controls the newborn successor and can spectate the guardians as they search. They do not take over a regent or the guardians as a substitute player character. |
| Upbringing | Players influence their upbringing. Guardians decide when the ruler is ready to leave mentorship and enter the adult command relationship. |
| Guardian agency | Guardians are loyal to each ruler and unwaveringly dedicated to finding, mentoring, and assisting the lineage. They can disagree, warn, and refuse orders. |
| Political authority | People can reject or depose the reincarnating ruler. Their political power is not guaranteed. |
| Guardian physiology | Guardians do not suffer hunger. They can tire, be injured, or become incapacitated, and always recover with time. Destroyed bodies regenerate in place; imprisonment can contain a recovered guardian. |
| Past-life costs | Include physical strain, identity loss, emotional instability, reduced lifespan, and temporary loss of control. Severity increases with duration and number of lives invoked. |
| Informed tradeoff | While the player retains control, warn before a new past-life cost and offer dismissal or acceptance. Accepting loss of control explicitly accepts uncontrolled consequences for that interval. |
| Regular use | Past lives should be usable regularly. Direct permanent invocation penalties are very rare and limited to very protracted use; ordinary consequences of uncontrolled actions are not capped. |
| Past-life information | Information requires active invocation and is not automatically copied into the current life. Deliberate notes and teaching can preserve it through ordinary means. |
| Loss of control | The warned duration is binding. Further costs and outcomes proceed without exceptional dismissal, renewed consent, or protective cost caps. |
| Combat time | Turn-based combat; each shared round advances a fixed amount of world time, independent of participant count. |
| Combat space | Procedurally generated maps with continuous positions. The strategic simulation retains named areas and travel links; people, goods, and lasting consequences remain shared. |
| Campaign | Start with an open-ended sandbox. Preserve room for a later overarching story or long-term world change without requiring that content now. |

[Lore](../lore.md) describes the premise and the two connected modes. The detailed documents incorporate subsequent clarifications.

## Proposed foundations

These ideas make the confirmed requirements concrete, but remain open to revision in discussion:

- Social outcomes come from supported actions and institutions, not a global government-type switch.
- Physical possession, ownership claims, access, custody, and obligations use separate records.
- Beliefs and relationships affect reactions; knowledge travels through witnesses and reports.
- Households, enterprises, governments, guilds, and other organizations use common assets, agreements, and decision procedures.
- Compliance and enforcement require actual behavior; future obligations are not automatically extracted from private inventories.
- Citizens start with utility decisions and explicit goal/plan templates, then gain the bounded social invention and reasoning capabilities described in [society](society.md#bounded-social-invention).
- The ruler's lineage, individual life, political office, and property remain distinct.
- Invocation uses accumulating burden with recovery and warnings at new cost boundaries while the player retains control.

## When to ask

Resolve questions in connected groups rather than collecting arbitrary answers to every parameter at once. The timing below concerns behavior that needs an answer, not a formal approval process for an entire stage.

These are the latest points at which answers are needed. High-level choices can be settled earlier when their tradeoffs are clear; rates, pacing, algorithm quality, and performance choices benefit from working examples.

The [first coding task](implementation.md#first-coding-task) has no outstanding gameplay prerequisite. It uses explicit test fixtures. Begin that work without waiting for final campaign content or a complete rules language.

The opening model, ordinary player information, and scope of bounded social invention are settled. No further scope discussion is needed to begin the first coding task. Detailed social behavior can be developed once needs, knowledge, and agreements can exercise it.

| Resolve before | Decision | Work that can proceed first |
| --- | --- | --- |
| Full social procedures, stages 4–6 | D7: bargaining, inheritance, sanctions, adoption, amendment, and adjudication semantics within the settled bounded scope. | Kernel, transport, needs, local knowledge, and ordinary decisions with authored access arrangements. |
| Shipping the first player starting scenarios | D1: actual scenario contents and setting. Scenario selection is already confirmed. | Scenario construction and selection infrastructure; explicit noncanonical fixtures. |
| Full household development and child agency, stages 3 and 7 | D3: family/dependency semantics, upbringing choices, and how guardians assess readiness. Guardian discretion over the transition out of mentorship is settled. | Adult needs, skills, relationships, care actions, and generic scheduled age events. |
| Full seasonal production, stage 5 | D9: environment, calendar, crop cycles, and starting technology. | Generic production, conservation, hauling, and depletion with test recipes. |
| Adding technological invention, after stage 5 | D9: whether discovery within a concrete production domain is worthwhile and which experiments the world can represent. | Complete ordinary production and learning; technology invention is not a prerequisite for them. |
| Reincarnation and guardian recovery, stage 7 | D2 and D4: selection among eligible births, presentation while waiting, instinctive-location detail, recovery rates, and confinement/escape actions. Eligibility, instinctive location, regeneration in place, and possible refusal are settled. | Mortal death, separate lineage identity, time-based recovery, and persistent world state. |
| Search presentation and distant orders, stages 7 and 9 | D6: details of guardian spectating and communication channels. The ordinary knowledge boundary is settled. | Ruler-local views and source/time-stamped reports. |
| Past-life abilities, stage 8 | D5: benefits, combinations, identity effects, autonomous behavior during control loss, and cross-life permanence. No dismissal or protective cost cap during accepted control loss is settled. | Skills, history, generic player choices, and stopping time for required input. |
| Full coercion and tactical conflict, stages 4 and 10 | D8: initiative, action budgets, movement, attacks, force, retreat, and capture within the confirmed turn-based model. Discuss basic force when enforcement first needs it; settle larger battles when party systems exist. | Injury, travel, inventory, social reactions, and isolated conflict test fixtures. |
| Connected sandbox play, stage 11 | D10: optional objectives and presentation during indefinite succession gaps; D9: expected history scale. An overarching story remains future content. | World persistence, save/load, and connected multi-life test scenarios. |

“Can proceed” does not make a fixture the final mechanic. Ask when an unresolved rule would change observable behavior; do not block unrelated work or request approval for routine engineering. Revisit the relevant question with a small playable example or failing behavioral test when that makes the tradeoff easier to assess.

## Open design decisions

### D1. Opening situation and inherited society

**Settled:** The game has multiple starting scenarios, selected explicitly or randomly by the player.

**Deferred:** Choose the actual scenarios, their place and era, and their starting relationships, resources, institutions, and life stages as the corresponding mechanics become playable. An adult founding a settlement, an established ruler, and a child incarnation are candidates rather than an approved scenario list.

Emergence does not require people to arrive without culture. Each scenario must explicitly define its starting customs and claims; no scenario becomes a universal government preset. Scenario construction can use named test data before this content is settled.

**Needed for:** Player scenario content and narrative introduction. Setting-specific physiology and calendars belong in scenario/rules data; they do not block the kernel.

### D2. Birth, search, and recognition

**Settled:** The successor comes from actual children born after the previous ruler's death. No new births means no new ruler; succession does not force a birth or select a pre-death child. Guardians locate the ruler instinctively through their innate connection and still need to reach them physically.

**Deferred:** The selection rule among eligible births, the precision and presentation of the connection, and the player's view while no successor exists. Keep political recognition and other people's beliefs separate from the guardians' connection. Ordinary birth and travel systems can be built before these details.

**Needed for:** Successor selection, guardian travel toward the successor, and presentation during succession gaps. Birth is not guaranteed on a deadline.

### D3. Childhood, time, and autonomy

**Settled:** Guardians decide when the ruler is ready to leave mentorship and enter the adult command relationship. Their judgment is separate from physical development, legal adulthood, and political office.

**Deferred:** Age-appropriate interactions, learning and care, how guardians assess readiness and weigh disagreements, and childhood pacing. Develop these with playable examples rather than choosing every age or threshold now.

Proposed direction: let players shape interests, relationships, and learning while guardians arrange care; offer time acceleration that stops for meaningful choices and major events. Exact pacing and expected real playtime per childhood are still open.

**Needed for:** Household dependency behavior, childhood interface, mentorship, childhood pacing targets, and transition to adult guardian orders. Generic pause/advance controls and the 10,000-person target are already settled.

### D4. Guardian recovery and agency

**Settled:** Destroyed bodies regenerate in place. Imprisonment can contain a recovered guardian; recovery is not an automatic escape. Guardians have a loyal disposition toward each ruler and unwavering dedication to finding, mentoring, and assisting the lineage, while retaining the ability to disagree, warn, and refuse.

**Deferred:** Recovery durations, actions possible during impairment, concrete confinement and escape rules, and how guardians coordinate duties. Implement judgments from their knowledge, dispositions, and mission; do not replace loyalty with unconditional obedience or let an ordinary grievance erase the mission.

**Needed for:** Full recovery, rescue/capture play, and explainable autonomous guardian decisions.

### D5. Past-life capability and identity

**Settled:** Past-life information requires active invocation. Deliberate notes and teaching can preserve information through normal records and learning; invocation does not automatically copy memories into the current life.

How does access develop with age and training? How do combined lives improve capability without simply adding every former ruler's skill values? How does practice while borrowing mastery affect the current person's learning?

**Settled:** Accepting temporary loss of control commits the player to a warned duration and whatever consequences follow during it. There is no exceptional dismissal, renewed consent, or protective cost cap. The warning must explain this before acceptance; ordinary warning rules resume when control returns.

What does identity loss actually change, and what actions occur during loss of control? Which consequences belong only to the current life, and can any alter the continuing lineage?

All five cost categories are selected. Their ordering, thresholds, warning scope, recovery, and permanent effects need concrete semantics before tuning numeric rates.

**Blocks:** Ability resolution, the complete warning UI, personality changes, control loss, and permanent consequence persistence.

### D6. Information and the player's viewpoint

**Settled:** Normal player views use the ruler's knowledge and received reports. Guardians' search can be spectated. NPC decisions always use each NPC's own knowledge. Omniscient inspection is a debug facility.

**Deferred:** Exactly what search spectating reveals, what the player sees before the successor's birth, and the available communication channels, delays, and remote-order limits. Spectating alone does not deliver information to a character.

**Needed for:** Search presentation and distant orders. Ruler-local views and ordinary report handling can be built now.

### D7. Social and institutional depth

**Settled:** Implement [bounded social invention](society.md#bounded-social-invention): new executable institutions and rules, learned values, and reasoning from structured precedents. People may construct arrangements that were not individually authored. Meaning comes from modeled facts, experiences, and actions; unrestricted human-like understanding is outside scope.

**Deferred:** Develop concrete bargaining, adoption, amendment, sanctions, inheritance, guardianship, and adjudication semantics when the economy and disputes can exercise them. Select internal representations, candidate-search algorithms, and case-comparison methods through implementation and tests. Routine algorithm choices do not require another scope discussion.

The remaining design concerns which actions and procedures the model can express and how people carry them out. It must not choose a universal inheritance law, punishment scale, or adoption rule for every society. Those arrangements come from the people and institutions in play; unresolved claims remain disputes rather than triggering an undisclosed default social policy.

**Needed for:** Full social procedures in stages 4–6. Physical attempts, authored access fixtures, local knowledge, and ordinary decisions can be built earlier.

### D8. Combat and party control

**Settled:** Combat is turn-based with a fixed interval per shared round, not per individual participant's action. Combat uses generated maps with continuous positions while the strategic world retains its area graph. Independent world work may be batched only while preserving the time and consequences of interactions with the encounter.

**Deferred:** Initiative and sequential versus simultaneous resolution, action budgets, reactions, movement, visibility, reach, attacks, retreat, capture, and army command. Settle these with small tactical examples; tune the round duration and numeric costs afterward.

**Needed for:** Complete tactical input, generated-map geometry and transitions, equipment/ability behavior, and the final conflict resolver. The agreed world clock and strategic travel model can be built first.

### D9. Environment, production, and scale

Choose the first environment, year and season structure, farming cycle, ecology, key crafts, technology, and health assumptions. The late-game target is now approximately 10,000 individual people on a typical desktop. Their distribution across settlements, the age of the world, and the expected number of incarnations still affect the workload.

Technological invention is a separate deferred decision after production works. Evaluate a bounded domain such as farming or metallurgy before committing to discovery mechanics. This includes newly invented physical evidence or sensing techniques; ordinary learning and use of existing production methods proceed independently.

**Blocks:** Final recipes, resource renewal, biological rates, content progression, and representative long-history fixtures. Provisional performance budgets are already specified in [performance](performance.md); refine them against chosen pacing and actual hardware rather than leaving speed unmeasured.

### D10. Campaign direction and enduring stakes

**Settled:** Start with an open-ended historical sandbox. Deposition, the loss of a realm, or unavailable guardians do not by themselves require a game over. No eligible births can leave the world without a living ruler indefinitely. Preserve the ability to add an overarching story or long-term world change later; a dying star is an example, not selected campaign content.

**Deferred:** Optional objectives, presentation of prolonged succession gaps or population extinction, and actual story content. Do not promise a replacement ruler where the birth conditions cannot be met.

**Implementation direction:** Later narrative conditions use persistent world state, scheduled events, and ordinary domain effects, with save/load and knowledge boundaries preserved. The sandbox needs no generic story scripting engine or authored central plot before its mechanics work.

### Engineering selections to make through evidence

The ECS/custom-system choice is an engineering experiment, not a missing preference that needs repeated user approval. Build the first executable slice, compare Bevy ECS, Legion, and custom typed storage/scheduling on its workload, then keep the selected implementation and useful tests. No architecture report is required. Standard-library-only was a prototype constraint, not a barrier to useful infrastructure dependencies now.

The 10,000-person desktop target is confirmed. The six-core/16 GiB benchmark profile and latency, throughput, and memory budgets in [performance](performance.md) are proposed engineering targets to validate, not measured claims or final hardware requirements. Select GPU APIs only if a later bottleneck and end-to-end measurements justify them.

None of these selections moves simulation rules into graphics. The complete behaviors in the [implementation stages](implementation.md) must work through the headless API before graphics is the remaining integration work.

## Tuning work, not questions about intent

Once the relevant mechanics are agreed, implementation can choose and test rates for hunger, work, travel, prices, learning, burden, trust, and recovery. Do not ask for approval of every coefficient. Keep units explicit, values centralized, and scenario evidence reproducible.

## Readiness

Begin with the executable slice. The full game is not specified end to end: later questions have explicit points at which they need answers. Tests establish completed behavior; an existing record type, test fixture, or updated document cannot stand in for an unimplemented mechanic.
