# Realm management, missions, and the wider world

**Status:** The two connected modes, shared world, turn-based combat with fixed time per shared round, and generated maps with continuous positions are confirmed. Detailed tactical rules remain open in [decisions](decisions.md).

## Places and political geography

The physical world contains regions, settlements, sites, and named areas connected by travel links. Political provinces are administrative jurisdictions that can be created or changed without moving the land itself.

Settlements need population, places to live and work, supply access, relationships with nearby groups, and institutions that actually govern them. Settlement founding is a project involving transport, shelter, protection, and negotiated or contested land access.

Areas can be discovered, settled, abandoned, damaged, restored, or controlled by different groups. Store changes in the shared world rather than making a separate mission-only copy of the same farm or mine.

## Management through people

The realm interface summarizes known conditions, petitions, proposals, offices, institutions, stores, projects, and external relationships. It offers actions available through the ruler's current position and relationships.

Setting a food priority means communicating an objective, offering terms, allocating authorized resources, or directing agents who accept those instructions. It does not directly alter every citizen's utility with no social channel.

Appointments create or fill real offices according to an institution's procedure. Delegates receive mandates and make their own choices. Reports can be delayed, incomplete, mistaken, or dishonest according to their production and communication.

During ruler absence or incapacity, decisions continue through existing mandates, institutional rules, household arrangements, and opportunistic action. Unanswered petitions can wait, be handled by a delegate, expire, or create consequences. The simulation never requires an unavailable infant to approve every transaction.

## Expedition and mission lifecycle

A mission has an objective, sponsor, participants, destination or route, supplies, equipment, authority, expected risks, fallback plans, and completion or failure conditions.

Preparation obtains participants' commitment, provisions, transport capacity, equipment, and information. Leaving removes those actual people and goods from their previous activities. Delegation and substitute labor can mitigate that absence at real cost.

Travel consumes time and supplies. Encounters create decisions using the existing movement, social, knowledge, injury, and economic systems. Outcomes change persistent places, people, property, agreements, trade access, and political relationships.

Mission failure need not destroy the party or end the game. It can produce retreat, capture, lost goods, delayed harvest, debt, changed loyalties, or contested territory. Guardian recovery and ruler reincarnation provide continuity without reversing those losses.

## Player control and party behavior

The player chooses the ruler's actions within current capability, except during accepted loss of control. Guardians judge when the ruler is ready for the adult command relationship; during childhood the player influences their decisions through interaction. Guardians can disagree, warn, or refuse adult orders while remaining loyal to the person and unwaveringly dedicated to the lineage's mission.

Other companions remain people with commitments, beliefs, needs, and limits. Recruitment, service, wages, loyalty, and orders use the social systems. A mission participant can refuse, defect, flee, or seek better terms where those behaviors are physically possible.

Combat uses turns within shared rounds. Each round advances a fixed simulation duration, independent of participant count. Choosing actions does not advance world time. Initiative, action budgets, reactions, and exact round duration remain to be designed and tuned; choosing turn-based combat does not settle whether actions resolve sequentially or simultaneously.

The surrounding world reaches the same logical time as combat. Work outside the encounter may be batched when equivalent results can be established. Reinforcements, deliveries, weather, deaths, messages, and other cross-boundary effects must be resolved before they can affect an encounter or a published observation. Do not simply freeze the realm and apply all consequences after the battle. Other encounters share the same clock rather than each adding their duration to it.

## Conflict and combat

Conflict must support one-on-one threats, theft resistance, guards, skirmishes, sieges, and armies. Reuse participants, equipment, injury, fatigue, morale, supply, leadership, and objectives at each scale.

Generate a local combat map with continuous positions, terrain, obstacles, reach, cover, and escape routes. Strategic travel continues to use the area graph. Use deterministic fixed-point coordinates and distances; continuous placement does not require floating-point authoritative rules or a tile grid. The exact movement, visibility, and attack rules remain to be designed.

A combat map refines a place in the existing world. Its entrances and exits connect to actual routes, and its participants, goods, and relevant structures reference existing records. Generate unspecified detail from stable seeds and versioned rules. Preserve established features and lasting damage; entering or reloading a battle must not reroll a destroyed wall, duplicate supplies, or restore a dead participant. Map generation and geometry work without a renderer.

