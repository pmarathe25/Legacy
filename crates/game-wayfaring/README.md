# game-wayfaring

`game-wayfaring` drives voluntary exploration journeys along remembered roads and tracks each character's visit history across areas.

**Why it matters:** Without voluntary travel, autonomous characters stay in their birth area until local food runs out; wayfaring connects settlements through physical road trips, layovers, and newly observed surroundings.

## Responsibilities

- **Journeys & visit history (`Wayfaring`):** Stores active outbound/return trip intentions, per-area last-visit timestamps, and an optional completed-trip cooldown (`wayfaring.journeys`).
- **Frontier & revisit selection:** Chooses unexplored frontier areas (known roads whose destination surroundings have never been observed) or revisits familiar areas not seen in 7 days for characters with sufficient risk tolerance.
- **Discovery learning:** Emits `Visited` on first arrival at a new area and strengthens the traveler's personal concept value for discovery via `game-relationships`.

## Boundaries

- **Voluntary exploration vs. survival foraging:** Owns curiosity and visit-driven trips; urgent survival searches when local food fails belong to [`game-provisioning`](../game-provisioning/README.md), and physical road movement belongs to [`game-transport`](../game-transport/README.md).

## How It Works

- **Return cycle:** On arrival the traveler observes local surroundings. Return remains available immediately; this owner does not impose a destination layover.
- **Experimental cooldown:** `ConfigureTripCooldown` optionally defers new voluntary journeys after a completed round trip. While a trip is active, voluntary exploration stays aimed at its original destination or home; reaching home also closes an outbound trip. It does not block a return leg, survival travel, or direct movement. The default is disabled; this changes behavior and should be evaluated against a baseline.
- **Other choices:** Controlled players are excluded, and travel competes with other options according to their current motives.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`wayfaring`, resource `wayfaring.journeys`).
- **Requests & Events:** Handles `Enable`, `Consider`, and `ConfigureTripCooldown`; registers with [`game-choice`](../game-choice/README.md); subscribes to `game_transport::Arrived` and `game_physiology::Died`; emits `Visited`.
- **Callers:** Enabled by [`game-campaign`](../game-campaign/README.md) and [`game-openings`](../game-openings/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-storage`](../game-storage/README.md), [`game-choice`](../game-choice/README.md), [`game-diagnostics`](../game-diagnostics/README.md), [`game-vitality`](../game-vitality/README.md), [`game-ecology`](../game-ecology/README.md), [`game-transport`](../game-transport/README.md), [`game-physiology`](../game-physiology/README.md), [`game-characters`](../game-characters/README.md), [`game-knowledge`](../game-knowledge/README.md), [`game-provisioning`](../game-provisioning/README.md), [`game-livelihoods`](../game-livelihoods/README.md), [`game-relationships`](../game-relationships/README.md), [`game-traits`](../game-traits/README.md), [`game-lineage`](../game-lineage/README.md).
- **Testing:** `cargo test -p game-wayfaring` verifies frontier selection, layover timing, return trips, discovery value updates, and save/load state.
