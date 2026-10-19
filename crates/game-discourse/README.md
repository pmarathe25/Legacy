# game-discourse

Personal advocacy, persuasion tactics, third-party citations, and coerced public endorsements during conversations.

**Why it matters:** Beliefs and reputations spread through argument, appeal, reputation, and threat. `game-discourse` lets speakers advocate learned positions using tactics shaped by their personality—while listeners weigh claims critically and spot contradictions.

## Responsibilities

- **Conversational Advocacy:** Composes persuasion attempts onto completed `game-social-life` conversations when a speaker's assertiveness and conviction motivate pressing a position.
- **Tactical Approach Selection:** Chooses between reasoned explanation, relationship appeal, citing a third party's stance, or threatening coercion based on foresight, honesty, sociability, aggression, and past experience.
- **Listener Evaluation & Credibility:** Evaluates heard claims through the listener's openness, conviction, independence, and directed bonds (`game-relationships`), penalizing a reporter's credibility if a cited person later states a contradicting view directly.
- **Coerced Endorsements:** Connects with `game-pressure` so physical intimidation can force a public endorsement (`HearOpinion` with visible coercion context) without altering the victim's private conviction.

## Boundaries

- **No Genetic Opinions:** Genes influence personality traits and vitality (`game-traits`, `game-vitality`), never the content of an opinion.
- **No Direct Bond Ownership:** Stores all resulting heard opinions, credibility shifts, and regard changes in `game-relationships`.

## How It Works

- **Public Speech Events (`Spoken`):** Emits public claims and observed delivery quality (gated by `game-communication` vocabulary comprehension) without leaking hidden truth flags or private listener belief deltas.
- **Gift & Care Impressions:** Also evaluates voluntary food and care offers through the recipient's own dietary memory and consent, updating directed gratitude and regard.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers discourse hooks on completed conversations, gift/care impression listeners, and compact speech history logs in `game-assembly`.
- **Requests & Events:** Emits `Spoken` events and dispatches `game-relationships::HearOpinion`, `ReviseOpinion`, and `Regard` updates.
- **Callers:** Triggered by `game-social-life`, `game-pressure`, `game-nurture`, and `game-care`.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-diagnostics`, `game-objects`, `game-cargo`, `game-transport`, `game-physiology`, `game-vitality`, `game-traits`, `game-communication`, `game-knowledge`, `game-food-memory`, `game-relationships`, `game-social-life`, `game-pressure`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-discourse`) cover personality-driven advocacy, resistance to admired speakers, false citations and counterevidence, coerced statements vs. private belief, and save/load consistency.