Combat participants spend their action time in the encounter and cannot also perform their previous economic work. Health, fatigue, supplies, and past-life burden continue according to elapsed world time. A required cost warning can interrupt round resolution; save and resume its remaining work without repeating an action, random draw, or time interval.

The authoritative spatial and timing model belongs to the backend. Text commands and queries must support a complete encounter. The future graphics engine visualizes these facts and submits intentions; animation or graphical collision callbacks cannot become the only implementation of attacks, movement, or damage.

Damage affects bodies and capability. Recovery uses the health system, with guardian regeneration as an explicit exception. Mortal death, capture, and retreat leave persistent economic and social effects. Past-life use follows the same warning and consent rules during combat as during ordinary work.

Armies are organized groups of individual people with command structures, training, equipment, provisions, and transport. A unit may resolve coordinated actions as a group for performance, while individual casualties and important participants remain attributable. Recruitment cannot create soldiers from an abstract number without people, and armies cannot operate indefinitely without supplies.

## Diplomacy and external societies

Neighboring societies use the same claims, agreements, institutions, and enforcement rules. Diplomacy includes recognition, passage, trade, protection, tribute, migration, alliances, grievances, and war settlements.

Relationships occur between particular people and institutions, not only through one friendship score per realm. Leadership changes and reincarnation can alter diplomacy without resetting institutional memory or existing obligations.

Different property customs can create disputes over land, trade, and migrants. Treat these as differing recognized rules rather than forcing every society into the player's institutions.

## Information and communication

Maps and management summaries distinguish observed facts, reports, estimates, and unknowns. Knowledge should have source and age when that matters to a decision. A stolen convoy can be missing before the government knows why.

Normal player views use the ruler's knowledge and received reports. An unknown distant event must not leak through maps, totals, explanations, or notifications. The player may spectate the guardians' search as an explicit exception; details of that view will be settled with the search mechanic. Spectating does not transfer knowledge to the ruler or NPCs. Omniscient inspectors remain debug-only.

## Interface requirements

The player needs to inspect known activities, reported plan blockers, resource locations, claims, access, and unfulfilled obligations. Mark estimates, reports, and unknowns rather than exposing hidden inventories or private intentions. Show alternatives through concrete known people, offers, routes, and actions.

Provide notifications for decisions requiring the ruler's participation, danger, death and birth, significant political changes, and impending past-life costs. While the player retains control, required cost warnings interrupt accelerated time before the cost. Accepted loss of control follows [the warning contract](ruler-and-guardians.md#warning-and-consent-contract): consequences continue for its duration without renewed consent or player dismissal, including during combat.

## Sandbox and later story

Begin with an open-ended sandbox. Loss of office, a ruined realm, and unavailable guardians are persistent situations rather than mandatory campaign endings. A lineage without eligible new births can remain without a living ruler; the simulation must not invent one to keep the campaign moving.

Leave room for an overarching story through persistent world conditions, scheduled events, and ordinary effects on environment, resources, people, and institutions. A future dying-star scenario could use those systems. Such changes must survive save/load and reach characters through their knowledge channels. Add the actual story and any needed domain mechanics later; the initial engine does not need a generic narrative framework.

The CLI supplies complete normal player interactions as well as explicitly separate omniscient debug inspectors. Follow the [client interface](client-interface.md) for commands, actionable choices, observations, advancement, and responsive pause. Graphical layout and rendering are deferred; none of the underlying choices or consequences are.

## Acceptance scenarios

- A party departure removes its members and supplies from local work and defense.
- A raid's damage, losses, and survivors persist in the settlement economy.
- Governors continue making decisions during travel and childhood according to their actual mandates.
- A distant crisis reaches the ruler through a communication path rather than instant NPC omniscience.
- An unreported distant crisis stays absent from all normal player views until information arrives; stale reports remain identifiable as reports.
- Capture, deposition, or defeat can leave a living player character with meaningful options.
- A newly grown incarnation can encounter institutions and consequences created during their absence.
- Management, negotiation, childhood, missions, and combat can all be played through normal headless commands and choices.
- A client that queries or renders more frequently receives observations without changing simulation outcomes.
- The same number of completed rounds advances the same world time with different participant counts; two concurrent encounters do not double elapsed time.
- A reinforcement due during combat arrives at the correct boundary under both ordinary and batched world advancement.
- Saving during a round preserves the generated map, resolved actions, remaining interval, and pending warnings; returning to the place preserves lasting damage and losses.
