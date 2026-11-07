# Individual attempts to take possessions

People can independently attempt to take a particular quantity from another person's actual custody. A target's personal notice and resistance preferences determine whether the attempt is opposed; lack of resistance is not approval. A person may choose whether to escalate opposition. This owner proposes physical movements and optional conflict intents; the host must admit goods through the physical runtime, start any conflict through the combat owner, and publish the resulting receipts atomically.

Policies and responses are data, not roles or institutions. An unsuccessful attempt creates no goods. Outcomes impose no moral judgment or automatic feelings. Optional `reactions` let each participant interpret each outcome with their own signed changes, including person-specific overrides; an unaware target has no direct experience. Controlled targets use their own saved standing preference.

An individual taking policy may opt into `recover_incapacitated`, allowing its
person to attempt to recover possessions from their specified holder when that
holder has zero health. The holder cannot notice or resist while incapacitated;
other witnesses may still observe. Real location, access, custody, capacity,
and contention still apply. Death transfers nothing by itself and this choice
does not establish an heir or authority. The existing player `recover` action
continues to handle controlled recovery separately.

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

An optional `prior_actor_adjustment` lets a person use an earlier report they
already received about the same actor when interpreting a later event. The
individual chooses the signed adjustment and can set `prior_belief` to consider
only reports they believed, doubted, or left unassessed. The latest qualifying
earlier observation is retained with the assessment as its personal basis;
unknown reports and other people's private records cannot affect it. This is
bounded by the existing personal report store and is neither a legal rule nor
a finding of fact. A manual revision clears the recorded basis. In player mode,
`taking-precedent off` disables it; `taking-precedent DELTA any|believed|doubted|unassessed`
sets it for future assessments.

A taking policy can optionally set `assessment_at_least` or
`assessment_at_most` for the actor's own assessment of an event attributed to
its target. Without an applicable assessment, this condition does not motivate
the voluntary action. The person may turn their willingness off; separately
configured fear or obligation can still motivate them. The action is an
individual physical attempt, not an automatic sentence, verdict, or right to
the goods, and the target may notice and resist as usual.

The ignored `precedent_benchmark` exercises synthetic already-received reports
and simultaneous answers. Run `cargo test --profile simulation -p game-economy-taking precedent_benchmark -- --ignored --nocapture`, setting
`LEGACY_PEOPLE` and `LEGACY_WORKERS` as desired. It times only the inquiry
update after setup and before result destruction, not a complete world boundary.

The ignored `assessment_selection_benchmark` measures selection for independent
personal taking policies. Set `LEGACY_ASSESSMENT_CONDITION=1` to make them
consult synthetic personal assessments, and use `LEGACY_PEOPLE` for the number
of actors and `LEGACY_WORKERS` for concurrency. Like the precedent benchmark,
it excludes setup, physical execution, and result destruction.
