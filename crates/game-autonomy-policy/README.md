# game-autonomy-policy

Autonomous decision parameters and an opt-in routine-continuation experiment.
`game-characters` reexports the existing `Mind`, `Rules`, `Configure`, and `load` API and
retains ownership of minds, scheduling, and decisions. This crate registers no
plugin or resource and owns no world resources. Its clock schedules owner-supplied
review requests; `game-characters` retains all writes to the `Minds` resource.

With `routine` and `option_memory` absent (the default), every existing decision
opportunity remains. An enabled `Policy` names eligible action kinds and bounds an attention
window. After selecting a timed `game-choice::Undertake`, a mind may reuse that
executable until its deadline. Each repetition receives an ordinary action ID,
duration, cancellation callback, owner validation, and completion outcome.

This changes behavior: a person may repeat a formerly attractive activity after
its motive has weakened, and alternative activities may wait until attention
expires. It does not promise equivalent histories. New evidence, feeding, injury,
arrival, failed completion, urgent hunger, and excessive fatigue end reuse. Food
plans, rest, and travel keep their existing path. Expiry permits no further
repetition; an already started action keeps its normal completion/interruption.

Routine state and its executable survive save/load. Failed starts roll back
through ordinary nested transactions before full deliberation resumes. The
optional rule and mind fields are omitted when disabled, retaining existing saves.

Use the research overlay `tools/history/profiles/routines-30` to evaluate 30-minute
minimum / two-hour maximum attention windows. The same expressed attention rule
selects a window within those bounds. The overlay deliberately selects only short
inquiry and signaling activities; broadening the list is a model decision.

The broader `reviews-5` overlay additionally sets `review_floor = 300000`.
Nonurgent wake-ups share one pending full review, due no later than five minutes
after the preceding full review. Urgent hunger, injury, and arrival bring that
review forward. Ordinary action completions still run at their scheduled times;
an idle actor may wait for the deferred review before choosing new work. New
information and feeding invalidate a cached routine immediately, but their
nonurgent decision response can wait. This can delay care and change observation
timestamps, choices, competition, and long-term survival. It is an explicitly
different model, disabled in ordinary content.

The review clock and pending callback identity survive checkpoints. Superseded
callbacks are cancelled atomically, including a deferred callback replaced by an
urgent review. Multiple wake-ups do not allocate duplicate review jobs. The
default rule and mind wire fields remain omitted when these policies are absent.

The clock predicate combines with the character owner's active-review marker to
absorb wake-ups raised inside the current full review. An external injury after
that review still brings a deferred callback forward, including at the same
timestamp. Tests distinguish these cases so coalescing does not add redundant
immediate reviews or suppress a new urgent interruption.

`Rules::option_memory` is a separate opt-in experiment. It retains the bounded,
unweighted non-survival offer list until a fixed expiry; ordinary action clocks,
food searches, hunger/fatigue scoring, trait weighting and failure penalties still
run on every review. Continuations are reconstructed for the current action,
rather than retaining an earlier action ID. Successfully selected new options are
consumed from the remembered shortlist; they can reappear on refresh. An exhausted
list refreshes immediately. This avoids repeating a completed one-off task just
because its old motive remains cached. Injury, arrival and abandonment clear
the memory. Urgent hunger bypasses it. A rejected reused executable or a reused
list selecting wait forces a fresh collection in that same review. Failed timed
undertakings also discard it. Feature owners retain execution and validation.

`option_limit` bounds the retained set (16 by default, valid range 1–256). Every
fresh collection still supplies its ordinary full list to the chooser. Only later
reuse is limited to the best currently trait-weighted offers; their original
motives and key order are retained. Ranking does not replace fresh survival
planning or the chooser's current failure costs. A smaller set reduces retained
payloads, serialization and repeated scoring, while potentially deferring more
alternatives until refresh. The default capacity is omitted from saved rules.

This changes the model: nonurgent new information does not immediately rebuild
the list; motives and unchosen opportunities can be stale until expiry. Provider
observations and other collection effects happen only on refresh. Availability
checks do not guarantee that an old motive is still attractive, and some owners
may permit an action that fresh deliberation would omit. Characters receive no
additional world knowledge. The saved memory contains ordinary executable owner
requests and survives rollback and checkpoints; an `Arc` keeps transactional
copies cheap; immutable offer storage is also shared across consumed-list versions.
Both rule and state fields are absent from default saves.

Use `tools/history/profiles/options-5` for a five-minute experiment in a new
history. `tools/history/decisions.py` makes explicitly identified policy forks of
existing checkpoints and compares runtime, memory, daily sampled state and event
counts. Such forks are new experiments, not equivalent study upgrades. This
policy does not enable routine repetition or deferred reviews.

The first reuse schedules one saved expiry wake. Fresh collection cancels it;
obsolete wakes check the current memory before doing anything. This bounds a
deferred response even when the current action lasts much longer than the memory.
An uninterrupted action with no reuse receives no extra periodic review. Expiry
does not duplicate an already pending same-time completion response. Option memory
cannot be combined with the routine/review-floor policy because their review
clocks impose different contracts.

`Policy::preserve_on_care_completion` is a narrower opt-in experiment, independent
of the global review floor. It defaults to false and is omitted when false.
`game-nurture` can leave an existing routine intact when the actor's own action
finishes, provided no food delivery is already in progress. Ordinary completions
still wake the character and run the usual capability, fatigue, hunger, deadline,
and failed-execution checks. External evidence still invalidates reuse. A
caregiver may consequently postpone noticing a need that was not signalled.
`tools/history/decisions.py --completion-minutes 5 15` evaluates fixed windows with
this policy and no global review floor, keeping ordinary execution as control.

`Rules::care_signal_changes_only` is independent of both routine reuse and the
review floor. It defaults to false and is omitted when false. The nurture listener
still records heard cues, but an autonomous actor without an existing delivery is
interrupted only by a new or changed food hypothesis. Non-food cues and repeated
estimates with fresh timestamps wait for another ordinary review opportunity.
This may reduce incidental voluntary switching or delay unsignalled care for the
current action's remaining duration. It does not guarantee equivalent histories.


`ReviewClock::frame` is the separate fixed-step experiment's once-per-step marker.
It shares the saved review clock without adding a second mind record. Ordinary
attention and routine policies do not call it. Character owners retain all mind
writes and wake scheduling; the experiment does not combine with routine or option
memory policies.

An optional `Mind::frame_retry` timestamp bounds one recovery review after a frame
idle fallback. It is absent in ordinary saves. `game-character-clock` owns its
scheduling, while `game-decision-food` supplies the optional cohort food query used
by consideration. Both policies remain explicitly opt-in.

`Rules::attention` supplies separate ordinary/evidence review floors through
`game-attention::Rates`. The saved `ReviewClock` is reexported from that leaf crate;
its wire fields are unchanged. The character clock owns runtime configuration and
all wake mutations. Attention cannot combine with legacy routine or option-memory
policies. Goal continuation is a separate choice setting, requiring an attention
floor; it delegates actual work to the owning mechanic.

`Rules::busy_attention` limits coalescing to actors with an active action, keeping
idle decisions and completions prompt. It is false/omitted in programmatic and
legacy-save defaults. New content enables it with 15-minute attention floors;
see the [quality and performance evaluation](../../tools/performance/BUSY_ATTENTION.md).
Urgent external evidence uses the character clock's `Alert` admission path.
