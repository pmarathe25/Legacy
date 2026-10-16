# game-relationships

Directed trust, affection, fear and grievance; separate social beliefs and slowly learned concept values. Interpretation requires an explicit credible observation.

Privileged `Initialize` and `SetRelationship` requests construct authored beliefs, values, and directed starting bonds. `Converse` applies a bounded personal impression supplied by the social-life owner, changing affection, trust, and grievance. `LearnValue` retains its bounded learning rate. Range checks apply to saved state and changed records; sharing, authority, and honesty remain separate beliefs.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-relationships`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.

`SufferForce` records direct bodily harm as fear, distrust, and grievance. It grants no affection or political legitimacy. The conflict/pressure path supplies actual witnessed hits; a rumor or unrelated injury does not identify an aggressor.
