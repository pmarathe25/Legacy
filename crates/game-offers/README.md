# game-offers

Typed proposals, bounded ordered collection and the saved choice scratch contract.
`game-choice` registers this state under its existing owner and reexports its API.
The wire keys, plugin version, collection order and arbitration rules are unchanged.
Optional frame data is absent from ordinary saves. This crate owns no autonomy
schedule or domain action. Tests remain in game-choice; frame tests cover saved
prepared proposals and owner boundaries.

Whole-actor preparation uses a private planning scratch value. It collects normal
bounded offers, records their pure-provider provenance, and defers typed sensing
updates through `defer`. Owners still execute those requests on resolution.
Blocked external writes invalidate preparation. Scratch metadata is never saved;
the resulting typed decision and effects are saved only in an enabled frame.
Nested provider preparation merges deferred effects and provenance as well as
offers. Each private provider starts with an empty effect buffer; earlier captured
effects must not be duplicated during fan-out.

Optional `FrameClock` state retains the demand clock's pending timestamp and
scheduled identity. It is absent from periodic-frame saves. Only choice-owned
clock handlers mutate it.
