# game-utility

Pure candidate scoring, stable tie breaking, and switching margins. Callers supply motives, consequence estimates, and whether to apply the urgency filter. This crate owns no character needs, state, priorities, or action execution. Characters reexports its public types for existing callers.

Validation: `cargo test -p game-characters --test utility`.
