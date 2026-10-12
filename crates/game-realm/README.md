# game-realm

Realm owns regional negotiation, ratification evidence, personal judgments, work selection, reports, deadlines, and reconciliation. Commitments promise deliveries, passage, or patrols between institutions. Institutional ratification permits a term to proceed; the people performing it still decide whether to consent.

The shared store holds `Commitment`, `PersonalKnowledge`, `Requests`, and `PendingTalk`. A commitment contains its bounded revisions and their pending work references. A talk freezes the sender's dated reports. `Runtime` retains rules and an event cursor. People learn through local observations and elapsed reports, preserving each source and date.

Typed plans recheck admission, remembered foreign work, and selected source rows before staging. Realm and receiving owners append their canonical changes to one batch. Guards bind finalization to that batch and reject conflicting source or recipient writes. The assembly supplies current facts, spatial candidate IDs, counters, and ordered events. Realm filters candidates and chooses behavior; agreements, missions, institutions, and characters apply their own receiving effects.

Production providers are `game-ecs`, `game-cargo`, `game-society`, `game-characters`, `game-agreements`, and `game-missions`, plus shared types from `game-core`. Spatial candidates enter through a factual provider backed by the shared `LocalRoster`. The crate retains no separate location index.
