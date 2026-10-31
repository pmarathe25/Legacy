# Ruler-facing economic interface

This interface exposes the controlled person's own state, possessions, current job, nearby people, local stock, personal food reports and current tactical encounter. Remote stocks, other minds and the administrative simulation snapshot are not exposed. Past-life reports require active invocation. While the player has lost control or a successor has not been born, actions requiring a ruler are unavailable. Guardian tracking is an explicit separate view. Observations borrow committed state and filter in parallel instead of copying the entire world.

The interface is an initial playable slice of the economic host, not a complete game client.

The ruler can inspect local physical work sites and their occupied capacity, and the recipe catalog exposes authored tool and site requirements.

The ruler can assert a personal claim over local stock or a local work site, inspect their own assertions, and withdraw them even after leaving or after a site is destroyed. The view includes the ruler's locally witnessed uses of their claimed sites. Other people's private claims are not exposed by this view; assertions do not themselves prevent anyone from taking goods or using a site.
