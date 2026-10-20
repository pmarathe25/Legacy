# game-pressure

Coercive demands (`DemandSubmission`, `Press`), refusal escalation into combat, bystander intervention, and road-based escape.

**Why it matters:** Coercion bridges social politics and physical violence. An ambitious aggressor can demand allegiance or a public endorsement, forcing the target to weigh compliance, flight along a known road, or fighting back—with real bodily wounds and mortality at stake.

## Responsibilities

- **Coercive Demands (`DemandSubmission`, `Press`):** Initiates local demands for either political allegiance or public endorsement of a specific stance between co-located, capable characters.
- **Anticipated Danger & Refusal:** Lets the target weigh compliance against resistance or flight using remembered fear, visible build, own wounds, foresight, independence, and conviction—without peeking at the aggressor's hidden traits.
- **Combat Escalation & Submission:** Escalates refused demands into real `game-conflict` battles if the aggressor chooses not to back down (`BackedDown`), re-evaluating submission, flight, or continued fighting after each round's `Struck` damage and `game-relationships::SufferForce` updates.
- **Escape & Bystander Aid:** Allows targets to flee (`Fled`) along a personally remembered outgoing road (`game-knowledge`, `game-transport`) and lets local friends weigh their regard and risk before intervening.

## Boundaries

- **Submission Is Not Legitimacy:** Emitting `Submitted` records fearful compliance—never affection, trust, a democratic ballot, or automatic ownership. Upper owners (`game-communities`, `game-discourse`) decide how to apply the result.
- **Player Autonomy Preserved:** Never auto-resolves compliance or surrender for the active player character.

## How It Works

- **Shared Tactical Lifecycle:** Reserves both participants' action clocks and links the pending demand to an active `game-conflict` encounter. If a remembered escape road turns out to be closed, withdrawal rolls back and leaves the engagement intact.
- **Symmetric Persistence:** Persists pending demands, their purpose (allegiance vs. endorsement), and linked combat pairs across save/load checkpoints.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers demand state, `DemandSubmission` / `Press` handlers, and `game-conflict` round listeners in `game-assembly`.
- **Requests & Events:** Exposes `DemandSubmission` and `Press`; emits `Submitted`, `BackedDown`, and `Fled` events.
- **Callers:** Invoked by `game-communities` and `game-discourse`; drives `game-conflict`, `game-relationships`, and `game-transport`.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-physiology`, `game-transport`, `game-conflict`, `game-relationships`, `game-traits`, `game-lineage`, `game-knowledge`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-pressure`) cover anticipated force, refused escalation, fatal resistance, road escape, closed-road rollback, friend intervention, endorsement coercion, and save/load consistency.

`game-violence` supplies learned moral costs and experience-driven attitude changes. Personal peaceful convictions can outweigh inherited aggression; learned violent convictions can encourage force. These remain competing motives, without a compulsory peaceful or violent social system.
