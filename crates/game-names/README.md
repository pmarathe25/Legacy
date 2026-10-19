# game-names

Personal identity histories, heard-name memories, and speaker-specific nicknames, titles, and epithets.

**Why it matters:** Names are social facts, not global string labels. A person's chosen name, the old name a distant acquaintance still remembers, and a local nickname or epithet can all differ at the same time.

## Responsibilities

- **Identity & Name History:** Stores original given names, full chosen-name histories (`Change`), and immutable person IDs that distinguish individuals who share a name.
- **Observer-Relative Memory (`NameFor`):** Tracks which names each observer has actually heard from or for specific people; knowing a person's ID never automatically reveals their current name.
- **Forms of Address (`Label`):** Records each speaker's active nickname, title, and epithet per person without altering the recipient's self-chosen identity or granting political office.
- **Child Naming Hooks (`NameChild`, `SuggestChild`):** Requests a present parent or caregiver to name an unnamed child, delegating candidate generation to `game-naming` when registered.

## Boundaries

- **No Candidate Scoring:** Does not decide which name a parent prefers or when an adult wants to rename themselves (owned by `game-naming`).
- **No Public Event Projection:** Coordinates with `game-social-life` for public registration and rename events.

## How It Works

- **Communication-Gated Learning:** When `game-communication` is active, a rename (`Change`) or child naming (`NameChild`) must be understandably presented before local observers update their heard-name table. Silent renames stay private, and distant or deaf observers retain older names until informed.
- **Copy-on-Write History Validation:** Shares immutable name-history prefixes across checkpoints, validating only newly appended entries during normal execution and full histories on load.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers identity records, heard-name tables, and naming request handlers in `game-assembly`.
- **Requests & Events:** Exposes `NameChild`, `SuggestChild`, `Change`, `Label`, and `NameFor` queries.
- **Callers:** Used by `game-naming`, `game-social-life`, `game-parenthood`, `game-nurture`, and UI/CLI presentation layers.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-transport`, `game-physiology`, `game-communication`, `game-social-life`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-names`) verify child naming, local vs. distant rename propagation, communication gating, speaker labels, and save/load validation.
