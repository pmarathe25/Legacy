# game-works

Procedural cultural compositions (poems, songs, books), physical manuscript creation, and oral or written circulation.

**Why it matters:** Creative pastimes can leave lasting cultural artifacts behind. Characters compose poems, songs, and books in their own spoken vocabularies, inscribe them onto physical pages, and retell or read them across generations.

## Responsibilities

- **Work Composition:** Turns completed creative `game-pastimes` (`compose-poem`, `compose-song`, `write-book`) into distinct cultural work records capturing creator, place, time, phonetic tokens (`game-communication`), intended glosses, line structure (`game-poetics`), and musical notes.
- **Physical Manuscripts:** Converts consumed paper into tracked `game-items` pages or bound books in `game-cargo` whose identity persists across transfers and destruction.
- **Oral & Written Sharing (`Present`, `Shared`):** Allows participants in completed `game-social-life` conversations to perform a remembered oral work or present an accessible text (evaluating up to eight known works per encounter).
- **Reader-Relative Decoding (`Read`):** Decodes a read or heard work strictly through the audience's own learned vocabulary (`game-communication`), leaving unfamiliar words untranslated rather than leaking the author's private gloss.

## Boundaries

- **No Automatic Broadcast:** Merely finishing a poem or song does not grant nearby bystanders an audience record or opinion without an actual conversation or presentation.
- **Pure Phrasing Delegated:** Delegates structural line, meter, and genre formatting to `game-poetics`.

## How It Works

- **Repertoire Indexing:** Maintains a derived per-person repertoire index (rebuilt on load) so conversations quickly select from up to eight personally known works without scanning global archives.
- **Posthumous Survival:** Because listeners learn the phonetic tokens of an oral work and books persist as physical `game-items`, works continue circulating long after their creator dies.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers work archives, repertoire indices, pastime completion hooks, and `Present` / `Read` handlers in `game-assembly`.
- **Requests & Events:** Exposes `Present`, `Read`, and `view` queries (bound to CLI `read-work` and `show-work`); emits `Shared` cultural transmission events used by `game-naming`.
- **Callers:** Triggered by `game-pastimes` and `game-social-life`; queried by `game-naming`, `game-api`, and `game-cli`.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-diagnostics`, `game-objects`, `game-cargo`, `game-items`, `game-property`, `game-transport`, `game-physiology`, `game-vitality`, `game-traits`, `game-communication`, `game-knowledge`, `game-relationships`, `game-social-life`, `game-pastimes`, `game-poetics`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-works`) cover composition costs, poems/songs/books, reader-specific decoding, manuscript destruction, posthumous retelling, and save/load consistency.

Works can preserve personally known theories and memories. Reading or hearing
understood meanings transmits their retained evidence through `game-testimony`;
untranslated tokens grant no facts. A physical care manual can therefore teach
future caregivers after its author dies, subject to language and comprehension.
