# Physical transaction batches

Flattens owner transactions into aligned operation arrays for deterministic parallel planning.
Large batches of independent single-leg transfers resolve directly in parallel while retaining
the same expiry, capacity, exact-cohort receipts, and atomic draft semantics. Specialized
paths also handle distinct sources feeding shared destinations and one shared source feeding
distinct destinations, using the same seeded arbitration as the general planner. Other
mixed or overlapping custody uses the general shared-resource planner.
