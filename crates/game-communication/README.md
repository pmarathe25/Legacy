# game-communication

Procedural vocabularies, sound–meaning associations, physical demonstration grounding, and nonverbal cue interpretation.

**Why it matters:** Language in Legacy is not a shared global string table. Characters coin words from a physical phoneme palette, ground meanings through local demonstration, and bridge unfamiliar dialects through practice and gesture.

## Responsibilities

- **Personal Vocabularies:** Tracks each person's sound–meaning associations with heard form, source, strength, and observation timestamp.
- **Expression & Coining (`Convey`, `Coin`, `WordFor`):** Selects a speaker's preferred word or coins a new phoneme sequence, grounding unfamiliar concepts via visible demonstrations or shared words.
- **Conversational Learning (`Practice`):** Runs bounded local vocabulary experiments during social contact so communities organically converge on shared dialects.
- **Capability & Nonverbal Channels (`Notice`):** Scales speech, hearing, gesture, and interpretation by traits and vitality (`game-traits`, `game-vitality`), and re-exports nonverbal cue handling from `game-signals`.

## Boundaries

- **No Pre-Built Dictionary:** Starts with no universal language, inherited vocabulary, or genetic word meanings.
- **No Written Scripts:** Does not manage physical pages, ink, or glyph proficiency (owned by `game-literacy`).

## How It Works

- **Sensory & Cognitive Gating:** Checks vocalization, hearing, and gesturing capabilities before transmitting a message. Deaf characters can learn concepts from visible demonstrations without acquiring acoustic forms.
- **Reader-Side Decoding:** `decode` translates forms strictly through the listener's own vocabulary table, leaving unfamiliar words untranslated while reinforcing successful matches.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers vocabulary tables, learning logs (`communication.learned`), and `Convey` / `Practice` / `Notice` handlers in `game-assembly`.
- **Requests & Events:** Exposes `Convey`, `Practice`, `Notice`, `WordFor`, `Coin`, and `decode` APIs.
- **Callers:** Used by `game-testimony`, `game-inquiry`, `game-social-life`, `game-discourse`, `game-naming`, `game-literacy`, `game-nurture`, and `game-agreements`.

## Dependencies & Testing

- **Dependencies:** `game-lexicon`, `game-signals`, `game-core`, `game-sim`, `game-storage`, `game-physiology`, `game-vitality`, `game-traits`, `game-transport`, `game-cargo`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-communication`) verify word coining, demonstration grounding, dialect divergence, deafness/gesture fallback, infant cues, and multi-worker save/load.

Reverse lookup lives in `game-lexicon`; learning and social requests remain here.
The decoder keeps up to 128 recent personal vocabularies, 32,768 forms and a 4 MiB
index-data budget. Compact sound keys and pooled text preserve strongest-association
and lexical ties. Weak versions invalidate edits, rollback and loading. Oversized
inputs keep the complete scan. `shared_meaning` checks understood titles without
building a translation surface or copying the matched text.
