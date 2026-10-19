# game-wayfaring

`game-wayfaring` drives voluntary exploration journeys along remembered roads and tracks each character's visit history across areas.

**Why it matters:** Without voluntary travel, autonomous characters stay in their birth area until local food runs out; wayfaring connects settlements through physical road trips, layovers, and newly observed surroundings.

## Responsibilities

- **Journeys & visit history (`Wayfaring`):** Stores active outbound/return trip intentions, 4-hour destination layover deadlines, per-area last-visit timestamps, and travel cooldowns (`wayfaring.state`).
- **Frontier & revisit selection:** Chooses unexplored frontier areas (known roads whose destination surroundings have never been observed) or revisits familiar areas not seen in 7 days for characters with sufficient risk tolerance.
- **Discovery learning:** Emits `Visited` on first arrival at a new area and strengthens the traveler's personal concept value for discovery via `game-relationships`.

## Boundaries

- **Voluntary exploration vs. survival foraging:** Owns curiosity and visit-driven trips; urgent survival searches when local food fails belong to [`game-provisioning`](../game-provisioning/README.md), and physical road movement belongs to [`game-transport`](../game-transport/README.md).

## How It Works

- **Layover & return cycle:** Upon arriving at a destination, the traveler observes local surroundings (`game-knowledge`) and remains free for 4 hours to gather, converse, or trade before scheduling the return trip home.
- **Non-interference with urgent needs:** Respects a 2-day cooldown between trips, excludes the controlled player and actors with active livelihood plans, and yields immediately if hunger or fatigue intervenes.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`wayfaring`, resource `wayfaring.state`).
- **Requests & Events:** Handles `Enable` and `Step`; registers with [`game-choice`](../game-choice/README.md); subscribes to `game_transport::Arrived` and `game_physiology::Died`; emits `Visited`.
- **Callers:** Enabled by [`game-campaign`](../game-campaign/README.md) and [`game-openings`](../game-openings/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-storage`](../game-storage/README.md), [`game-choice`](../game-choice/README.md), [`game-diagnostics`](../game-diagnostics/README.md), [`game-vitality`](../game-vitality/README.md), [`game-ecology`](../game-ecology/README.md), [`game-transport`](../game-transport/README.md), [`game-physiology`](../game-physiology/README.md), [`game-characters`](../game-characters/README.md), [`game-knowledge`](../game-knowledge/README.md), [`game-provisioning`](../game-provisioning/README.md), [`game-livelihoods`](../game-livelihoods/README.md), [`game-relationships`](../game-relationships/README.md), [`game-traits`](../game-traits/README.md), [`game-lineage`](../game-lineage/README.md).
- **Testing:** `cargo test -p game-wayfaring` verifies frontier selection, layover timing, return trips, discovery value updates, and save/load state.
