# Physical transaction batches

Flattens owner transactions into aligned operation arrays for deterministic parallel planning.
Large batches of independent single-leg transfers resolve directly in parallel while retaining
the same expiry, capacity, exact-cohort receipts, and atomic draft semantics. Specialized
paths also handle distinct sources feeding shared destinations and one shared source feeding
distinct destinations, using the same seeded arbitration as the general planner. Other
mixed or overlapping custody uses the general shared-resource planner.
Shared-source and shared-destination single-leg transfers can also enter directly from
flat action batches, without constructing a full transaction for each participant.
Independent shared sources with distinct destinations resolve as bounded parallel groups.
Overlapping endpoints, mixed goods within one source, and oversized groups use the
general planner; expiry and accepted cohort receipts remain identical.
Independent single-source consumptions also retain exact dated receipts in a
parallel path; shared sources and identity-sensitive custody use the general owner.
