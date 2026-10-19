# game-relationships

Directed interpersonal bonds (affection, trust, fear, grievance, respect, admiration), social beliefs, and attributed opinions.

**Why it matters:** Cooperation, trade, care, and conflict depend on how individuals view one another. Separating affection, trust, fear, grievance, and respect—while tracking heard opinions separately from private beliefs—creates nuanced social dynamics.

## Responsibilities

- **Directed Interpersonal Bonds:** Tracks per-dyad affection, trust, fear, grievance, respect, and admiration (`Regard`, `Converse`, `SufferForce`).
- **Social Beliefs & Concept Values:** Stores personal norms (such as attitudes toward sharing, authority, and honesty) and slowly learned concept values (`ReviseOpinion`, `LearnValue`).
- **Attributed Opinions (`HearOpinion`):** Records what a speaker was heard expressing—along with intermediary, timestamp, and visible coercion context—without exposing the speaker's hidden private beliefs.
- **Apparent Difference Queries:** Computes `apparent_difference` strictly from the listener's own views and remembered expressions.

## Boundaries

- **No Telepathic Belief Access:** Never leaks a counterpart's unexpressed private norms during social evaluation.
- **No Conversation Scheduling:** Does not schedule social encounters or choose conversation partners (owned by `game-social-life` and `game-companionship`).

## How It Works

- **Experience-Driven Updates:** `Converse` applies bounded affection, trust, and grievance shifts from social encounters; `Regard` updates respect and admiration from witnessed competence or generosity; `SufferForce` records direct witnessed violence as fear, distrust, and grievance without granting legitimacy.
- **Validated Bounded Storage:** Enforces numeric bounds on all bond channels and rolls back cleanly across simulation checkpoints.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers directed relationship tables, opinion records, and mutation handlers in `game-assembly`.
- **Requests & Events:** Exposes `Initialize`, `SetRelationship`, `Converse`, `Regard`, `SufferForce`, `HearOpinion`, `ReviseOpinion`, `LearnValue`, and bond/opinion query views.
- **Callers:** Queried and updated by `game-social-life`, `game-companionship`, `game-discourse`, `game-nurture`, `game-care`, `game-livelihoods`, `game-exchange`, `game-credit`, `game-agreements`, `game-conflict`, and `game-pressure`.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-relationships`) verify directed bond updates, attributed opinion provenance, coercion context, force grievances, and rollback/save consistency.
