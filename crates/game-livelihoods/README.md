# game-livelihoods

**Purpose:** People pursue personal stock goals by composing real gathering and workshop actions. Goals come from [runtime content](../../content/livelihoods/daily.toml). Traits, individual variation, learned interests, available supplies, and remembered local resources determine which goal is feasible and attractive.

`Install` freezes resolved goals and source stamps before `Enable`. `Consider` is also subscribed to the characters owner's `IdleReview`; it adds no scheduler. The current player is excluded. Encountered, permitted local inventories supply ingredients and hold outputs. Planning prefers the person's carried supplies and checks capacity through every intermediate step. Stock targets cover all encountered accessible inventories, although a production chain runs in one inventory.

Installed rules are shared immutably across transaction snapshots. Planning removes already satisfied and unreachable goals before scoring and sorting; feasible goals retain their original preference and tie order.

Expressed intelligence bounds search depth (2–32 dependency levels), plan length (4–256 actions), and ingredient checks (32–8,192). Each recipe permits at most eight batches. Search rejects cycles and chooses cheaper feasible ingredient paths. A plan stores its remaining typed steps. Each step calls the ecology or workshop owner, which reserves capacity, consumes physical inputs, and owns cancellation. Failed attempts roll back and abandon the plan until the configured retry time. Contested access is weighed against personal motives. A completed stock goal slowly strengthens the person's interest in its concept.

Generated history enables this owner. Authored fixtures install the catalog for validation but leave autonomous production disabled. Travel, transfers between ingredient stores, markets, prices, assigned professions, and collective production orders are outside this owner. [Horticulture](../game-horticulture/README.md) owns plant-tending decisions separately.

**Verification:** `cargo test -p game-livelihoods` covers real costs and time, cancellation, stale knowledge, permission withdrawal, capacity, cycles, and save/load across worker counts.

Personal stock goals count carried possessions, the person's claims, and cooperatively shared local supplies. Another person's unrelated possessions do not automatically satisfy the goal. Physical crafting can still defy a claim; expanding autonomous proposals for appropriating another person's inputs belongs to the deferred autonomy plan. Ingredient reachability is indexed per work inventory, because a single-inventory plan cannot combine materials from every neighboring container.
