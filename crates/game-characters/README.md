# game-characters

Owns individual decisions, survival-plan commitment, failed-option cooldowns, and urgent hunger interruption of configured actions. Candidate scores use traits, learned values, needs, and known travel costs. The current ruler acts autonomously only while player control is unavailable.

Food plans come from [game-provisioning](../game-provisioning/README.md). Their progress and the last decision are saved in each mind; actual travel and gathering use the common action record. Death removes the mind. Failed food attempts refresh locally observable facts and allow another plan on the next review.

When food and rest needs are satisfied, enrolled people can ask [game-social-life](../game-social-life/README.md) for local company. That owner handles willingness, companion selection, contact history, and conversation actions. The mind records the social intention for inspection. NPC searches exclude the current player from implicit consent. Social actions can be interrupted by urgent hunger through the same authored policy as work.

Scenarios select a `characters` profile from [content/characters](../../content/characters). The owning [schema](src/rules.rs) controls review intervals, retry intervals, need thresholds, utility weights for time and gathering, switching margins, and action kinds that urgent hunger can interrupt. Existing food acquisition stays committed; interrupted work keeps the cancellation consequences defined by its owner. Configuration freezes before minds are created.

Run `cargo test -p game-characters`. Tests cover plan continuation, stale-stock alternatives, blocked routes, partial travel, gathering, urgent hunger, learned values, and save/load with different workers, using only required feature plugins.

Reviews process at most 128 due people per committed request, in stable person order. Idle minds retain their authored observation and decision cadence. Active minds use a compact ordered set; sleeping minds use an indexed deadline queue. Busy minds sleep until the action deadline or a hunger interruption threshold supplied by physiology. Completion and cancellation send an internal signal. An existing food intention or unmet food/rest need prompts reconsideration immediately; otherwise the mind wakes on the authored grid. This prevents short unrelated journeys from repeatedly displacing food decisions. Cursor state prevents periodic reviews from reviewing an earlier actor twice at the same clock time. Save/load restores these deadlines.

Candidate inputs are 56-byte numeric records. Trait weights use five numeric channels and visit only recorded deviations from authored defaults. Decision explanations are built after selection. New mechanics that invalidate a physiological forecast must also invalidate the corresponding mind's deadline; ordinary progress does not require waking every actor.

Minds use indexed dense rows. Checkpoints journal changed people and snapshot the small rules/schedule metadata. Physical row slots never determine review order.

An internal `IdleReview` notification lets upper owners consider care, organizing, horticulture, production, and leisure before ordinary company. Each checks the common action record; the first accepted action occupies it. Owner-specific cooldowns leave opportunities for other activities. Any action they start uses the existing action clock and is recorded in the mind’s last intention. This adds no second autonomy scheduler or dependency from characters back to communities.

Chosen food plans resume when travel or gathering finishes, independently of the ordinary review clock. This continues an existing intention rather than starting additional periodic clocks. Urgent needs can interrupt unrelated rest or conversation even while a food intention exists. Reviews inspect the character’s own carried supplies; other inventories remain subject to personal evidence. Independence measures reliance on another person’s provisions, so eating one’s own stored food does not encourage gathering still more.

Feeding also prompts reconsideration when needs remain. One portion does not impose a review-length wait on a still-hungry character. Each further choice uses the updated needs, experienced food effects, and available supplies.
