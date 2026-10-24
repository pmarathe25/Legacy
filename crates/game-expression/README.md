# game-expression

Pure developmental expression from a `Life` value, logical time and inherited
potential. The module owns `Life`, `Upbringing`, growth curves, condition recovery,
aging, expressed ability and disposition formulas. It has no simulation host,
resource, request, observation or scheduling dependency. `game-vitality` owns the
world's life records and reexports the established public types and methods.

`ability` and `disposition` reuse exact numeric inputs in a bounded thread-local
cache: person, complete life value, exact clock, attribute and raw potential.
Dispositions share the existing 32-slot expression frame rather than introducing
another world cache. Changed traits, within-transaction condition changes,
rollback, another world and colliding person slots all retain exact results.
Unknown attributes return their inherited potential. No world snapshots or
mutable resource aliases are retained.

`Projection` lazily shares maturation curves, upbringing integration, aging and
condition recovery across attributes for one complete `Life` value and exact
clock. It owns only numeric inputs and derived values. It preserves each integer
division and saturation in the reference `Life` methods; it never rounds time or
reassociates products. The vitality owner's bounded projection frames reuse it.
The direct `Life` methods remain independent differential-test references.

`ability_slot` and `disposition_slot` identify attributes with developmental
formulas. They let the vitality owner validate and reuse complete projections;
other authored traits continue through the ordinary data-driven lookup.

Tests compare cached queries with uncached formulas across development, aging,
condition, signed raw inputs, slot collisions, restored values and other threads.
