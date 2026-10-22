# Observation, explanation, and writing

Features supply perceived episodes through `game-inquiry::Witness`. An episode names a subject, measured outcome, observable context, and optional shared experiment ID. The interface does not name food, crops, or any other particular domain. Adapters currently cover travel, crafting, cultivation, meals, thermal harm, and inspected food stocks. Active food samples and experiments belong to `game-food-inquiry`; passive lived-event adapters belong to `game-observations`. New domains need their own perceptual adapters; characters do not receive a global stream of hidden state.

Intelligence controls retained working observations, comparison work, contextual complexity, and deduction depth. Reflection is a cancellable action that competes with other interests. Improved expressed intelligence can reopen old evidence. Contrasts hold other measured conditions constant. Agreement across varied settings can support an uncertain abstraction; deductions compose compatible conditional links. Uncontrolled associations remain tentative, contradictions reduce confidence, and chaining cannot manufacture certainty. The numerical model currently represents bounded conditional relationships, not arbitrary symbolic reasoning or a complete scientific theory language.

Personally remembered measurements and claims support `game-knowledge::forecast`. Predictions preserve conditions and local confidence, and may be wrong. Craft planning uses remembered practice and work-time relationships when estimating effort. Storage also consults learned conditional effects. Physical execution still uses the actual feature rules. Other decision owners can consume the same forecasting interface without depending on inquiry or a particular observation adapter.

Discovery and understanding have different demands. A teacher can explain in detail, simplify a practical explanation, or demonstrate using concrete local referents. Recipient intelligence, teacher capacity and patience, openness, directed regard, and available communication channels affect success. Simplification reduces cognitive demand without deleting conditions or increasing certainty. Ordinary testimony passes through the same comprehension boundary. Receiving a claim does not automatically make it true.

Writing must be developed through paid practice. Scripts have personal proficiency; notes consume real paper and ink. Glyphs encode the assertion, provenance, conditions, and uncertainty rather than just its title. Reading needs physical access, interpretation, a known script, understood language, and cognitive capacity. Literary works can supply a physical model for script instruction. A remembered page cannot reveal its contents after an unseen transfer.

Run `cargo test -p game-reasoning -p game-inquiry -p game-literacy -p game-observations -p game-works`. These cover intelligence differences, practical and visual teaching, uncertain deduction, conditional forecasts, developing capacity, source-specific observations, invented scripts, physical access, and deterministic saving/loading.

## Questions and practiced care

`game-information` separates an information need from its answer. Any feature can
register interest in a fact, category, or supply. The ordinary chooser can ask a
local person or consult a seen record; the respondent separately chooses whether
to share. Replies contain personal evidence, including its age and uncertainty.
They do not query world truth. Practical explanations can reduce cognitive demand,
while writing requires physical exposure, learned vocabulary, and a known script.

Feeding produces caregiver practice and personal care observations. Acquired
`child-care` skill improves cue interpretation and anticipation. Understood care
lessons and readable works can pass learning between generations; children do not
inherit their parents' experience. Skill changes competence, not compassion.


## Topic retrieval, fading, and associations

Memory retrieval should use a person's own topic indexes and local associations.
A supply question needs remembered stores and resource patches; unrelated family
stories and mechanical experiments should not be scanned to answer it. Cue indexes
can include people, places, objects, subjects, and perceived events. They index
what this person experienced or understood from a report, including mistakes;
they never expose another person's private memories or add hidden world links.

Existing responsibilities remain distinct: `game-facts` owns indexed fact storage
and shared scene representation, `game-knowledge` owns personal evidence and
observation, and `game-food-memory` projects remembered supply facts. General
memory retention, reinforcement, salience, and associative retrieval are a proposed
separate `game-memory` feature owner. This lifecycle and association layer is not
yet implemented by the topic-range optimization in `game-information`.

A memory's accessibility should fade with elapsed time according to its kind and
the person's expressed capacities, condition, attention, salience, and meaningful
reinforcement. Temporary stock reports, significant episodes, practiced knowledge,
and relationships do not share one universal expiry clock. Strong emotional or
repeated experiences can remain accessible longer, without making their content
more accurate. Stable skills, commitments, and relationships retain their existing
owners; losing recall of one episode does not automatically erase those states.

Actual use, reflection, retelling, or reading can reinforce accessibility. Read-only
queries, cache hits, proposal enumeration, and unchosen alternatives must not do so.
Otherwise an implementation optimization would change what a person remembers,
and repeatedly considering an action would prevent forgetting it. Reinforcement
must not advance the original observation timestamp, replace its source, or turn
rehearsal of a stale or false report into new evidence. Accuracy and confidence in
an assertion remain separate from vividness and ease of recall.

Direct associations should be personal, typed, weighted links formed through
experienced or understood connections. Retrieval begins with relevant cues and
follows a bounded set of nearby links, ranked by accessibility and current context.
It does not traverse every memory or a global social graph. Related memories may
help interpretation or contradict one another; an association alone proves no
causal relationship. Link strength may fade or be reinforced independently of the
linked episodes. Topic indexes and adjacency indexes must remain consistent through
replacement, forgetting, rollback, and save/load.

Subjective forgetting does not delete the historical journal, physical records,
or cultural artifacts. An old text can survive its author's fading recollection,
but another person must encounter, read, and understand it to learn from it. A
player-facing chronicle should distinguish witnessed events, attributed reports,
and remembered interpretations, preserving their provenance and uncertainty.

A future memory model needs tests for context-limited retrieval, gradual fading,
meaningful reinforcement, stale and false evidence, bounded association traversal,
and equivalence across saving and worker counts. Introduce behavioral retention
rules as a separately identified model in historical comparisons. Storage indexing
and caching can be optimized independently when complete replay outputs match.
