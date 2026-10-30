# Directed trust

Each observer's trust in another person is separate. Authored rules define the initial and bounded trust values and how directly experienced fulfilled or defaulted promises update them. The creditor learns from the debtor's outcome; no omniscient global reputation is created.

An agreement recipient may set `minimum_trust` in their automatic acceptance policy. An observed default lowers that person's trust in the debtor; a later fulfilled promise can rebuild it. The ruler's ordinary view exposes only their own opinions. `content/parallel/distrust.toml` starts with a proposal that the recipient's configured threshold causes them not to automatically accept. Manual decisions remain possible.

`encounter_gain` optionally increases each participant's directed trust after an encounter, except for a participant who responded under pressure. Agreement outcomes and encounters at the same boundary combine before trust is clamped. A report threshold reads the prior committed trust, so a newly earned change affects later encounters. See `content/parallel/social_trust.toml`.

`theft_loss` controls the default trust change when a present claimant witnesses unauthorized use or pickup. An absent claimant does not automatically learn of it. This is an individual's reaction to a claim being disregarded, not a universal criminal status.

`fear_per_damage` optionally increases the victim's directed fear of a person who actually injures them. This is tracked independently of trust and does not require a market. The fused host applies those updates only after the complete tick succeeds. Automatic responses to requests can use the victim's previously committed fear; compliance does not imply approval or increase trust.

Optional `fear_decay_per_tick` reduces existing fear in parallel at each committed tick, before new injuries are recorded. It defaults to zero. Choices read the prior committed fear, so a person can later stop complying as fear subsides without altering trust or obligations.
`content/parallel/fear_recovery.toml` demonstrates this: request work from person 2, advance three ticks, and request it again; the same person can begin the first job and refuse the later request.

`gratitude_gain` optionally increases a person's directed sense of obligation when they actually receive a one-sided gift. It does not create a contract or guarantee future repayment; it can influence later requests. Failed or declined transfers do not create this experience.

`obligation_relief` optionally reduces the giver's own felt obligation toward the recipient after a real one-sided gift. It cannot reduce that feeling below zero, does not change trust or fear, and does not settle a separate agreement. Simultaneous responses read the prior committed feelings.

Optional `[[opinions.sensitivities]]` rows override `fear_per_damage`, `gratitude_gain`, `obligation_relief`, or `theft_loss` for an individual, including zero. People can therefore react differently to the same experience instead of receiving a universal response.

An individual's optional `confrontation` preference can cause them to initiate an ordinary conflict with a local person they personally distrust, provided their own fear is within their tolerance. This reserves the actual participants and starts the encounter at the next boundary; the target does not need to consent and can subsequently defend or retreat. A person can disable or re-enable their tendency with `confrontation PERSON enable|disable`. Candidates are selected from the person's own assessments with a bounded local search; competing confrontations use deterministic disjoint matching. If a participant dies before publication, the fight does not start. Currently this works with non-tactical conflict; tactical-map generation for automatic confrontations remains unfinished. See `content/parallel/confrontation.toml`.
