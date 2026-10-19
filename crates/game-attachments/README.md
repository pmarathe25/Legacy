# game-attachments

Autonomous choice policy for keepsakes, gift-giving, memory sharing, and tending or hunting local animals.

**Why it matters:** Sentimental attachments (`game-sentiment`) and animal bonds (`game-fauna`) need an active decision layer so characters seek out lost keepsakes, give meaningful gifts, reminisce, and interact with local wildlife.

## Responsibilities

- **Keepsake Recovery:** Proposes locating and retrieving cherished items (`game-items`, `game-sentiment`) from remembered containers (`game-knowledge`), recording disproved-location evidence if the item is gone.
- **Gifts & Reminiscing:** Proposes giving valued items to loved ones (`game-relationships`) and recounting personal attachment memories.
- **Fauna Interaction:** Proposes tending bonded local animals or hunting wild fauna (`game-fauna`) through `game-choice`.

## Boundaries

- **Choice Policy Only:** Owns no underlying item identity (`game-items`), animal biology (`game-fauna`), or sentiment ledger (`game-sentiment`).
- **No Belief Bypass:** Recovery journeys rely strictly on remembered container locations in `game-knowledge`.

## How It Works

- **Proposal Generation:** Evaluates a character's sentimental memories, local companions, and nearby fauna during `game-choice::Collect` to submit utility-weighted proposals.
- **Validated Retrieval:** Checks the remembered container upon arrival before transferring the keepsake via `game-cargo` and `game-property`; if missing, updates `game-knowledge` so the character stops repeating the failed search.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers attachment choice providers and execution handlers in `game-assembly`.
- **Requests & Events:** Submits proposals to `game-choice` and dispatches `game-cargo`, `game-property`, `game-knowledge`, `game-sentiment`, and `game-fauna` requests.
- **Callers:** Driven by `game-choice` decision cycles.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-choice`, `game-sentiment`, `game-fauna`, `game-cargo`, `game-objects`, `game-items`, `game-knowledge`, `game-property`, `game-relationships`, `game-vitality`, `game-physiology`, `game-transport`.
- **Testing:** Tested via `cargo test -p game-sentiment -p game-items` and assembly integration tests covering keepsake recovery, disproved locations, gifting, and fauna choices.
