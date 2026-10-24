# game-autonomy-frames

With demand-driven frames, the adapter forecasts the earliest living character's
pending wake, agenda deadline, or regular review. Action completions and new cues
can bring that boundary forward through the character clock. Empty periods sleep;
physical simulation clocks continue to run normally.

Participation and observation adapter for the opt-in decision-frame experiment.
Registers a callback under the existing assembly owner, with no resource or plugin
manifest entry. At each boundary, characters with a due wake or periodic review
refresh local and carried-store evidence together. Independent proposal providers
then prepare that cohort concurrently. Empty cohorts do no preparation.

Character wake notifications coalesce at the next boundary, including injury and
food cues. Physical events still execute at their original times. One mind reviews
at most once per step; newly received information can affect its next step.
The provider mode keeps final selection and food planning live. Optional
`batch_decisions` delegates whole-actor preparation to the character-owned intent
adapter. Actors entering a review after cohort collection use live fallback.
Resolution also validates cohort action identities: a character newly involved
by another resolution defers its obsolete turn until the next boundary.
`game-characters` owns the wake queue, clock and final choice. Effectful providers
and physical action resolution remain ordered. The pilot requires ordinary
attention rules; combining it with routine/option-memory experiments is rejected.
