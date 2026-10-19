# game-lineage

Dynastic ruler succession, persistent past-life archives, and post-death reincarnation into real births.

**Why it matters:** In Legacy, the player is a mortal ruler whose death leaves the world running on its own until a new child is born to inherit the dynasty's latent past-life memories and skills.

## Responsibilities

- **Mortal Ruler Lifecycle:** Tracks the active ruler, records their death (`game-physiology`), and preserves a persistent archive of completed past lives (skills and personal knowledge snapshots).
- **Succession Gaps:** Allows the simulation to advance through indefinite rulerless interregnum periods while communities, seasons, and families continue autonomously.
- **Birth-Driven Reincarnation:** Selects the next ruler exclusively from actual post-death births (`game-lifecycle`)—never fabricating a child on demand.

## Boundaries

- **No Automatic Office Restoration:** Inheriting the dynastic lineage never automatically restores political offices, budgets (`game-realm`), or property claims (`game-property`) held by a previous incarnation.
- **No Active Memory Bleed:** Archived past-life skills and knowledge remain dormant until explicitly channeled via `game-invocation`.

## How It Works

- **Death Snapshot & Interregnum:** When the current ruler dies, `game-lineage` snapshots their final `game-skills` and `game-knowledge` records into the lineage archive and enters a waiting state.
- **Birth Hook Selection:** Listens to `game-lifecycle` birth events after the ruler's death and binds the lineage to an eligible newborn according to the scenario's succession policy.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers lineage state, past-life archives, and mortality/birth listeners in `game-assembly`.
- **Requests & Events:** Exposes active ruler queries, past-life archive views, and succession configuration; emits ruler death and succession events.
- **Callers:** Queried by `game-invocation`, `game-guardians`, `game-characters`, `game-family-life`, `game-communities`, `game-livelihoods`, `game-session`, and `game-api`.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-physiology`, `game-lifecycle`, `game-skills`, `game-knowledge`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-lineage`) verify ruler death archiving, multi-year succession gaps, post-death birth selection, and save/load consistency.
