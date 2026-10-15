# game-provisioning

Food acquisition templates turn known sources into executable plans: travel to a meal, or travel to a resource patch, gather into a container, and eat the output. Opportunities use the person's recorded stock, locations, capacity, patches, and roads. They never inspect distant actual stock or the global route graph.

`options` reads numeric known snapshots and seeks only stock and patch namespaces. `next_step` chooses a known road endpoint, including interrupted travel. `step` performs the next physical action through the owning feature's request; `learn_failure` records only local evidence. Own carried containers follow the carrier, and interrupted travel resumes from the actual road position. Output appears only when gathering completes. Eating consumes the object's authored portion.

Each query decodes known container locations once and reuses routes to the same destination across stores and patches. Being at the destination requires no graph search. These temporary results use one immutable personal fact view and expire with the query.

Plans are serializable values owned by their caller, such as a character's mind. This crate creates no second action table or inventory ledger. [game-characters](../game-characters/README.md) chooses plans, applies motivation, and controls when they advance.

Run `cargo test -p game-provisioning`. Independent tests compose the physical features and knowledge without characters or the application.
