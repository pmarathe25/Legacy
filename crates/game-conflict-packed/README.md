# Compact conflict planning

`game-conflict-packed` compiles conflict membership and orders into immutable incoming-strike topology. It evaluates independent attack availability in parallel and produces compact, generation-owned round receipts. Per-attacker events can be expanded from those receipts in a deterministic order. Earlier receipts remain valid when later topology generations are prepared.

The planner can apply an optional round-local eligibility predicate for range or visibility. Permanent participant removals and changed defenses use filtered topology. Saturating total damage does not discard the individual attributed receipts.

Run `cargo test -p game-conflict-packed` for owner tests. The `conflict_compare` example compares compact planning and damage reduction with the ordinary conflict path; compilation and receipt expansion are reported separately. This owner-only measurement does not establish full simulation scaling. See the [current campaign benchmark](../game-economy-cli/README.md) for whole-boundary measurements.
