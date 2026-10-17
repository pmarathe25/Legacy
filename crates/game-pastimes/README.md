# game-pastimes

**Purpose:** Characters develop personal interests through timed activities with physical supplies. The [catalog](../../content/pastimes/daily.toml) includes music, writing, drawing, weaving, decorative work, tea, reflection, and clothing care.

`Pursue { person, activity, inventory }` requires local permitted supplies and the person's common action slot. `inputs` are consumed at the beginning; `requires` remain reusable possessions. Completion checks locality, continuing permission, and possession of required goods. A successful experience records a private participation count, practices the authored skill, and slowly strengthens the participant's value for the activity's concept. This can influence future leisure and livelihood goals. Cancellation spends the already used inputs and awards no experience or skill.

`Install` resolves content and freezes definitions before `Enable`. Generated history enables autonomous leisure. `IdleReview` ranks activities by the participant's traits, interests, and stable individual variation, then uses their carried possessions. Recent satisfaction reduces an activity's personal utility; it never forbids repetition. Interests, traits, experience, and desire for company affect the decision. Explicit player actions use the same request; automatic decisions exclude the current player. Normal player observations include only that player's completed pastime counts.

Each activity is an individual experience. Music does not automatically influence everyone nearby, reflection prescribes no religion, and reading does not fabricate historical facts. Reusable instruments need to remain available through completion; the owner does not reserve them exclusively.

**Verification:** `cargo test -p game-pastimes` covers physical inputs, permissions, one-action exclusion, cancellation, missing instruments, personal learning, individual satisfaction and immediate voluntary repetition, player control, and save/load.
