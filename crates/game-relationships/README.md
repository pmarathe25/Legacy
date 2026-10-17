# game-relationships

Directed respect and admiration are separate from affection, trust, fear, and grievance. `Regard` applies an observer's interpretation of personally experienced actions. `HearOpinion` stores an attributed expression with its intermediary, time, and visible coercion context; it does not assert the speaker's true private belief. `ReviseOpinion` gradually updates an existing social belief or a learned concept value.

`apparent_difference` compares only the listener's own views and remembered expressions. Private counterpart beliefs are not a source of conversational knowledge. Heard testimony and regard retain ordinary transaction rollback and default cleanly when loading earlier saves.

Directed trust, affection, fear and grievance; separate social beliefs and slowly learned concept values. Interpretation requires an explicit credible observation.

Privileged `Initialize` and `SetRelationship` requests construct authored beliefs, values, and directed starting bonds. `Converse` applies a bounded personal impression supplied by the social-life owner, changing affection, trust, and grievance. `LearnValue` retains its bounded learning rate. Range checks apply to saved state and changed records; sharing, authority, and honesty remain separate beliefs.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-relationships`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.

`SufferForce` records direct bodily harm as fear, distrust, and grievance. It grants no affection or political legitimacy. The conflict/pressure path supplies actual witnessed hits; a rumor or unrelated injury does not identify an aggressor.
