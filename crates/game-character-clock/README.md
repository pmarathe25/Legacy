# game-character-clock

`Rules::busy_attention` makes an attention floor apply only while an action is
active. Idle choices and action completions stay prompt. The receiver-owned
`Alert` event admits urgent external evidence before that floor; own injury,
urgent hunger and arrivals retain their immediate paths. `ConfigureBusyAttention`
changes this setting without replacing unrelated character rules.

Changing attention rates preserves existing periodic/action wakes. Only already
pending coalesced callbacks need rescheduling. Attention delays also preserve the
next periodic review grid point instead of accidentally adding an entire review
period after crossing its boundary.

Optional `FrameTuning::commit_batch` queues bounded, stable-order resolution groups
after `PlanCohort` preparation. Wakes retain their existing character deadlines and
share the group's scheduled request. Each actor still uses the ordinary `Resume`
handler, receiver validation and frame guards. Physical actions resolve sequentially
within a group. A failed actor rolls back that entire group.

Zero retains individual resume jobs. Positive sizes are limited to 128 actors,
matching the existing bounded periodic-review pattern. Pending groups survive
save/load. Other same-time jobs can run between groups; grouping changes scheduler
identities and the point at which those jobs can see action effects, so it is an
explicitly different model.

Switching from grouped to individual commits, or changing/disabling frames,
restores pending individual wake jobs before the choice configuration changes.
These configuration hooks share the transaction, so invalid configuration also
rolls back the restored jobs.

Character-owned wake scheduling and review admission. `game-characters` registers
these handlers under its existing owner and supplies the review callback. This
crate owns no resource or plugin manifest entry; `Minds` stays character-owned.
It preserves ordinary attention, routine and event wake behavior.

The opt-in frame recovery policy schedules one next-boundary follow-up after an
idle fallback. The follow-up does not repeat itself if it still finds no activity;
later external events can initiate another bounded recovery. This reduces the
cost of a snapshot missing a temporary opportunity without polling every idle
character indefinitely. Future wakes survive cleanup of the current review.

`ConfigureAttention(Some(AttentionRates { ordinary, evidence }))` enables separate
nonurgent completion and evidence floors. Injury, arrival and urgent hunger still
advance the review immediately. Changing rates atomically replaces pending jobs;
disabling attention restores pending ordinary reviews. This experiment requires
event-driven decisions and cannot combine with frame or option-memory policies.

With `game-choice::ConfigureExecution(true)`, a successful owner step can emit
`GoalReady` carrying a typed continuation. A same-time scheduled callback checks
the chosen goal, completed action, full-review timestamp, current action and urgent
body state before calling that owner. It runs after the producer has deposited its
outputs. Successful continuation leaves the broad review deadline unchanged;
failure or exhaustion requests an immediate full review. Cancellation, abandonment,
a newer review or disabling execution invalidates the queued callback. This
permits multiple steps toward the selected goal, rather than replaying its original
selection request and reinstating obsolete steps. It changes choices and remains
opt-in. Pending callbacks and owner tokens survive rollback and save/load.
