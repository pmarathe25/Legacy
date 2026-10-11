# game-logistics

`game-logistics` plans one next step for a resource requirement and destination. The same planner handles food, water, timber, tools, and other goods: it can acquire or carry supplies, travel, transfer between inventories, and reach a handoff. Acquisition offers may declare prerequisites, which the planner composes recursively, including requirements that must be carried, consolidated into one usable lot, or available at the provider.

The caller defines the goal and supplies the actor’s known observations, actual usable cargo, carrying room, and travel constraints. Remembered stock and provider offers guide attempts; they do not guarantee current access or yield, and an actor may fail or refuse an action. Arrival only proposes a handoff: delivery progress changes only when an actual receipt is recorded.

Delivery readiness and acquisition demand expansion stay in this crate. Providers supply prospects for each requested good; Characters supplies willingness judgments. The planner expands prerequisites once per good and chooses the next step from those facts.
