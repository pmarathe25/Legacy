# game-companionship

Autonomous companion search (`SeekCompany`) and conversation initiation proposals for `game-choice`.

**Why it matters:** Separating the desire to seek company from the physical mechanics of talking lets characters scan local neighbors, handle refusals gracefully, and propose conversations alongside work and rest without deadlocking on uncooperative peers.

## Responsibilities

- **Companion Search (`SeekCompany`):** Evaluates local neighbors from `game-social-life` and proposes concrete conversation offers to `game-choice`.
- **Bounded Cursor Advancement:** Advances a person's search cursor (`game-social-life::Scanned`) after actual attempts—including observed refusals—so hostile early neighbors never permanently block discovery of later companions.
- **Continuation Satisfaction:** Recognizes when an incoming conversation initiated by someone else already satisfies a character's desire for company, preventing mutual cancellation loops.

## Boundaries

- **No Physical Conversation State:** Delegates `Talk` execution, recipient consent, contact history, and area residency to `game-social-life`.
- **No Bond Storage:** Reads directed bonds and social values from `game-relationships`.

## How It Works

- **Local Invitations:** A willing local partner is reserved by the timed conversation itself. Refusal advances the search cursor and returns `Outcome::Reconsider` directly to the chooser.
- **Changing Partners:** Switching from an existing meeting takes a brief, cancellable deliberation step, so asymmetric preferences cannot produce endless changes at one instant.
- **Legacy Key Compatibility:** Retains the `social-life.seek` request key for `SeekCompany` so saved agendas and fixtures deserialize seamlessly.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers `SeekCompany` handlers and `game-choice` proposal providers alongside `game-social-life` in `game-assembly`.
- **Requests & Events:** Handles `SeekCompany` requests and submits conversation proposals during `game-choice::Collect`.
- **Callers:** Invoked by `game-choice` and `game-characters`.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-choice`, `game-utility`, `game-physiology`, `game-transport`, `game-relationships`, `game-traits`, `game-social-life`.
- **Testing:** Tested via `cargo test -p game-social-life` and assembly tests covering bounded neighbor scanning, refusal recovery, and continuation handling.

Current partners remain available for reevaluation outside the bounded alternative
search. Their ordinary motives and attention margin still compete with new company
and other needs, including when a settlement exceeds the recall window.
