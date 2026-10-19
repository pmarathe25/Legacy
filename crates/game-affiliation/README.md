# game-affiliation

Recruiting newcomers into existing communities, voluntary or coerced admission, and member departure or succession cleanup.

**Why it matters:** Communities grow and fracture over time. `game-affiliation` lets existing groups invite newcomers during local encounters and handles the permission and office fallout when members leave or leaders die.

## Responsibilities

- **Newcomer Admission:** Evaluates invitations into existing communities during local encounters, allowing newcomers to join through cooperation, loyalty, or fear-driven compliance according to the institution's active procedure (`game-institutions`).
- **Voluntary Departure:** Allows members to leave arrangements they no longer accept, automatically withdrawing the compact `game-property` permissions they previously granted.
- **Mortality & Leader Departure Cleanup:** Clears memberships and offices upon death, reverting a leaderless group to personal decision rights until survivors adopt a new arrangement.

## Boundaries

- **No Separate Institution Store:** Uses `game-institutions` for procedures/offices and `game-communities` for membership indices and personal attachments.
- **Historical Preservation:** Empty institutions remain inspectable in history after their last member departs or dies.

## How It Works

- **Procedure-Gated Admission:** Checks `game-institutions` for required member or leader approvals while recording the newcomer's personal attachment mode in `game-communities`.
- **Event Publication:** Emits membership and procedure change events whenever newcomers join, custodians depart, or offices fall vacant.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers encounter recruitment hooks, departure handlers, and mortality cleanup listeners in `game-assembly`.
- **Requests & Events:** Dispatches admission and withdrawal requests to `game-institutions`, `game-communities`, and `game-property`; publishes affiliation change events.
- **Callers:** Triggered by local social encounters, `game-characters` idle reviews, and `game-physiology` death events.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-property`, `game-transport`, `game-physiology`, `game-traits`, `game-relationships`, `game-institutions`, `game-communities`, `game-pressure`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-affiliation`) verify newcomer recruitment, ballot vs. leader admission, permission withdrawal on departure, and leader death fallback.
