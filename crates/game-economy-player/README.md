# Ruler-facing economic interface

This interface exposes the controlled person's own state, possessions, current job, nearby people, local stock, personal food reports and current tactical encounter. Remote stocks, other minds and the administrative simulation snapshot are not exposed. Past-life reports require active invocation. While the player has lost control or a successor has not been born, actions requiring a ruler are unavailable. Guardian tracking is an explicit separate view. Observations borrow committed state and filter in parallel instead of copying the entire world.

The interface is an initial playable slice of the economic host, not a complete game client.

The separate `interval::Player` adapter is an initial path for the interval
host. It checks that the chosen person is controlled before each action, and
provides their own state, possessions and observable wound severity, along with
co-located people. It does not reveal the wound's future recovery deadline.
An incapacitated controlled person may still advance time but cannot observe.
It can advance time, depart on a route, eat carried food, start and stop chosen
production work, and use a carried wound remedy. Chosen work uses configured
physical capability rather than a universal age or hunger refusal; its base rate
currently comes from the host schedule rather than a personal effort choice. Where
fatigue is enabled, the person can see their own fatigue and choose when to rest
or wake; resting pauses ongoing work. A controlled helper can leave their own
shared-work assignment without dismissing the other helpers.
The interval view also includes only that person's own spoken vocabulary,
written-form associations and retained observations, along with marked pages
they actually carry. The player can choose a form, inscribe it with physical
supplies, interpret a carried page, or adopt their own remembered observation.
Time advancement reports elapsed time and failures without returning administrative
events. A limited `--interval-player PERSON` CLI exposes this adapter. It does
not yet provide campaign succession or the full player command set, and the
ordinary `--player` CLI still uses the whole-boundary host.

`advance_boundaries` can yield between committed whole boundaries and reports partial progress and pending warnings. Its callback exposes boundary time only; administrative physical events remain outside the player view. Partial-boundary time is not yet supported.

The explicit guardian search view reads embodied guardians' current health, location and journeys from the same person and travel records as the rest of the world. The ruler may meet a local guardian and request release; the guardian's decision uses their own accumulated mentorship.

The view shows the controlled person's health and their own configured medicine and quantity. It does not reveal an unobserved infection, its strain, or a hidden match between that strain and a remedy. A treatment attempt reports whether medicine was consumed; later observable health may provide evidence of its effects.

The ruler can inspect local physical work sites and their occupied capacity, and the recipe catalog exposes authored tool and site requirements.

The ruler can assert a personal claim over local stock or a local work site, inspect their own assertions, and withdraw them even after leaving or after a site is destroyed. The view includes the ruler's locally witnessed uses of their claimed sites. Other people's private claims are not exposed by this view; assertions do not themselves prevent anyone from taking goods or using a site.

The ruler can independently choose whether to assert a claim automatically after personally constructing a site.

The controlled person sees work requests addressed to them and can accept or decline each one. Acceptance attempts the requested physical work, subject to actual materials and site requirements; a failed start does not become a completed job. The requester can only learn outcomes they witness.

The controlled person can configure their own signed trust reactions to witnessed fulfilled and unfulfilled promises, or use the scenario defaults.

Local wildlife appears in the person's view by identity, species, and observable health. Remote animals, genomes, diets, and pregnancies are not exposed. `feed-animal ANIMAL GOOD QUANTITY` attempts to offer the person's carried food to a local animal and reports whether the food was used, without revealing whether it was nutritious.
When configured, a local animal view also includes the controlled person's own affinity toward it; the animal's feelings remain private.
`hunt-animal ANIMAL` attempts to hunt a local animal; success requires real space for the species' fresh yield and advances the shared world.
