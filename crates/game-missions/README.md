# game-missions

Consenting expedition parties, supply preparation, shared strategic travel, arrival, return, and mortality handling.

**Why it matters:** Long-distance scouting, diplomacy, and military expeditions require assembling consenting companions, packing real supplies, and marching together across the map while the rest of the world continues in real time.

## Responsibilities

- **Party Formation & Consent:** Assembles multi-person expedition parties from consenting, available characters.
- **Preparation & Shared Travel:** Coordinates supply loading (`game-cargo`) and synchronized leg-by-leg strategic travel (`game-transport`, `game-land`, `game-spatial`).
- **Lifecycle & Mortality:** Handles destination arrival, mission completion, return journeys, and mid-expedition injury or death (`game-physiology`).

## Boundaries

- **No Time Freeze:** Mission members occupy their real action clocks while home settlements and distant regions continue simulating normally.
- **No Military Recruitment Rules:** Unit muster and army upkeep belong to `game-armies`, which delegates actual expedition movement here.

## How It Works

- **Synchronized Party Legs:** Advances party members along shared road routes, keeping the group together across waypoints and updating member states if a participant dies or becomes incapacitated en route.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers mission rosters, party travel state, and expedition request handlers in `game-assembly`.
- **Requests & Events:** Exposes mission creation, member recruitment, departure, and return requests; emits arrival and completion events.
- **Callers:** Used by `game-armies`, `game-realm`, `game-api`, and scenario fixtures.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-physiology`, `game-transport`, `game-land`, `game-spatial`, `game-cargo`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-missions`) verify party consent, synchronized multi-leg travel, supply carrying, mid-journey mortality, and return completion.
