# game-food-memory

`game-food-memory` indexes a character's remembered inventories, edible portions, and carried containers from subjective [`game-knowledge`](../game-knowledge/README.md) facts and scores foods against dietary needs.

**Why it matters:** Scanning all historical facts on every hunger review is expensive; maintaining a process-local derived index updated only on `game-facts` payload deltas keeps meal and foraging queries fast without mutating gameplay state.

## Responsibilities

- **Derived store index (`with_stores`):** Projects a character's remembered inventory locations, carriers, and edible quantities from `game-knowledge`, refreshing only when fact payload revision tokens change.
- **Contextual dietary appraisal:** Combines remembered edible stocks with [`game-nutrition`](../game-nutrition/README.md) `appetite_for` scores, personal self-provision preferences, and an intelligence-bounded recall window (up to 64 external stores).

## Boundaries

- **Read-only projection utility:** Owns no `game-sim` plugin or saved resource, and never reads actual world inventories or teaches new facts; [`game-provisioning`](../game-provisioning/README.md) owns food acquisition planning.

## How It Works

- **Bounded LRU cache:** Retains projections for at most 1,024 recently queried characters in process-local memory; cache eviction or rollback rebuilds the exact same view from committed facts.
- **Intelligence-scaled recall:** Personal carried packs and owned stores are always considered, while comparison across other remembered stores scales with expressed intelligence from [`game-vitality`](../game-vitality/README.md).

## Game Integration

- **Plugin / Entry Point:** Pure query library (`with_stores`, appraisal helpers); registers no `game-sim` plugin.
- **Requests & Events:** None.
- **Callers:** Used by [`game-provisioning`](../game-provisioning/README.md), [`game-characters`](../game-characters/README.md), [`game-livelihoods`](../game-livelihoods/README.md), [`game-care`](../game-care/README.md), and [`game-households`](../game-households/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-knowledge`](../game-knowledge/README.md), [`game-objects`](../game-objects/README.md), [`game-nutrition`](../game-nutrition/README.md), [`game-property`](../game-property/README.md), [`game-vitality`](../game-vitality/README.md).
- **Testing:** `cargo test -p game-food-memory` verifies alias handling, moving carried containers, fact rollback invalidation, and nested query safety.
