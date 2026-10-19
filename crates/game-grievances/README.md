# game-grievances

Witnessed property interference, empathy-weighted forgiveness vs. resentment, and voluntary local retaliation.

**Why it matters:** Taking someone's food or working their garden plot shouldn't trigger an omniscient police response. Only awake, present witnesses notice the interference, weigh whether it was driven by desperate need, and decide whether to forgive or retaliate.

## Responsibilities

- **Witnessed Interference Evaluation:** Evaluates contested inventory transfers (`game-property`) and unauthorized plot cultivation (`game-horticulture`) strictly for co-located, awake, living observers.
- **Forgiveness vs. Grievance:** Weighs the witness's recognized property claims, empathy (`game-traits`), and the actor's observable necessity (such as starvation) to either forgive the act or record a directed grievance in `game-relationships`.
- **Voluntary Retaliation:** Proposes local retaliation decisions by weighing accumulated grievance and aggression against patience and fear, initiating ordinary `game-conflict` actions when chosen.

## Boundaries

- **No Omniscient Witnesses:** Absent, sleeping, or dead characters never learn of an interference event automatically.
- **No Ambient Criminal Code:** Retaliation is a personal local choice; institutional prosecution belongs to `game-justice`.

## How It Works

- **Event-Driven Observation:** Subscribes to contested use notifications from `game-property` and `game-horticulture`, filtering local bystanders through `game-physiology` and `game-transport`.
- **Choice-Based Escalation:** When a resentful witness later encounters the offender locally, `game-grievances` scores a retaliation offer against other personal priorities.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers interference listeners and retaliation choice handlers in `game-assembly`.
- **Requests & Events:** Consumes contested property/plot events; dispatches `game-relationships` grievance updates and `game-conflict` encounters.
- **Callers:** Triggered by `game-property`, `game-horticulture`, and `game-choice`.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-objects`, `game-cargo`, `game-property`, `game-transport`, `game-physiology`, `game-vitality`, `game-traits`, `game-relationships`, `game-horticulture`, `game-conflict`, `game-choice`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-grievances`) cover witnessed vs. unwitnessed theft, empathy-driven forgiveness of starving actors, plot interference, and local retaliation.
