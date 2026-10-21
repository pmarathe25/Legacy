# game-affiliation

Recruiting newcomers into existing communities, voluntary or coerced admission, and member departure or succession cleanup.

**Why it matters:** Communities grow and fracture over time. `game-affiliation` lets existing groups invite newcomers during local encounters and handles the permission and office fallout when members leave or leaders die.

## Responsibilities

- **Newcomer Admission:** Evaluates invitations into existing communities during local encounters, allowing newcomers to join through cooperation, loyalty, or fear-driven compliance according to the institution's active procedure (`game-institutions`).
- **Voluntary Departure:** Allows members to leave arrangements they no longer accept, automatically withdrawing the compact `game-property` permissions they previously granted.
- **Mortality & Leader Departure Cleanup:** Clears memberships, offices, contributions and custodial rules upon death. Literal support requirements remain unchanged, so survivors can face deadlock and must establish a new arrangement themselves. Historical `Leader` saves retain their personal-rights fallback.

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

A proposed procedure is compared against each participant's current arrangement,
using that participant's preferences and acquired commitments. Liking two potential
leaders no longer implies accepting repeated reversals between them. Inherited
rigidity changes resistance; a sufficiently strong preference change can still
outweigh it. The policy grants no privileged status to voting or any other procedure.

Autonomous reform constructs a concrete influence arrangement from the proposer's values and relationships. Addressed local participants respond independently. A person may prefer the terms, or comply with a nearby capable claimant out of credible fear. Public responses do not expose the private grounds. Fear is recorded as `Attachment::Fear`, not loyalty or an identity commitment. Refusal returns `Reconsider`, preserving the response evidence and leaving alternative choices available.

An allegiance demand within a larger group can attempt a takeover after submission. A submitted member records fear and grants access to the carried supplies they can authorize, even when others resist. Each affected member must independently recognize or comply with the claimant; defeating one person does not transfer everybody else's allegiance. Dissent can leave the old arrangement in place. This is a bounded local model, not military conquest of distant institutions. Existing discourse can spread misleading citations or force public endorsements without changing private conviction; institutional preferences read participants' own beliefs rather than an oracle of truth.
