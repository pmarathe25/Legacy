# game-api

`game-api` defines the renderer-independent player `Command` enum, `CommandResult`, and immutable `PlayerView` observation structs.

**Why it matters:** Decoupling the public player protocol from the CLI and internal host transactions allows text scripts, interactive shells, and future graphical clients to share one stable, actor-safe interface.

## Responsibilities

- **Public player commands (`Command`):** Enumerates all player-issuable actions (`Move`, `Eat`, `Rest`, `Observe`, `ObserveSurroundings`, `ObservePatch`, `Tell`, `Talk`, `ReadWork`, `PresentWork`, `Take`, `Sow`, `Harvest`, `Craft`, `Plant`, `Pursue`, `Treat`, `Gather`, `Haul`, `Cancel`, `Accept`, `Reject`, `Fulfill`, `Vote`, `Decide`, `Build`, `GuardianOrder`, `Invoke`, `Consent`, `Dismiss`, `Combat`, `Depart`, `Return`).
- **Subjective player view (`PlayerView`):** Bundles current time, revision, ruler `PersonId`, `BodyView`, `Location`, active action, remembered `Facts`, invocation warnings/memories, personal `lexicon`, accessible `works`, completed `pastimes`, active `battles`, `communities`, visible `neighbors`, and witnessed `PlayerEvent`s.
- **Command acknowledgement (`CommandResult`):** Returns the commit timestamp, host revision, and JSON-serialized request response.

## Boundaries

- **No client-selected actor:** `Command` never accepts an arbitrary actor `PersonId`; [`game-assembly`](../game-assembly/README.md) supplies the active ruler from [`game-lineage`](../game-lineage/README.md).
- **Data transfer types only:** Contains no command routing, state mutation, or view construction logic.

## How It Works

- **Subjective projection:** Every field of `PlayerView` represents only what the current ruler can perceive or remember, preventing omniscient state leaks to clients.
- **Serde-compatible wire format:** All commands and views derive `Serialize` and `Deserialize` so commands can be submitted as raw JSON (`command <JSON>`) and replayed across saves.

## Game Integration

- **Plugin / Entry Point:** Pure protocol library (`Command`, `PlayerView`, `PlayerEvent`, `CommandResult`); registers no `game-sim` plugin.
- **Requests & Events:** None directly (bound to internal feature requests by `game-assembly`).
- **Callers:** Consumed by [`game-assembly`](../game-assembly/README.md), [`game-session`](../game-session/README.md), and [`game-cli`](../game-cli/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-lineage`](../game-lineage/README.md), [`game-physiology`](../game-physiology/README.md), [`game-transport`](../game-transport/README.md), [`game-knowledge`](../game-knowledge/README.md), [`game-invocation`](../game-invocation/README.md), [`game-conflict`](../game-conflict/README.md), [`game-social-life`](../game-social-life/README.md), [`game-communities`](../game-communities/README.md), [`game-communication`](../game-communication/README.md), [`game-works`](../game-works/README.md).
- **Testing:** `cargo test -p game-api` verifies JSON round-trip serialization of commands and views.
