# game-signals

Nonverbal need expression, infant distress cries, and observer-relative cue interpretation.

**Why it matters:** Infants and people without shared words still express hunger, fatigue, and distress. `game-signals` lets observers notice and interpret nonverbal cues locally—including realistic ambiguity and misinterpretation.

## Responsibilities

- **Nonverbal Expression:** Emits cries, gestures, and physical distress cues when characters experience unmet physiological needs (`game-physiology`, `game-vitality`).
- **Contact-Gated Perception:** Requires actual co-location—including shared travel—for an observer to perceive a signal (`game-transport`).
- **Fallible Interpretation:** Estimates the sender's need using the observer's own sensory/interpretive capabilities and prior caregiving observations, without leaking exact hidden hunger values.

## Boundaries

- **No Symbolic Vocabulary:** Does not manage coined phoneme words or spoken dialects (owned by `game-communication`).
- **No Care Execution:** Does not feed or shelter the signaling person directly (owned by `game-nurture` and `game-care`).

## How It Works

- **Cue Emission & Observation:** When a dependent or distressed character signals, co-located observers run capability checks (`game-vitality`) to form a coarse impression of the need—which can fail or mistake fatigue for hunger.
- **Caregiver Foresight:** Allows caregivers who personally witnessed a prior feeding to anticipate recurring hunger from elapsed time even before a fresh cry is understood.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers nonverbal signal state and cue handlers in `game-assembly` (with compatibility re-exports through `game-communication`).
- **Requests & Events:** Handles nonverbal signal emission and observer notice requests.
- **Callers:** Used by `game-communication`, `game-nurture`, and `game-care`.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-choice`, `game-physiology`, `game-objects`, `game-transport`, `game-vitality`.
- **Testing:** Tested via `cargo test -p game-communication -p game-campaign` covering infant cues, sensory impairments, co-location requirements, and caregiver anticipation.

Practiced `child-care` skill improves interpretation and anticipated hunger
without changing willingness to help. `game-observations` awards practice for
actual feeding and comprehensible care lessons; genes influence the abilities
used to learn rather than transmitting acquired knowledge.
