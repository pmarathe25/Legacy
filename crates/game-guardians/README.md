# game-guardians

Immortal guardian identities, innate successor sensing, route-by-route search, and mentorship of new rulers.

**Why it matters:** Across generations and interregnum gaps, guardians act as enduring anchors who sense when a new dynastic successor is born, travel across the physical road network to find them, and offer mentorship.

## Responsibilities

- **Guardian Identity & Sensing:** Tracks persistent guardian characters and their innate awareness of the current `game-lineage` successor.
- **Route-by-Route Search:** Moves guardians step by step across the physical `game-land` / `game-transport` network toward the successor without teleporting.
- **Mentorship & Refusal:** Assesses and mentors the successor in `game-skills` upon contact, handling player or successor refusal without permanently abandoning the dynasty.

## Boundaries

- **No Teleportation or Instant Healing:** Bodily recovery remains governed by `game-physiology` (and restraint by `game-confinement`) without moving the guardian across the map.
- **No Forced Tutelage:** Mentorship requires co-location and respects refusal.

## How It Works

- **Stepwise Pathing:** Queries spatial and road topology (`game-land`, `game-spatial`) to schedule real `game-transport` travel legs as the successor's location changes.
- **Mentorship Transfer:** When co-located with an eligible ruler, conducts training interactions that boost skill acquisition in `game-skills`.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers guardian records, search scheduling, and mentorship handlers in `game-assembly`.
- **Requests & Events:** Exposes guardian registration, search updates, and mentorship requests.
- **Callers:** Coordinates with `game-lineage`, `game-confinement`, `game-transport`, `game-physiology`, and `game-skills`.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-lineage`, `game-transport`, `game-land`, `game-spatial`, `game-physiology`, `game-skills`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-guardians`) verify successor sensing after reincarnation, multi-hop road travel, mentorship skill transfer, refusal handling, and recovery in place.
