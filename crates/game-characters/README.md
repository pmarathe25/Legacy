# game-characters

Owns individual decisions, survival-plan commitment, failed-option cooldowns, and urgent hunger interruption of configured actions. Candidate scores use traits, learned values, needs, and known travel costs. The current ruler acts autonomously only while player control is unavailable.

Food plans come from [game-provisioning](../game-provisioning/README.md). Their progress and the last decision are saved in each mind; actual travel and gathering use the common action record. Death removes the mind. Failed food attempts refresh locally observable facts and allow another plan on the next review.

Scenarios select a `characters` profile from [content/characters](../../content/characters). The owning [schema](src/rules.rs) controls review intervals, retry intervals, need thresholds, utility weights for time and gathering, switching margins, and action kinds that urgent hunger can interrupt. Existing food acquisition stays committed; interrupted work keeps the cancellation consequences defined by its owner. Configuration freezes before minds are created.

Run `cargo test -p game-characters`. Tests cover plan continuation, stale-stock alternatives, blocked routes, partial travel, gathering, urgent hunger, learned values, and save/load with different workers, using only required feature plugins.
