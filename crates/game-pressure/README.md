# game-pressure

Owns local demands for submission and the pending connection between those demands and actual conflict. It creates no government or permission by itself. `game-communities` decides whether a character attempts a demand and how another person's response changes social arrangements.

`DemandSubmission` requires both people alive, capable, idle, and together. Current player responses require an explicit control path and are not resolved automatically. A person already sufficiently afraid can comply. Otherwise the demand starts an ordinary `game-conflict` encounter; both people reserve the canonical action clock, can fight back, suffer real bodily damage, and can die. There is no separate damage resolver or protective damage cap.

The conflict owner emits attributed `Struck` evidence after simultaneous round damage. A direct victim learns fear, distrust, and grievance through `game-relationships::SufferForce`. Fear is compared with that person's risk tolerance, grievance, and attitude toward authority. The subject may submit, the aggressor may abandon the demand, or fighting may continue. Submission is an event, not affection, legitimacy, a ballot, or automatic ownership. An upper social owner can act on that response.

Completion, interruption, retreat, incapacity, and death release the pending pair. An interrupted demand grants no submission. Pending engagements save with symmetric participant references and an actual battle; save validation checks actions and live fighters. No independent review clock is added.

This uses the current tactical model's simple local encounter geometry, damage, and timing. Coalitions, negotiated threats, witnesses' indirect fear, larger military power, and player-facing responses are not yet implemented.

Run `cargo test -p game-pressure`. Tests cover cancellation and death of either participant, actual fatal resistance, distance, player control, and save/load. Dependencies are `game-core`, `game-sim`, `game-storage`, `game-physiology`, `game-transport`, `game-conflict`, `game-relationships`, `game-traits`, `game-lineage`, and `serde`.
