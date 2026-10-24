# game-decision-clock

Choice-owned frame timing. The existing choice plugin registers these requests;
there is no additional plugin or resource. Frame preparation stays in
`game-decision-frames`, and character deadlines stay with autonomy.

Ordinary frames keep their periodic clock. Opt-in `Demand(true)` cancels polling
and schedules a frame only for a forecast deadline or a new wake. New cues can
bring the next frame forward; later cues share it. Sleeping retains the frame
phase, so a cue waits at most one configured interval. The autonomy adapter
forecasts mind deadlines, and character wakes request frames through a typed
choice-owned request. Physical action and world-event clocks continue unchanged.

The pending frame identity is saved with optional frame state. Failed transactions
restore both that identity and the queue. Disabling demand restores periodic
frames; disabling frames cancels their work. Repeated configuration is a no-op.

Skipping empty jobs changes global identity allocation and can change later
random outcomes. This remains a separate model experiment, even where decision
boundaries coincide. Disabled demand omits its state from existing saves.
