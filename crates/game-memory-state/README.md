# game-memory-state

Saved personal facts and trace metadata, their validation/serialization contract,
and a request-local ordered cue index. `game-memory` retains plugin registration
and all gameplay writes, retention and scheduling. This crate registers no plugin.
Public types are reexported through `game-memory`, preserving caller and wire APIs.

Facts use packed row undo journals. Trace books use persistent ordered records;
transaction checkpoints retain only their root and each trace is shared until
edited. Nested rollback restores both. Normal validation checks changed facts
and traces. A process-local fact-key token skips unchanged key-set comparisons;
membership changes still check the complete fact/trace key join. Trace removals
also check unchanged traces for dangling associations. Full load/save validation
still checks the complete resource. Sharing and tokens do not enter save files.

The cue index preserves the first four matching keys in logical sorted order and
is rebuilt per observation batch; it is never serialized. Physical packed-row
order does not choose game outcomes. Crowding helpers retain the existing ranking
and tie breaks.
