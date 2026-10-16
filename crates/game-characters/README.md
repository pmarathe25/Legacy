# game-characters

Owns individual decisions, survival-plan commitment, failed-option cooldowns, and urgent hunger interruption of configured actions. Candidate scores use traits, learned values, needs, and known travel costs. The current ruler acts autonomously only while player control is unavailable.

Food plans come from [game-provisioning](../game-provisioning/README.md). Their progress and the last decision are saved in each mind; actual travel and gathering use the common action record. Death removes the mind. Failed food attempts refresh locally observable facts and allow another plan on the next review.

Scenarios select a `characters` profile from [content/characters](../../content/characters). The owning [schema](src/rules.rs) controls review intervals, retry intervals, need thresholds, utility weights for time and gathering, switching margins, and action kinds that urgent hunger can interrupt. Existing food acquisition stays committed; interrupted work keeps the cancellation consequences defined by its owner. Configuration freezes before minds are created.

Run `cargo test -p game-characters`. Tests cover plan continuation, stale-stock alternatives, blocked routes, partial travel, gathering, urgent hunger, learned values, and save/load with different workers, using only required feature plugins.

Reviews process at most 128 due people per committed request, in stable person order. Idle minds retain their authored observation and decision cadence. Active minds use a compact ordered set; sleeping minds use an indexed deadline queue. Busy minds sleep until the action deadline or a hunger interruption threshold supplied by physiology. Completion and cancellation send an internal signal that wakes the mind on the authored grid. Cursor state prevents reviewing an earlier actor twice at the same clock time. Save/load restores these deadlines.

Candidate inputs are 56-byte numeric records. Trait weights use five numeric channels and visit only recorded deviations from authored defaults. Decision explanations are built after selection. New mechanics that invalidate a physiological forecast must also invalidate the corresponding mind's deadline; ordinary progress does not require waking every actor.

Minds use indexed dense rows. Checkpoints journal changed people and snapshot the small rules/schedule metadata. Physical row slots never determine review order.
