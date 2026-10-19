# game-poetics

Pure procedural genre, meter, repetition, and musical note helpers for composed literary works.

**Why it matters:** Separating pure poetic form, line structure, and melody generation from stateful inventory and social transmission keeps `game-works` focused on simulation rules while making literary structure easy to test in isolation.

## Responsibilities

- **Poetic Structure:** Formats phoneme token sequences and glosses into verse lines, refrains, and repetition patterns for poems.
- **Musical Note Sequences:** Generates deterministic note sequences and rhythmic structures for composed songs.
- **Book & Prose Layout:** Structures longer multi-line compositions for written pages and bound books.

## Boundaries

- **Pure Formatting Library:** Owns no ECS state, registers no simulation plugin, and performs no inventory or social checks.
- **No Script or Vocabulary Storage:** Receives vocabulary tokens from `game-works` / `game-communication` and leaves script glyphs to `game-literacy`.

## How It Works

- **Deterministic Structural Rules:** Applies genre-specific templates (poem, song, prose book) to caller-supplied vocabulary tokens and seeds to produce structured line and note arrays.

## Game Integration

- **Plugin / Entry Point:** Pure library crate; requires no `game-sim` plugin registration.
- **Requests & Events:** None.
- **Callers:** Used directly by `game-works` when constructing new compositions and formatting `view` / `Read` outputs.

## Dependencies & Testing

- **Dependencies:** Pure Rust / serialization types (`serde`).
- **Testing:** Tested via `cargo test -p game-works` covering poem repetition, song note sequences, and line boundary preservation.
