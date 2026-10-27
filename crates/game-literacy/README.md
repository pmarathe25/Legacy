# game-literacy

Invented writing scripts, personal reading/writing proficiency, and physical written records.

**Why it matters:** Oral testimony fades or distorts across generations. Inventing scripts and inscribing physical pages lets societies preserve exact assertions, provenance, and causal knowledge across time and distance—if future readers know the script.

Literacy opportunity discovery opts into the experimental fast-review scope;
ordinary and full reviews continue to offer all available literacy actions.
An opt-in experimental character policy enables fast reviews between full deadlines.
`reading_signature` exposes the comprehension retry signature used by the
information owner; it is not a check that a document is currently accessible.

## Responsibilities

- **Script Invention & Proficiency:** Tracks invented writing systems and each character's reading/writing proficiency without assuming any initial universal script.
- **Inscribing Records:** Consumes paper, ink (`game-cargo`, `game-items`), and focused time to encode assertions, provenance, conditions, and uncertainty into script-specific glyphs on physical pages.
- **Reading & Teaching:** Requires physical access to the page, sensory vision/interpretation, script proficiency, language familiarity (`game-communication`), and cognitive capacity (`game-reasoning`) to decode written claims or teach scripts to peers.

## Boundaries

- **No Automatic Omniscience:** Seeing a page in passing does not reveal privately carried copies or translate an unknown script.
- **No Literary Art Valuation:** Aesthetic composition and paid creative works are owned by `game-works` and `game-poetics` (though literary writing can practice a script).

## How It Works

- **Glyph-Encoded Assertions:** Each inscribed item stores the exact claim and provenance from `game-knowledge` alongside the script ID used to write it.
- **Multi-Gate Decoding:** When a reader inspects a written object, `game-literacy` verifies property/location access, checks script proficiency and underlying language vocabulary, and feeds comprehended claims into `game-knowledge`.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers script registries, personal proficiency tables, and inscription/reading choice handlers in `game-assembly`.
- **Requests & Events:** Exposes script development, note inscription, reading, and script-teaching actions via `game-choice`.
- **Callers:** Coordinates with `game-knowledge`, `game-communication`, `game-reasoning`, `game-social-life`, and `game-works`.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-choice`, `game-reasoning`, `game-knowledge`, `game-communication`, `game-items`, `game-cargo`, `game-objects`, `game-property`, `game-transport`, `game-physiology`, `game-vitality`, `game-relationships`, `game-social-life`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-literacy`) cover script invention, distinct encoded assertions, unreadability of unknown scripts, physical access rules, and save/load consistency.

Writing uses `game-inscription` for pigment, carving, or impression on actual
physical materials. `Develop` charges practice time; `Inscribe` retains the marked
item and method in its document. Stone, fabric, clay, bark, wood, and regional
materials can support writing. Tool compatibility is checked when marking begins; incision tools and the physical surface must remain available at completion;
interrupted work does not create a finished document. Reading still requires
script proficiency, understandable language, and sufficient cognitive capacity.
Rigidity can reject a comprehensible contradictory explanation.
