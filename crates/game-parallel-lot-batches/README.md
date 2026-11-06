# Physical transaction batches

Flattens owner transactions into aligned operation arrays for deterministic parallel planning.
Large batches of independent single-leg transfers resolve directly in parallel while retaining
the same expiry, capacity, exact-cohort receipts, and atomic draft semantics. Mixed or
overlapping custody uses the general shared-resource planner.
