# Cultivation in personal choice

Cultivation currently runs at a physical boundary, after the optional shared
choice round. To compare it with other activities without advancing growth
prematurely, its owner needs a separate candidate and admission path.

* Produce bounded, person-local candidates from shared regional indexes. A
  candidate identifies the worker, field, observed phase and crop, and the
  exact planting movement and personally chosen seed batch when applicable.
  Candidate generation must not reserve stock or publish work.
  `game-economy-agriculture::opportunities::discover` now implements this
  bounded discovery, and the interval host exposes a read-only preview and
  includes these opportunities in the optional shared choice round.
* Let the person's ordinary choice profile compare that candidate with other
  activities and idleness. Preserve the exact selected intent and its actor and
  field reservations through partial intervals and save/load. A queued choice
  is not yet a successful harvest or planting.
  The scheduler reports a pending intention when it reserves a field and a
  separate accepted, denied, or stale outcome at settlement.
* At the next boundary, revalidate the same field, worker, crop, phase and seed.
  A stale or unavailable intent fails without substituting another field or
  cohort. Reconcile requests and legacy autonomous actors against reservations.
* Combine admitted selections and legacy work in the existing physical boundary
  transaction. Physical receipts settle shared stocks and capacities and
  determine the final outcome. Advance growth, irrigation, maturation, soil,
  request state and provenance exactly once per boundary.
* Exclude people governed by the shared choice round from automatic boundary
  cultivation unless their selected intent admits it. Controlled people and
  request responses retain their own independent choice paths.

Calling the existing `Context::prepare` during candidate generation is not
safe: it matches workers, evaluates requests, and advances field growth. Split
its owner-owned discovery and exact-action construction first; do not repeatedly
run it per candidate. Use deterministic parallel indexing and batched
arbitration rather than a global serial worker loop. Test competing choices,
refusal, stale intents, stock contention, rollback, partial-interval save/load,
worker determinism and concentrated scaling.
