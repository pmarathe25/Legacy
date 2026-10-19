# game-soma

`game-soma` owns authoritative living and deceased body records, validates damage, healing, exertion, and death transitions, and advances living bodies across ticks.

**Why it matters:** Isolating dense body storage (`Bodies`) from action orchestration (`game-physiology`) ensures only one owner mutates health, hunger, and fatigue while avoiding per-tick writes to thousands of deceased historical ancestors.

## Responsibilities

- **Bodies resource (`Bodies`):** Stores dense `game-packed` body rows, physiological `Rules`, phase clock, and a live-person index (`physiology.bodies`).
- **Validated body mutations:** Handles `SpawnBody`, `Damage`, `Heal`, `Exert`, `RestState`, `FeedBody`, and `TickBodies`.
- **Deferred dead-row projection:** Advances only living rows during periodic ticks while projecting deceased rows on read/save to preserve canonical save compatibility.

## Boundaries

- **State & validation owner:** Only `game-soma` edits `Bodies`; [`game-physiology`](../game-physiology/README.md) composes eating, serving, and resting actions over `game-soma`'s requests, and [`game-body`](../game-body/README.md) provides the pure math.

## How It Works

- **Living-only tick loop:** `living()` maintains an index of alive `PersonId`s rebuilt on load so parallel Rayon body ticks skip deceased rows even in long multigenerational histories.
- **Sorted mortality effects:** When starvation or wounds reduce a mortal's health to zero, death transitions and `Died` events are emitted in deterministic `PersonId` order; immortals enter in-place recovery instead.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (part of the `physiology` resource family, `physiology.bodies`).
- **Requests & Events:** Handles body mutation and tick requests; emits `Died`, `Starved`, and `Recovered`.
- **Callers:** Wrapped and re-exported by [`game-physiology`](../game-physiology/README.md); queried via `body()` and `living()`.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-storage`](../game-storage/README.md), [`game-packed`](../game-packed/README.md), [`game-body`](../game-body/README.md), [`game-metabolism`](../game-metabolism/README.md), [`game-nutrition`](../game-nutrition/README.md), [`game-vitality`](../game-vitality/README.md), [`game-traits`](../game-traits/README.md), [`game-diagnostics`](../game-diagnostics/README.md).
- **Testing:** `cargo test -p game-soma -p game-physiology` verifies deferred dead-row projection, nested rollback, worker count equivalence, and mortality ordering.
