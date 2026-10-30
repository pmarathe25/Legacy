# Parallel goods cargo

Carries nonfood goods with individual people, including during travel. Local pickup and deposit batches validate against a shared boundary snapshot and commit atomically; each participant can make at most one transfer per batch. Capacity is an aggregate item count. A deceased person's possessions remain with them; a living local person can recover them into local stock; ownership rights are not yet modeled. Food uses the separate food-cargo owner.

An exchange request represents offers already accepted by both local participants. The owner checks both inventories and carrying capacities before moving either side. It does not yet model negotiation, valuation, property claims, or autonomous consent.
