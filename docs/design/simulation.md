# Simulation architecture and contracts

**Status:** These are implementation defaults for the design. They can be refined during engineering without changing the confirmed game rules.

## Backend and interfaces

Build the complete mechanics in a Rust simulation library with a CLI client and headless scenario runner. Defer the graphics engine. Follow the [client interface](client-interface.md) for workspace boundaries, commands, observations, choices, and later renderer integration.

Evaluate standalone Bevy ECS, Legion, and custom game-specific storage and scheduling using the [performance spike](performance.md#evaluate-storage-and-scheduling-before-choosing-a-framework). There is no blanket requirement to use or avoid an ECS. Keep domain types explicit under any storage choice, and keep graphics-library types outside authoritative state.

The backend owns world state and exposes scenario creation, typed commands, time advancement, read-only views, choices, and events. Normal clients receive views appropriate to the player; debugging tools can explicitly request omniscient state. A command cannot acquire authority just because it came from the UI. Every gameplay interaction must work through this interface before graphics is added.

Organize code by domain:

```text
identity / time / rules / world
characters / knowledge / relationships / decisions
areas / travel / storage / goods
tasks / production / projects / enterprises
claims / agreements / institutions / government
health / conflict / missions
lineage / guardians / past_lives
commands / simulation / events / inspection / persistence
scheduling / batches / compute / observations / session_host
```

Introduce modules as their behavior is implemented. Do not create a generic engine or empty framework for every eventual feature before proving a small loop.

## Identity and state ownership

Use typed `u64` IDs and one checked logical world-wide allocator starting at `1`. Never reuse an ID, including after death or deletion. Preserve historical references through archived or terminal records. Workers emit creation requests; assign IDs in deterministic logical order rather than from racing increments. ECS handles and dense row indices are separate from persistent IDs.

Characters, organizations, places, storage, physical lots and unique items, claims, agreements, projects, enterprises, institutions, offices, missions, and incarnations have distinct identities where needed. A resource quantity is not a second independently owned object merely because an account displays it.

Each domain owns its authoritative records. Enterprises reference physical inventories; agreements reference assets and obligations; governments reference offices and organizations; characters reference claims and memberships. Derived totals, inspection summaries, and caches must be reconstructible.

Use compact hot-state storage and indexed sparse records as defined in [performance](performance.md#data-layout-and-access). Split frequent numeric state from history and descriptive text. Hash maps may serve lookups, but their iteration order must never decide an outcome. Avoid full-population opportunity scans and all-pairs relationship tables.

## Concurrency contract

Concurrency is required in the initial kernel, with a one-worker execution mode for comparison. Ordered phases define dependencies, not a requirement that all computation runs sequentially. Jobs read a consistent phase view and write disjoint next-state ranges or owned result buffers. Shared interactions become intentions that are resolved before commit.

Use bounded worker execution, declared read/write dependencies, stable logical operation keys, and explicit coordination for cross-partition transactions. Independent groups can resolve and commit in parallel. Keep contention visible; a global lock around the world is not the concurrency architecture. Full details and tests are described in [performance](performance.md#execution-model).

Pure numeric batches have CPU implementations behind narrow compute contracts. These permit optional later GPU implementations without giving a device ownership of game state or coupling compute to graphics.

## Time and scheduling

Use typed integer `SimTime` and `SimDuration`, stored as milliseconds from the world epoch. Time `0` is Day 1 at `00:00`; a minute contains `60,000` units and a day contains `1,440` minutes. The fine representation permits combat and interruption timing later; it does not require executing a simulation step every millisecond. The calendar's year length, seasons, and aging rates belong to setting and rules data.

The initial CLI advances time only through `run`. Reading, typing, and debugging never advance the world. When combat is implemented, committing a shared round authorizes its fixed world-time interval; choosing an individual action does not authorize a separate full interval. The graphical client can support real-time noncombat play, pause, and speed controls through the same simulation API. Real elapsed time is not an input to game rules.

Use a one-minute economic quantum in the first reference implementation, subdividing at earlier scheduled events and threshold boundaries. Combat rounds use their configured duration and any internal effect or warning deadlines; economic reviews keep their own schedules. Advancing a minute sixty times produces the same state and events as advancing an hour, provided commands arrive at the same game times. Different client time-advance chunk sizes must not create extra decision opportunities or change results. Stopping inside an interval preserves its current action and remaining schedule.

Keep logical simulation boundaries distinct from host yields and requested stopping times. Align periodic deadlines to the simulation schedule, not to the start of each `run` call. A partial advance can materialize elapsed progress and publish a view, but must preserve pending effects, fractional remainders, reservations, and the next logical deadline. It cannot trigger an extra review, resource grant, completion, or gameplay event. A timestamped gameplay command can introduce a logical boundary; compare replay runs with commands at the same times.

Use different review schedules for different work: immediate physical actions and urgent reactions, periodic goal review, daily institution or household planning, seasonal production, and age-related events. An action can trigger an earlier review when its assumptions change. Do not rescore every long-term goal every minute.

Build due-work queues, dependency invalidation, and a cost breakdown from the start. Add analytic progress up to the next meaningful boundary where equivalence to the reference path can be proved and tested. Never skip an emergency, harvest, birth, death, delivery, conflict, player choice, or required past-life cost warning. Meet the [performance budgets](performance.md#measure-the-whole-simulation) throughout development; do not postpone scale testing until the wider realm stage.

## Advancement interval contract

At logical boundary `t`, apply queued player commands in authoritative submission order and resolve events due at that boundary. Build a consistent view for decisions and progression until the next logical boundary `t + dt`, chosen from scheduled updates, completion times, interruptions, and required warnings. Client yields inside that interval do not repeat these phases.

1. Reconcile institutions, agreements, projects, and tasks whose scheduled review is due.
2. Check urgent needs, invalid actions, and due plan reviews.
3. Generate intentions from the same decision snapshot.
4. Arbitrate task capacity, stock, equipment, and action reservations.
5. Resolve `dt` of travel, work, rest, communication, care, or conflict for actions active during this interval.
6. Commit physical production, transfers, consumption, damage, and accrued obligations.
7. Apply the interval's physiological changes and boundary consequences, including death where applicable.
8. Record resulting events at `t + dt`, advance the clock, and publish a committed revision.

Specify each event's boundary and deterministic sequence number. Where conflict requires simultaneous effects, collect outcomes before committing them so iterating one actor first does not grant an extra attack or transfer. A phase's execution may be parallel; its observations and visible outcomes must satisfy the same ordered contract at every worker count.

Before advancing through a player warning threshold, stop at the last safe boundary and return the warning. A large `run` request returns the elapsed portion and its stop reason; it does not silently skip the decision. If a new cost would begin immediately on a command, preview it before committing that command.

The [control-loss contract](ruler-and-guardians.md#warning-and-consent-contract) is an explicit exception: after player acceptance, its consequences proceed for the warned interval without renewed consent or dismissal. Persist that interval, autonomous control state, and elapsed exposure. Reject player character actions during it; host pause, observation, and saving do not end it. Restore ordinary warning handling when control returns.

## Commands, intentions, and transactions

Commands express requests or attempts, not arbitrary state changes. Examples are moving the ruler, issuing an adult guardian order, requesting help, making an offer, authorizing spending within an office's powers, attempting theft, or choosing a childhood interaction.

Validate syntax, identities, actor capability, and relevant procedural authority before committing a command. Invalid commands leave the world, ID allocator, and time unchanged. A physically possible illegal attempt can be valid as an attempt; failure or social consequences are simulated afterward.

Separate command acceptance from eventual success. Accepting a proposal can create a future delivery obligation. It cannot move a distant stack into local storage. Direct orders persist as intentions while travel and self-care produce the immediate actions needed to carry them out.

For contested resources, reserve against one physical ledger. Competing intentions cannot consume the same final meal or use the same exclusive tool. Preserve valid existing reservations, then arbitrate equal competing new requests in a deterministic rotating actor order. For each contested resource, keep the last granted actor ID; among equally eligible requests sorted by actor ID, begin after that ID and wrap, starting with the lowest ID if no grant exists. Update the cursor only on a successful grant and persist it. This is a technical tie-breaker among otherwise eligible actions, not a resolution of property disputes or physical force.

Resolve physical interference through the conflict rules before relying on reservation priority. A reservation can fail when goods are stolen, destroyed, or made inaccessible; it never grants an invisible barrier. Cancel or reduce affected reservations without duplicating stock, and keep that authoritative correction separate from when an actor learns about it.

Commands and instantaneous exchanges either commit all immediately applicable effects or none. Long processes have explicit intermediate states and consequences. Do not make an entire multi-day project atomic or roll back work when its funding fails.

## Actions and interruption

Each character has one primary time-consuming action. Modifiers such as carrying a load and invoking a past life can accompany that action without producing an extra minute of labor. Explicitly declared passive processes, such as crop growth and guardian regeneration, can continue independently.

Actions have prerequisites, reservations, progress, interruption rules, and completion effects. Arriving at a destination at a boundary does not also grant work during the interval just spent traveling. Newly produced goods are eligible for new actions after the current resolution completes. Use actual duration so subdividing time cannot manufacture work, food, or attacks.

Urgent needs, danger, and invalidated prerequisites can interrupt actions. An interrupted plan retains valid progress and records why it stopped. Consumed supplies stay consumed. Reservations release when their purpose ends; cargo remains physically carried until another action moves it.

Persistent orders have active, suspended, blocked, completed, and cancelled states. A suspended order can resume after recovery. A blocked order records its missing prerequisite. New orders replace old intentions only after validation.

## Travel

Use an area graph with integer travel durations and properties such as mode, capacity, danger, and access. Exact paths across a tile grid are not simulated. Named areas can represent a village, farms, forest, docks, a mine, or a military camp.

Combat adds a local map and continuous position for participating characters, as defined in [realm and missions](realm-and-missions.md#conflict-and-combat). This refines their location without creating a second character or inventory. Transfers between travel and combat occur through explicit entry, interception, retreat, or exit events.

Location is `AtArea` or `InTransit` with a route, current leg, remaining duration, and carried cargo. Use deterministic shortest-route search over the traveler's known graph for ordinary travel, with alternative route preferences available for safety or secrecy. Tie-break equal routes stably. Physical execution uses actual link conditions; an undiscovered change can cause a failed or revised journey without granting advance knowledge.

A traveler cannot operate a facility at either endpoint. In the first engine slice, a leg finishes before a replacement destination takes effect. Add turnback, interception, and stranding through explicit transit events when conflict arrives; none can teleport a traveler to an endpoint.

Communication between separated actors uses a delivery channel with travel or transmission time. Same-area conversations still consume the relevant actors' attention. A player watching an area does not automatically inform their character about it.

## Quantities, conservation, and randomness

Use integer fixed-point values for quantities, progress, needs, utility, and burden. Define units and valid ranges in rules data. Preserve fractional remainders for repeated production, payments, healing, and time conversions. Use checked arithmetic at boundaries where overflow could corrupt persistent state.

All stock changes have a cause: production, consumption, spoilage, destruction, or physical transfer. Claims and accounts cannot create stock. A transfer conserves total physical quantity. Birth, death, and regeneration have explicit rules instead of copying or deleting unrelated assets.

Start deterministic scenario tests with authored character variation. When simulation uncertainty is added, use a fixed, versioned random algorithm keyed by the world seed and logical operation identity, as specified in [performance](performance.md#determinism-across-worker-counts). Persist any logical occurrence counters. A sequential generator shared by racing workers is not permitted. Presentation randomness is independent.

## Events, knowledge, and history

Events contain time, sequence, cause, participants, affected records, and inspectable results. The complete event stream supports debugging. Character knowledge is a separate projection that applies witness, report, and communication rules.

Record meaningful changes and decision explanations without writing a permanent event for every unchanged minute. Preserve historically significant deaths, incarnations, agreements, disputes, institutional changes, and memories; aggregate routine work history with trace mode available for short investigations.

Saving captures a committed revision with world state, clock, ID allocator, pending actions and events, reservations, random seeds and counters, knowledge, contracts, lineage, and unacknowledged warnings. In-flight worker or device buffers are not authoritative saved state; finish to a boundary or save the prior consistent revision with its pending logical work. Load must preserve deterministic continuation. Use an explicit versioned format; select its encoding when persistence is implemented.

## Backend invariants

- One physical location for each item or resource lot; no negative stock or duplicated cargo.
- One living incarnation of the lineage at most; each guardian retains one persistent identity.
- No more exclusive work than elapsed actor time or site capacity allows.
- Access rights, claims, custody, and obligations are separate.
- All actors use the same physical rules, with explicit physiology and ability exceptions.
- Information does not spread without a knowledge event or communication path.
- Technical command acceptance does not guarantee political obedience or contractual performance.
- Identical initial state, rules, random seed/counters, and timestamped commands yield identical results across supported worker counts and compute backends.
- Render frequency, inspector queries, client replacement, and ECS storage layout do not change gameplay outcomes.
- All mechanics, including tactical resolution and player choices, run without a graphics context.
- Advance size cannot bypass a required warning or create acceptance of control loss. A previously accepted control-loss interval permits its ensuing consequences under the same rules at every advance size.
