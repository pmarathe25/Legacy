# Individual attempts to take possessions

People can independently attempt to take a particular quantity from another person's actual custody. A target's personal notice and resistance preferences determine whether the attempt is opposed; lack of resistance is not approval. A person may choose whether to escalate opposition. This owner proposes physical movements and optional conflict intents; the host must admit goods through the physical runtime, start any conflict through the combat owner, and publish the resulting receipts atomically.

Policies and responses are data, not roles or institutions. An unsuccessful attempt creates no goods. Outcomes impose no moral judgment or automatic feelings. Optional `reactions` let each participant interpret each outcome with their own signed changes, including person-specific overrides; an unaware target has no direct experience. Controlled targets use their own saved standing preference.

Optional `[[taking.witnesses]]` entries describe a person's attention to a particular actor, an optional target filter, and their separate `toward_actor` and `toward_target` interpretations for each outcome. A witness must notice and be physically co-located and accessible; their reaction may be positive, negative, or absent regardless of the participants' views. These observations do not imply legal authority or automatically obligate anyone.

Each witness retains their most recent directly noticed taking by that actor. The observation belongs to that person, persists across saves, and is recorded even when relationship tracking is disabled. Changing attention cannot fabricate or rewrite it.

The actor or target can also use a personal attention entry to retain their own experience. A target only remembers an attempt they actually noticed; participants use their own direct reaction rules rather than receiving a second witness reaction.

`[[taking.report_policies]]` lets a person share a previously observed event during an actual encounter, voluntarily or under their own fear/obligation threshold. `claimed_outcome` and optional `details` (target, named good, and quantity) may differ from their memory. Any omitted detail repeats the remembered or heard account. `[[taking.receptions]]` gives each listener their own stance and separate signed responses to claims they believe, doubt, or leave unassessed; hearing a report does not establish its truth. Each recipient retains at most 64 latest reports, and repeated identical claims do not repeatedly change feelings.

With `source = "heard"`, a policy forwards a previously received account instead of claiming direct observation; optional `heard_from` selects a particular prior speaker. The delivered statement names that immediate source, and the new listener forms their own belief. An account received in the current boundary cannot be forwarded until a later one.

Retained reports use immutable recipient-keyed pages, sharing unchanged pages across drafts. Saves still contain the same flat, canonically ordered report list. The owner benchmarks include dense relays, a single update in a large history, concentrated recipient batches, and retirement of densely changed history. Retirement releases unshared branches in parallel while retaining any pages shared with other generations.

`taking_selection_benchmark` exercises independent attempts; set `LEGACY_SHARED` to make all attempts target one person and `LEGACY_WORKERS` or `LEGACY_PEOPLE` to vary execution. It measures proposal selection, not the complete world boundary.

The owner hands simple taking actions to the physical host as flat movements. Independent batches can be staged without per-action movement allocations; mixed or contested physical work uses the general transaction arbiter.

A listener can set `matching_toward_speaker` and `different_toward_speaker` to signed reactions when a received account refers to the same event in that listener's own direct memory. An absent memory or a different observation time gives no comparison reaction. Each person chooses these responses independently; a difference does not establish intent or force belief.
Inquiry recipients can optionally interpret a later witness statement using their
own `assessment_preferences`. Separate optional signed scores apply to matching
or different statements which the recipient believes, doubts, or leaves
unassessed. An absent score means no judgment; zero is an explicit neutral
judgment. This private assessment is recorded with the received answer and
does not establish truth, impose a duty, change feelings, or cause enforcement.
It reflects the preference and belief at the time the statement is received.
The player can set future interpretations with `taking-assessment` followed by
six scores (matching believed, doubted, unassessed, then different believed,
doubted, unassessed), using `none` to withhold a judgment, or `clear` to remove
the preference.
`taking-assessment-now SCORE|none` lets the current recipient revise or
withdraw their existing assessment without changing the testimony itself.
