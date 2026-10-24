# game-memory

The authoritative personal evidence store replaces permanent `Knowledge` retention
and the separate recollection sample and empirical reasoning queue. `game-knowledge::Knowledge` remains a
compatibility name for this resource, not a second copy. Perception caches retain
shared physical payloads for efficient observation; characters cannot query them
as memories. `game-recollection` records experienced episodes into this store and
selects creative subjects. `game-inquiry` retains only process counters; its
empirical evidence is retrieved from this same store before actual reflection. Skills, relationships, private beliefs and commitments
retain their separate owners; forgetting an episode does not erase those states.

New observations, understood reports, readings and inferences enter short-term
memory. Accessibility declines with elapsed time. At ordinary expressed potential
its retention window is 48 hours, ranging from six hours to about seven days with
`memory_durability`. Transient caregiver cues and remembered need estimates also consult this same
hereditary duration; fresh ambiguous cues cannot renew older food evidence.
A sweep removes expired evidence from the same topic-indexed
facts used by decisions, routes, food projections and reports, invalidating those
indexes. At most 512 short-term entries survive crowding, preferentially keeping
important or emotional evidence. Direct observation can refresh evidence; a cache
lookup or candidate-action query cannot rehearse it.

An observation or initialization is processed as one batch before crowding. Pruning
after each entry could delete a later entry in that same batch and panic on large
scenes. Batch processing also calculates personal context once, ranks crowding
once and removes forgotten association links in one pass. This corrects oversized
scene behavior; it is not an equivalent-history claim against the broken path.
The optional request-local cue index preserves the first four matching keys in
sorted order; `LEGACY_MEMORY_INDEX=0` retains a linear reference with the same batch
semantics. Linear lookup is the default; `LEGACY_MEMORY_INDEX=1` enables the index.
`game-memory-state` holds the saved data contract, with row-journal checkpoints
and local transaction validation. Memory retains registration and all gameplay
writes. Resource keys and save schema remain unchanged.

`game-knowledge::plugin` installs this owner automatically; callers do not also
register another memory plugin/store.

Completion of the existing physical `Rest` action is the current sleep boundary.
Cancellation emits no completed-rest event and performs no consolidation. Sleep
encodes memories with importance or absolute emotional intensity at least 500/1000.
Importance can follow personal values and experienced relationships, or be raised
by actual recollection/use. Emotional salience includes experienced condition and
participant birth/combat episodes. Salience increases retention, not truth.
Long-term capacity is 32–416 entries (128 at baseline), influenced by expressed
`memory_capacity`; crowding retains stronger/more significant memories. Long-term
strength loses one point per 30 days without reinforcement, so significant unused
memories fade on a scale of decades rather than short-term hours. Actual recall
strengthens accessibility without changing original observation/source metadata.
These are initial explicit model constants, not validated human cognitive rates.

`Recall` is an explicit cognitive use: reports/retellings and completed creative
activities call it. Long-term recall has a small seeded reconstruction probability
(0.1–1% per use, about 0.78% at baseline), reduced by `memory_reliability`. Payload
owners restrict reconstruction to a single quantitative descriptive detail, about
1% or one unit; actors, identities, sources, predicates and observation dates stay
intact. The remembered detail is replaced in place and confidence decreases slightly.
Physical records and canonical journal events never change. This is a bounded
reconstruction model, not arbitrary invented narratives or replacement beliefs.
Read-only queries, opportunity enumeration and cache hits never roll this chance.

All three named traits participate in the existing diploid founder/child inheritance
pipeline. Expressed ability also includes mental condition and aging. They influence
capacities/risk, not the contents of a parent's memories. The newborn inherits no
actual episode, skill, or report.

Facts retain sorted topic ranges. `retrieve` seeks one topic and at most 16 results,
following at most four personally established related links per entry. Links are
formed from shared personal cues (topic, actors, place, perceived relation), never
hidden world relationships. They are a bounded association aid, not proof of
causation; weighted independent link decay/context ranking remains future work.

The compressed evidence archive is preserved, with cognition metadata alongside
it. Earlier retained facts migrate in place as legacy long-term knowledge; old
sample bookkeeping is discarded because the episode content already exists in
that evidence. Old empirical queues move into the same archive; since their
original schema had no dates, migration marks their observation age as unknown
(epoch zero), rather than claiming a fresh observation. New saves retain lifetimes, links, recall counters and pending
sweeps. Tests cover selective/absent consolidation, genetic variation/inheritance,
expiration, shared scenes, correction, reconstruction, provenance, query purity,
transaction rollback and deterministic save/load across worker counts.

The current matched historical study keeps its original binaries and physically
isolated content; this behavior is a separate model and does not rewrite it.
