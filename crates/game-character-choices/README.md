# game-character-choices

Character-owned review preparation, selection, failure learning, and action
application. The characters plugin registers these handlers under its existing
owner. Transaction-local cursors are not saved and may not cross commits.

With `local_interactions`, collected choices are stable intentions: selection and
deferred resumption do not regenerate the chosen provider. Urgent/body/action/
control guards remain in the interaction driver, and execution owners retain
their live validation and hooks. An actual rejected deferred request records its
ordinary failure and tries remaining prepared alternatives within the existing
eight-step limit shared by attempts and fresh deliberation. Every next alternative checks the actor's live body, control,
urgency generation and action identity. New body/urgent changes force fresh
deliberation; a newly started activity is preserved; an ended continuation is
discarded. Exhausted invalidation preserves a bounded future review wake. Explicit
`FrameRecheck` hooks retain provider rechecking.

Urgent/body invalidation retains the remaining retry budget when rebuilding a
cursor. A committed `Reconsider` outcome cannot reset that budget by repeatedly
raising new evidence. Exhaustion completes the current review and preserves a
strictly future wake, including while an action is active. That wake rechecks
player control and actor eligibility through the ordinary review path.
