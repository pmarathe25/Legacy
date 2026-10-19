# game-testimony

Spoken fact transmission (`Tell`), delayed report delivery (`Report`), and cognitive comprehension hooks.

**Why it matters:** Second-hand information carries the speaker's staleness and uncertainty, not objective world truth. `game-testimony` ensures passed facts preserve their original provenance, decay in confidence, and respect listener comprehension.

## Responsibilities

- **Direct Spoken Testimony (`Tell`):** Transfers a single selected fact between two co-located people who share an understood communication channel (`game-communication`).
- **Delayed Report Delivery (`Report`):** Delivers arranged reports that snapshot the sender's belief and confidence at sending time rather than remote reality at arrival time.
- **Comprehension Gating (`Comprehend`):** Invokes receiver-owned comprehension callbacks so complex causal claims (`game-inquiry`) can check cognitive capacity and instructional scaffolding.

## Boundaries

- **Single-Fact Scope:** Never transfers unrelated knowledge beyond the specific fact being told or reported.
- **No Belief Storage:** Stores resulting facts in `game-knowledge` rather than maintaining its own fact ledger.

## How It Works

- **Provenance & Confidence Decay:** Copies the speaker's recorded observation timestamp and commit order while attenuating confidence. Delayed reports never overwrite fresher evidence recorded at or after the report's original observation time.
- **Saved-Work Compatibility:** Retains historical `knowledge.*` request keys so serialized pending reports restore cleanly across saves.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers `Tell`, `Report`, and `Comprehend` handlers in `game-assembly`.
- **Requests & Events:** Exposes `Tell`, `Report`, and the `Comprehend` callback registration API.
- **Callers:** Used by `game-inquiry`, `game-social-life`, `game-discourse`, `game-missions`, and scenario fixtures.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-choice`, `game-knowledge`, `game-transport`, `game-communication`.
- **Testing:** Tested via `cargo test -p game-knowledge -p game-inquiry` covering co-location checks, language comprehension, confidence decay, and cognitive scaffolding.
