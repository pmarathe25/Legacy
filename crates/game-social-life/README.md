# game-social-life

Timed local conversations (`Talk`), recipient consent, contact history, and area residency indexing.

**Why it matters:** People form bonds and exchange knowledge face-to-face. `game-social-life` enforces co-location, mutual availability, and recipient consent for conversations while updating each participant's feelings and memories independently.

## Responsibilities

- **Physical Conversation (`Talk`):** Verifies both participants are alive, capable, available, and co-located; checks recipient consent based on their directed relationship; and occupies both action clocks for the conversation duration.
- **Asymmetric Impression & Fact Sharing:** Updates each speaker's relationship (`game-relationships`) and fellowship value separately according to expressed beliefs and empathy, and exchanges one remembered fact via `game-testimony`.
- **Contact History & Residency:** Tracks recent encounter timestamps, search progress (`Scanned`), public person registration/rename projections, and an event-driven area residency index (`neighbors`).
- **Interruption Cleanup:** Releases both participants without encounter credit if either cancels, dies, or suffers an urgent hunger interruption.

## Boundaries

- **No Initiator Search Policy:** Autonomous companion-seeking proposals (`SeekCompany`) are owned by `game-companionship`.
- **No Political or Household Rules:** Models ordinary companionship and conversation—not romance, household care, or political coercion.

## How It Works

- **Event-Driven Locality Index:** Subscribes to movement (`game-transport`) and mortality (`game-physiology`) events to maintain per-area neighbor lists without polling. `neighbors` exposes only co-located people and their visible activity.
- **Independent Outcome Evaluation:** Because impressions depend on each listener's own beliefs and empathy, a single conversation can warm one participant's affection while souring the other's.

## Game Integration

- **Plugin / Entry Point:** `Plugin` loads rules from `content/social-life/*.toml` and registers residency indices, contact logs, and `Talk` / `Scanned` handlers in `game-assembly`.
- **Requests & Events:** Exposes `Talk`, `Scanned`, `Register`, `Rename`, and `neighbors` queries; emits completed conversation events.
- **Callers:** Used by `game-companionship`, `game-characters`, `game-discourse`, `game-naming`, `game-names`, `game-inquiry`, and `game-literacy`.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-content`, `game-physiology`, `game-transport`, `game-knowledge`, `game-testimony`, `game-relationships`, `game-traits`, `game-communication`, `game-choice`, `game-utility`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-social-life`) cover consent refusals, asymmetric impressions, fact sharing, interruption cleanup, locality indexing, and multi-worker save/load.
