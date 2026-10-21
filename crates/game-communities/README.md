# game-communities

Emergent group organizing, informal supply sharing, personally constructed influence arrangements, and coercive submission.

**Why it matters:** Communities shouldn't spawn with a preset government type. Friends start by sharing supplies informally, develop arrangements about whose opinions to consider from experience, or comply under physical threat while harboring private resentment.

## Responsibilities

- **Organizing Decisions:** Evaluates group formation and governance choices after social encounters and during `game-characters::IdleReview`.
- **Personal Attachment Modes (`Attachment`):** Tracks each member's private reason for participating—informal friendship (`Personal`), valuing a voice in decisions (`shared-decisions`), trust in an admired leader, or fear-driven compliance after `game-pressure`.
- **Practical Authority Gating (`CanCommit`):** Implements `game-institutions::CanCommit` so a leader's decree or group ballot only commits a custodian's supplies if that custodian still willingly follows or fears the authority.
- **Authority Withdrawal:** Allows idle followers who lose loyalty or escape immediate coercion to withdraw their granted `game-property` permissions.

## Boundaries

- **No Preset Government Tree:** Does not pick a global "government type" for a settlement; governance is the sum of recorded `game-institutions` procedures and private member attachments.
- **Player Autonomy Preserved:** Excludes the active player from implicit permission grants and automatic submission.

## How It Works

- **Experience-Driven Governance:** Successful informal sharing teaches egalitarian peers to value `shared-decisions` and propose `Vote` procedures in `game-institutions`. Conversely, ambitious and aggressive characters can demand submission via `game-pressure`, producing fearful compliance without affection or genuine loyalty.
- **Observer-Relative Privacy:** Local observations reveal an institution's public procedure and membership list, plus only the observer's own private `Attachment`—never another member's hidden fear or loyalty.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers community attachment tables, `CanCommit` handlers, and `IdleReview` / encounter organizing hooks in `game-assembly`.
- **Requests & Events:** Handles `Enable`, organizing requests, and `CanCommit`; dispatches `game-institutions`, `game-property`, and `game-pressure` requests.
- **Callers:** Coordinates with `game-characters`, `game-institutions`, `game-affiliation`, `game-pressure`, and `game-property`.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-objects`, `game-cargo`, `game-property`, `game-transport`, `game-physiology`, `game-traits`, `game-knowledge`, `game-relationships`, `game-lineage`, `game-institutions`, `game-pressure`, `game-characters`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-communities`) cover informal sharing, constructed influence arrangements, admired leadership, withdrawal of authority, coerced compliance, and private observation boundaries.

`Cooperate` succeeds when an agreement, permission, procedure, or attachment
actually changed. An unrealized request or an already fulfilled pact supplies
failure feedback to the chooser, which can weigh other goals and retry when
worthwhile. Revoked permissions and changed beliefs can make renegotiation
useful again. No care duty, mandatory cooperation, or retry ban is introduced.

Autonomous joint arrangements now carry explicit participant influence and required support via `game-deliberation`. Renewing cooperation preserves an existing agreement; changing it is a separate deliberation. Historical procedure names remain loadable.
