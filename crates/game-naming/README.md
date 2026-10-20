# game-naming

Autonomous child naming tributes, phoneme recombination, conversational nicknames/epithets, and identity reconsideration.

**Why it matters:** Instead of assigning static random strings or numeric suffixes, characters name children after loved ones, recombine familiar spoken sounds, coin epithets from witnessed deeds, and occasionally adopt a nickname that fits who they have become.

## Responsibilities

- **Child Name Selection:** Supplies `game-names::SuggestChild` by honoring admired/loved acquaintances or recombining phoneme sequences from names the parent has actually heard.
- **Nicknames, Titles & Epithets:** Generates affectionate nicknames during conversations, personal titles from recognized institutional offices (`game-institutions`), and epithets inspired by medical care (`game-care`), creative works (`game-works`), or survived violence (`game-conflict`).
- **Identity Reconsideration (`Reconsider`):** Weighs attachment to a character's current name (and the relationships backing those who offered it) against self-expression and heard nicknames.

## Boundaries

- **No Separate Identity Store:** Stores all resulting names, histories, and forms of address in `game-names`.
- **Player Autonomy Preserved:** Excludes the active player character from autonomous self-renaming (`Reconsider`).

## How It Works

- **Confusion & Tribute Scoring:** Candidate child names balance tribute motivation (affection, respect, admiration, empathy) against a confusion penalty that rises when many important local acquaintances already share similar-sounding names.
- **Patience-Buffered Stability:** In `Reconsider`, adopting a new name must overcome both the relationship support of those who gave the current name and a patience-scaled switching margin, keeping identities stable unless relationships or experiences shift markedly.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers `SuggestChild` hooks, `Reconsider` handlers, and event listeners for care, works, institutions, and conflict in `game-assembly`.
- **Requests & Events:** Handles `Reconsider` and naming triggers; dispatches `game-names::Change` and `game-names::Label` requests.
- **Callers:** Invoked by `game-names`, `game-social-life`, and campaign/scenario setups.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-transport`, `game-physiology`, `game-traits`, `game-communication`, `game-relationships`, `game-social-life`, `game-names`, `game-care`, `game-works`, `game-institutions`, `game-conflict`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-naming`) cover tribute naming, phoneme recombination, confusion avoidance, epithet generation from deeds, and identity reconsideration stability.

Inherited rigidity contributes to the cost of changing a chosen name. Equal offers
retain the incumbent; a low-rigidity individual can respond to a smaller change
in regard, while stronger evidence can still change a high-rigidity identity.
