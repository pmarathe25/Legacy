# game-rest-activities

Physiology-owned persistent recovery, registered by `game-physiology` under its
existing owner. `RestActivities(Vec<RestActivity>)` validates a sorted batch of
idle, healthy, local actors and starts a single recovery epoch job. Soma changes
their disjoint resting rows through `SetRestingMany`, using the shared pool for
batches of at least 32. Action and schedule identities are allocated in person
order, outside worker callbacks.

Recovery retains the same action identity until fatigue reaches zero, hunger
reaches the supplied bound, health deteriorates, character permission changes,
or eight hours elapse. Epochs are at most an hour apart, shortened by the existing
hunger forecast under unchanged metabolic rates. Physiology advances the bodies;
this crate does not apply metabolism a second time. Epoch computation classifies
rows in parallel, settles resting flags, then publishes ordered action completion
and `Rested` events. Existing scalar resting hooks retain scalar flag dispatch.
Permission callbacks precede physical classification. After completion observers,
retained rows pass the barriers again before another epoch is scheduled; each
pass removes at least one row. Body-phase hooks that replace a completing action
or invalidate its settled resting flag reject and roll back the epoch.

Epoch jobs contain owner continuation rows and expected action identities. The
action's cancellation request clears resting immediately; obsolete epoch rows
cannot complete a replacement action. There is no additional saved resource or
new plugin identity. Old checkpoints remain readable, and pending new epochs
survive save/load. All requests and jobs participate in ordinary transaction
rollback. A rejected owner batch leaves fallback to its caller.

`CheckRestActivities` is a receiver-owned permission barrier supplied by the
character owner. The character adapter also interrupts on alerts, injury,
control transfer and disabling the model. Unknown `Rest` hooks cause that adapter
to retain ordinary scalar decisions. This API does not batch arbitrary owner
callbacks or promise uninterrupted rest after player control takes precedence.

Profile-only counters: `activity.rest.started`, `activity.rest.continued`, and
`interaction.owner.resting.{serial,parallel}.{calls,rows}`. These count attempts,
including work subsequently rolled back. The parallel state update currently
changes resting flags; this is not evidence of accelerated metabolic computation.
