# game-information

Owns information interests and remembered questions. `Want` registers interest
in a fact, a category of facts, or sources of a particular good. `Withdraw`
abandons an interest. Features can use these requests without defining a new
conversation system for each mechanic. Contrasting personal observations with no
remembered explanation can also create an interest through `game-inquiry`.
Openness and contemplation influence that curiosity; questions remain competing
choices.

The common chooser offers asking nearby characters, answering remembered
questions, declining, and consulting previously seen texts with understood
titles. It never inspects a prospective respondent's knowledge to decide whom
to ask. `Ask` requires contact, conversational capability, and an understood
channel when language simulation is active. Asking alone teaches nothing.

`Answer` selects from the respondent's own knowledge. Generosity, sociability,
relationships, learned privacy, effort, and competing needs influence whether
they answer. Replies use testimony or a practical explanation; language and
cognitive limits still apply. Stock reports can include remembered locations
and routes. Evidence retains its original age and uncertainty, even when wrong.

A registry is an actual written record, created and shown through `game-literacy`.
There is no global stock index. Seeing a page does not reveal its private meaning;
reading still requires exposure, vocabulary, script knowledge, and comprehension.

Interests, pending questions, and attempted contacts/readings survive save/load.
Memory is bounded (eight interests, sixteen incoming questions, sixty-four
contacts/readings per interest). A new encounter can motivate asking again;
learning a script or improving comprehension can renew a reading attempt;
there is no mandatory response or universal conversation cooldown. Death clears
incoming questions and the deceased's interests.

`cargo test -p game-information` covers voluntary answers/refusal, stale reports,
unknown respondents, route sharing, language/contact limits, simplified explanations,
physical registries, and replay across worker counts.

Refusal is a completed action, not a failed attempt. A failed explanation, reading,
or stock search can retain its evidence and return `Outcome::Reconsider` to the
chooser. This keeps failure learning without rolling back a deliberate refusal
or newly discovered absence.

Question inboxes and interests use per-person shared records. Updating one history
copies that person's row, preserving retained revisions without copying neighbors'
question vectors. Proposal collection borrows immutable row snapshots; it changes
neither willingness to answer nor which facts are available.

Answer search visits only the personal-memory key ranges relevant to a topic.
Supply questions examine remembered `patch:` and `store:` facts; selection still
uses evidence age, confidence, and the same key tie-break. Unrelated memories
remain available to other questions, and no cross-review answer cache is retained.

Transaction validation checks only rows changed since the valid checkpoint,
using the persistent records' structural diff. Each invariant is local to one
row, so removing a row cannot invalidate another one. Loading and saving still
validate every row. Direct edits, tree splits and nested rollback are covered by
differential tests against full validation.
