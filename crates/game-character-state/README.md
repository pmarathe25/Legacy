# game-character-state

Saved autonomous mind collection, rules reference and review agenda. `game-characters`
registers and exclusively mutates this resource and reexports `Minds`; the resource
key and wire representation remain `characters.minds`. This crate implements only
state validation and transactional storage hooks, with no plugin or decision loop.
`game-autonomy-policy` supplies individual mind and policy types; `game-choice` supplies
the agenda. Weak diagnostic and query caches remain outside this resource.


Frame review marking and clearing a matching wake are small state operations;
callers must obtain the owner-checked mutable resource. The review predicate is
supplied by `game-autonomy-policy`; this module adds no dispatch or scheduling loop.
