# Headless play and the future graphics interface

**Scope:** Implement the complete game mechanics without a graphics engine. The CLI and scenario runner exercise the same public commands, choices, and observations that a graphical client will use. Graphics is a later consumer of an already playable simulation.

## Dependency boundaries

Separate public data, simulation, host scheduling, and presentation:

```text
CLI / scenario runner / future graphical client
                    ↓ commands, queries, choice responses
             session host and public API
                    ↓
        simulation domains and compute interfaces
                    ↓ committed observations, choices, events
             client-specific presentation
```

The session host owns a simulation instance, its worker budget, command queue, advancement requests, and publication of committed views. It can run on the calling thread in tests or on a dedicated host thread in an interactive application. Threading policy is separate from game rules.

Use a Rust workspace with small `game-api`, `game-sim`, and `game-cli` crates under `crates/`. The API crate contains renderer-independent value types. The simulation crate contains domains, scheduling, and CPU compute; an optional compute adapter can be extracted into its own crate later. Add a benchmark target using the simulation directly. The [first coding task](implementation.md#first-coding-task) defines the initial slice; introduce further boundaries as working code needs them.

Neither the API nor authoritative simulation depends on windows, a scene graph, a rendering engine's clock, animation callbacks, textures, input devices, or a graphics context. If an ECS is used internally, its entity handles and mutable world are private implementation details.

## Public operations

The interface supports:

- listing available starting scenarios and creating a game from explicit or seeded random selection;
- submitting typed commands and responding to pending choices;
- querying player-visible state and available actions;
- advancing toward a game time or duration, pausing, and inspecting progress;
- receiving committed view changes, meaningful events, and stop reasons;
- saving and loading complete sessions;
- explicitly privileged debug inspection and scenario control for testing.

Commands carry a caller sequence number and any relevant expected revision, offer, or choice ID. The host assigns an authoritative game boundary and command sequence for accepted input. Record those values for replay. Repeating a submitted command must not accidentally purchase goods, acknowledge a cost, or issue an order twice.

Starting scenarios use stable IDs and versioned initial data. Random selection chooses from supported player scenarios in stable order using the session seed; persist the selected ID, content version, and seed for replay. Test fixtures are separate from the player scenario menu. Adding a scenario changes initial conditions, not the simulation's fundamental rules or a global government-mode switch.

A stale observation can still support a valid request. Revalidate affected preconditions at submission rather than rejecting every command merely because the world revision advanced. Return a structured rejection for expired offers, invalid actors, changed prices or quantities, and unavailable authority.

Command results and available-action lists obey the same knowledge boundary as views. They cannot reveal an unseen stock change or a distant actor's private decision through a rejection reason. Accept a physically meaningful attempt or message when appropriate, then expose failure when the actor can discover it. Debug validation can explain the authoritative cause separately.

Normal play never receives unrestricted mutation access. Debug controls are separate operations that cannot masquerade as lawful player authority.

## Observations and views

Publish immutable observations with a world revision and game time. Use stable game IDs to connect characters, places, goods, orders, institutions, and events. Normal play exposes the ruler's knowledge and received reports, including their source and age where relevant. Unknown facts must stay absent from details, summaries, explanations, and event notifications. Omniscient inspection is a distinct debugging facility.

Guardian-search spectating is a separate allowed view. Its precise contents will be settled with the search mechanic; it cannot silently become general world omniscience or add facts to a character's knowledge merely because the player watched. Observer modes convey information, not extra command authority.

Provide overview summaries, paged or filtered collections, focused entity details, and updates since a revision. Do not copy every person, relationship, and historical record into every displayed frame. Maintain indexes for common views and invalidate them from committed changes.

Clients can interpolate visual movement between authoritative samples. That interpolation cannot determine arrival, collision, damage, resource transfer, action completion, or cost thresholds. Combat positions and other facts needed by gameplay belong to the simulation and remain queryable without graphics.

The future renderer maps stable IDs to its own visual objects. Removing or replacing those objects, hiding a panel, changing the camera, or rendering at a different frame rate does not alter the simulation.

## Decisions, dialogue, and warnings

Represent a player interaction as structured state: its subject, participants, context, available options, prerequisites, costs, expiry or waiting rules, and the effect of each supported response. The CLI must be able to display and answer it without embedding game rules in text parsing.

This covers childhood requests, negotiation and counteroffers, institutional decisions, missions, combat orders, death/birth transitions, and past-life warnings. A choice is not considered implemented when only an event saying that it happened exists; the actual response and consequences must work through the API.

Some decisions permit the world to continue; others require a stop. While the player retains control, past-life warnings stop before an unaccepted new cost and remain pending until an explicit response. A closed window, disconnected client, or automated runner cannot silently accept them.

Before accepting loss of control, display its duration and the fact that subsequent costs and outcomes will proceed without another consent choice, emergency dismissal, or cost cap. During the accepted interval, character actions and dismissal are unavailable to the player; observation, session pause, and save/load remain available. Preserve the interval across reconnection and apply ordinary warnings again when control returns. Guardian commands likewise express orders a guardian may refuse, not direct mutation of their actions.

Domain output supplies explanation data and displayable placeholder text. Later localization, portraits, voice, layout, and animations can change presentation without changing the available choice or consequence.

## Advancement and responsiveness

An advancement result includes the requested target, actual reached time, current revision, and a stop reason: target reached, user pause, pending required choice, lifecycle transition needing input, campaign terminal state, or error.

Break large run requests into bounded scheduling work. Process new control input at consistent boundaries and publish progress from committed revisions. A request to advance a year must not block pause processing until the year finishes. The latency budgets are defined in [performance](performance.md).

The renderer and CLI do not hold the simulation's write access while drawing or printing. Share immutable snapshots or selected versioned data; bound retained revisions so a slow consumer cannot pin the entire history in RAM.

Presentation updates can be coalesced. Accepted commands, pending required choices, and authoritative state changes cannot be dropped. A client that misses retained incremental events requests a new snapshot; pending critical choices remain in authoritative state and survive save/load.

An overloaded machine slows simulated progress or uses a lower requested speed while remaining responsive. It must not grant free work, skip citizens, change random outcomes, or waive a cost to catch up with the wall clock.

## Complete mechanics without graphics

All supported player actions must be reachable through the public API and a minimal text interaction. The headless deliverable includes the complete economy and institutions, growth and mortality, reincarnation and upbringing, past-life access and costs, missions, conflict, armies, diplomacy, saves, and campaign continuation or termination rules once designed.

A headless battle exposes the generated map geometry, known participants and continuous positions, available turn actions, round state, and actionable warnings. Shared rounds advance fixed world-time intervals. Initiative and action rules remain to be designed, but their implementation belongs to the backend. The future visual client shows these same results; it does not supply the only map generator or conflict resolver.

Test the same timestamped command script through a direct scenario runner and the CLI command adapter. Vary observation frequency, client attachment, and time-advance request sizes; the canonical state and meaningful event stream must agree. Pending choices and warnings must remain actionable after save/load and client reconnection.

An integration test can act as a second client: list available actions, inspect state, respond to choices, and track movement or combat. Reuse the test fixture and API; no separate fake-client application or handoff artifact is needed. Include a test where an unreported distant event remains absent from normal queries and appears only after its report arrives.
