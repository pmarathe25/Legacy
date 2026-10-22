# game-knowledge

Per-person belief tables, active local inspection, stale-stock disproof, and belief-based route planning.

**Why it matters:** Characters act on what they personally remember or have been told—not global truth. Storing individual facts with provenance and staleness makes exploration, scouting, and communication genuinely meaningful.

## Responsibilities

- **Personal Fact Tables:** Stores per-person facts with source, observation timestamp, confidence, and revision across namespace-indexed tables.
- **Active Inspection Actions:** Executes `ObserveSurroundings` (local roads, exposed patches, and accessible containers), `ObserveStore` (container stock, capacity, and mass), and `ObservePatch` (local resource state).
- **Absence Recording:** Records `{"absent_from": area}` when a remembered store is missing from its expected location so planning stops targeting vanished supplies.
- **Belief-Based Routing & Forecasting:** Builds travel routes (`route`) strictly from remembered roads and projects outcomes (`forecast`) under changed conditions using learned claims.

## Boundaries

- **No Passive Omniscience:** Read queries return only recorded personal beliefs; passive inspection never teaches new facts.
- **No Spoken Delivery Rules:** Delegates contact validation and testimony comprehension to `game-testimony` and `game-communication`.

## How It Works

- **Shared Immutable Scenes:** Shares immutable road and exposed-patch snapshots across local observers via `game-facts` while maintaining separate per-person evidence timestamps. Moving to a new area freezes prior scene evidence into cold history.
- **Versioned Discovery Caches:** Tracks weak permission-version tokens per person to skip redundant container discovery scans until property access changes.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers personal fact storage, scene caches, and observation request handlers in `game-assembly`.
- **Requests & Events:** Handles `Initialize`, `ObserveSurroundings`, `ObserveStore`, and `ObservePatch`; exposes belief queries, `route`, and `forecast`.
- **Callers:** Queried by almost all decision and planning crates (`game-provisioning`, `game-livelihoods`, `game-characters`, `game-inquiry`, `game-testimony`, `game-exchange`, `game-conflict`, etc.).

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-facts`, `game-objects`, `game-cargo`, `game-property`, `game-transport`, `game-ecology`, `game-physiology`, `game-reasoning`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-knowledge`) cover local discovery, stale stock disproof, report delivery, spoken locality, and routing across partially known road networks.

Personal retention is now owned by `game-memory`. `Knowledge`, `Facts`, `Receive`
and `Initialize` are compatible names for that same evidence store and its
requests. Perception caches belong here but cannot bypass personal forgetting.
There is no permanent second copy of a character's remembered facts.
