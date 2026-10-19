# Autonomy follow-up: attention and experienced outcomes

**Status:** Implementation completed, 2026-10-18; workspace verification is complete. The defect descriptions below preserve the audit findings before correction. Long-history observations remain separate from these regression results. This supplements [the historical audit](character-autonomy.md).

The coastal diagnostic recorded 141,420 reviews over two days with 82 living mortals ([log](../../target/history/centuries-2026-10-18/profile-dense.log)). Calls include continuation and failed attempts; they are not distinct acts of thought. Attention-based scheduling, soft failure costs, valid continuations, and broader invention search subsequently addressed several original findings.

## Defects corrected

**Delayed outcomes disappear.** `game-choice/src/undertaking.rs::Finish` ends the action, then discards `tx.attempt(Run(...))`. Starting `Undertake` already returned success to the chooser; `game-characters/src/choices.rs` records failure only around immediate execution. A delayed rejected loan, unsuccessful hunt, or `Outcome::Reconsider` therefore produces no common character-specific failure expectation. Domain owners can still retain their own learning; this is not proof that every failed action teaches nothing.

Emit a typed completion outcome from choice, carrying person and action identity. Preserve rolled-back physical effects and committed owner observations. Characters should consume it through the same failure-memory helper used for immediate attempts, without confusing an old completion with a replacement engagement. Keep perceived failure costs personality-dependent; do not impose a retry prohibition. Refactor existing character bookkeeping to offset the subscriber's line cost. This requires no simulation-kernel changes or budget increases.

**Live encounters skip experience learning.** `game-communities/src/organizing.rs`, `communities.consider`, returns early when Minds is running, before its conditional `shared-decisions` learning. Generated histories follow this branch; controlled fixtures can follow the other. The current encounter-based origin of that preference is consequently missing during autonomous play. This does not prove voting is absolutely unreachable through every external or discourse path.

Run the existing individual-belief/trust learning before the running-state branch; continue deferring organization to character proposals. Do not move automatic permission withdrawal/organization with the learning loop or give every encounter a pro-voting reward. Reordering the existing block need not enlarge the communities owner.

## Remaining design work

**Current conversation recall:** The initiator's scan can exclude its current
partner before continuation valuation. The implementation now includes that known
partner outside the alternative-search limits and retains actual continuations
within bounded recall. All 490 Rust tests, 24 Python history tests, formatting,
and strict Clippy passed. Four release comparisons were nearly neutral
(about 1.7% slower through 1.3% faster); the performance target remains unmet. This fixes
visibility without prescribing whom a character follows or talks to.

Distinguish perceived evidence from full reconsideration. A notification need not reopen unrelated intentions, although genuine danger and successive feeding must remain actionable. Retain expected outcomes across work phases instead of valuing continuation only when a start proposal survives. Scoped failure causes remain preferable to broad action discounts.

Invention now considers remembered work needs and broader designs. Remembered useful designs can now be replicated, unmet loads motivate retesting, and individuals can decline familiar mechanical aids. Private physical limits remain with physics.

## Acceptance and causal boundary

- A timed failure updates only its actor's matching engagement, once, across save/load; success and cancellation stay distinct.
- Eligible encounters produce the same conditional experience learning with Minds running, while institution changes still require selected actions.
- Mixed beliefs permit cooperation, leadership, fear, or shared decisions without prescribing voting.

The separate phantom-cooperation correction already passed 487 Rust tests and a matched post-birth replay: identical child genes, 45 feeds and survival to day 365 versus unfed death previously. These findings neither revise that prepared causal result nor establish additional causes of those deaths.

## Implementation record

- Choice emits typed completion outcomes. Character-specific delayed failures use
  the same finite learning as immediate failures, scoped to the matching action
  and applied once. Cancellation remains distinct from failure.
- Minds retain raw expected motives when a provider no longer advertises a
  running action. Retention does not repeatedly compound a continuation bonus.
- Live encounters learn shared-decision preferences conditionally before the
  autonomous-state branch; organization still requires a chosen action.
- Mechanical techniques, design copying, conditional retesting, and voluntary
  aid selection use personal measurements and ordinary resource constraints.
- Contrasting unexplained observations can motivate curiosity and general
  information requests through separate asker/respondent choices. Asking
  teaches nothing; answers retain evidence age, language, and cognitive limits.

Regression evidence is in `game-characters/tests/failure_learning.rs`,
`game-communities/tests/behavior.rs`, `game-mechanics/tests/behavior.rs`,
and `game-information/tests/questions.rs`. No 200-year or twenty-generation
outcome is inferred from these tests.

## Final verification

Implementation verified on 2026-10-18: 525 Rust behavior tests passed across
`cargo test --workspace --no-fail-fast`; the documentation build passed after
rebuilding stale Rust artifacts with `cargo test --workspace --doc`. The 24 Python
history tests, 5 Python performance-tool tests, formatting, and strict workspace
Clippy also passed. Source budgets and feature ownership remain enforced.

Physics simulations covered 20,000 networks, 5,000 loaded trials, and 256 finite
apparatus trials, including 52 interruptions and 64 cross-worker replays. They
exposed and verified corrections for overflowing load-capacity probes and ongoing
work retaining a superseded agility rate. Compact evidence is under
`target/history/plans-physics-2026-10-18/`.

The ongoing population studies and broad performance targets remain separate,
incomplete empirical work. These implementation and physical-invariant checks
do not establish 200-year outcomes or twenty-generation selection effects.
