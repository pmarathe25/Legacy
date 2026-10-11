# game-world

`game-world` provides feature-agnostic host infrastructure over one shared component store. `GenericHost<State, Event>` manages logical time, stable identity allocation, typed publications, and snapshots; composition supplies schemas, state, events, validation, and systems. `ResourceBudget` preflights transaction allocations and publications.

The crate depends only on the core types, ECS adapter, and simulation engine. Feature policy and playable game bindings belong in domain crates and `game-assembly`. The playable assembly is still migrating to the generic host; its remaining behavior and entity maps must move to their owners before the migration is complete.
