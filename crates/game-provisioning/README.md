# game-provisioning

Food acquisition templates turn known sources into executable plans: travel to a meal, or travel to a resource patch, gather into a container, and eat the output. Opportunities use the person's recorded stock, locations, capacity, patches, and roads. They never inspect distant actual stock or the global route graph.

`options` uses [game-food-memory](../game-food-memory/README.md) to index remembered edible stores and carried containers, and reads the known patch namespace. The derived index follows immutable payload changes instead of rescanning unchanged historical inventories. `next_step` chooses a known road endpoint, including interrupted travel. `step` performs the next physical action through the owning feature's request; `learn_failure` records only local evidence. Own carried containers follow the carrier, and interrupted travel resumes from the actual road position. Output appears only when gathering completes. Eating consumes the object's authored portion.

Each query reuses routes to the same destination across stores and patches. Being at the destination requires no graph search. Route results use one immutable personal fact view and expire with the query; cached store projections are rebuilt after changed evidence, rollback, or different food rules.

Plans are serializable values owned by their caller, such as a character's mind. This crate creates no second action table or inventory ledger. [game-characters](../game-characters/README.md) chooses plans, applies motivation, and controls when they advance.

Run `cargo test -p game-provisioning`. Independent tests compose the physical features and knowledge without characters or the application.

When no remembered meal or usable food patch remains, options can include searching a frontier reached by remembered roads. This reveals no distant food: the traveler must arrive and observe before planning a meal there. A forage action can ask the optional stockpiles owner to free carried capacity through actual local storage. Features that omit that plugin retain ordinary capacity refusal.

Food opportunities carry actor-specific dietary motives derived from remembered stock and experienced meals. A deficiency motivates foods believed to supply the missing nutrient; it does not make every edible source equally useful. Unfamiliar sources may warrant a trial when a diet is unsatisfying, without revealing their chemistry. Reserve gathering compares remembered travel cost and personal dietary value instead of always breaking equal-distance ties by source identity.

Repeated foods across remembered stores share one actor-specific appraisal within an immutable planning query. The temporary cache is discarded before execution; neither observations nor dietary values are shared between characters or retained across changed state.

Meal opportunities identify a particular food, and execution consumes that food. Foraging likewise consumes its selected produce after gathering. Other carried foods cannot silently replace an intended experiment and leave the selected food perpetually unfamiliar. Older inventory-only meal plans still decode and use the former generic meal selector. Meal candidate identifiers combine inventory identity with a catalogue-local food slot; foraging identifiers retain their separate parity.
