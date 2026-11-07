# Physical lot planning

Stages deterministic, all-or-nothing custody changes and expiry against an immutable snapshot.

General transaction receipts share bounded lot arenas instead of allocating one arena per movement. A single receipt with more than 4,096 dated cohorts gets its own arena, so retaining a different receipt does not retain that oversized result. Receipts remain ordered by movement ID and retain exact acceptance and cohort data; compact consumption keeps its accepted empty receipt.
