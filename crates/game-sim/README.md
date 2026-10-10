# game-sim

`game-sim` coordinates registered pure boundary and input-stop callbacks. `ClockWorld` maps its clock operations to the composition’s existing authoritative time and publication counter. Module and phase keys give phases a stable dependency order, and module IDs give callbacks a stable evaluation order.

Preparation reads an immutable world and produces a transaction for the clipped interval. A bounded executor runs keyed jobs and returns results in logical-key order. The engine applies the prepared transaction once to the sole mutable world and publishes one closing clock; the transaction may return explicit caller input.
