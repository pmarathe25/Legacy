# Generated personal food choices

Seeds physical food stores and independent local food preferences from authored world-generation settings, without granting institutional authority.

`food_stores_per_region`, `food_store_users_percent`, and `food_depositors_percent` configure distinct starting stores and individual tendencies. Half of each region’s initial food is distributed among its stores, so total starting food is conserved. Founding reports use the remaining regional stock. The default generated profile enables this mode.

Generated stores can carry a person's initial claim and their own objection to future takings; these are seeded with `food_store_claim_percent` and `food_store_objection_percent`. `confrontation_percent` seeds a separate individual tendency, with its threshold, fear limit, interval, and physical conflict damage supplied by the generation profile. A claimant's reaction still depends on witnessing a taking and their own sensitivities; no store claim grants automatic enforcement.
