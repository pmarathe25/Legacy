# Dependent care: corrected decisions and learning

**Status:** Cooperation, accompaniment, independent-child care, displaced-person search, and learned care corrections implemented, 2026-10-18. Final workspace verification is complete. All 487 Rust tests, formatting, and strict Clippy passed. A matched post-birth replay demonstrates improved care in one case; population effects remain unverified.

## Confirmed outcomes

Six early warm/rainforest infants died from deprivation. Five had no recorded feeding; one received three meals. A later coastal infant died after nine feedings. Rainforest child 11241910 survived day 1081 after 19 feedings, but the [day 2176 child extract](../../target/history/centuries-2026-10-18/forest-child11241910-day2176.json) records deprivation death at age 3.319 after 469 feeds. The [family extract](../../target/history/centuries-2026-10-18/forest-family-day2176.json) places full-health caregiver 8 in area 3 and the child in area 6; mother 58 was dead. These snapshots establish separation, not its cause.

## Confirmed decision defect

A diagnostic replay of warm child 4003334's first days produced exactly the same complete final world as an ordinary replay. Hooks inspected existing reviews and executions without injecting observations or collecting extra offers.

Both parents recognized food need by day 333. Their recorded estimates eventually reached 220. Neither attempted `Assist` or began a food-care intention. Last samples still contained remembered candidate food plans; their feasibility for every actual care request remains unproven.

Parent 61 selected `communities.cooperate` 609 times; parent 35 selected it 306 times. Source inspection exposes false success: affiliation offers cooperation for equal membership records, including two unassociated people and members of larger groups. Community organization returns success when no agreement is possible or pair organization cannot affect the group. Reaffirming unchanged permissions can also report success. Consequently, the chooser receives no failure feedback and repeatedly values an unproductive request above other motives.

## Correction and verification

The community owner now rejects unrealized or already fulfilled cooperation requests. Real changes to permissions, procedures, or attachments succeed. Revoked permissions and changed beliefs permit useful renegotiation. The request keeps its existing payload and response types.

Existing chooser failure memory supplies a finite, personally weighted cost while preserving voluntary refusal. Regression cases cover unsuccessful agreement, fulfilled practice, renegotiation, and larger groups across saving/loading.

The matched replay began at day 333, after child 4003334's birth, with identical complete inherited loci, family, and world. By day 365 the control child had died unfed. With corrected cooperation feedback, the child was alive with health 41, hunger 17, and 45 feedings from parent 61. This establishes a causal improvement for this case, not general survival or natural selection. Evidence is retained under `target/history/centuries-2026-10-18/cooperation-care-comparison.json`.

## Accompaniment and care candidacy

**Corrected source issue (historical):** `game-family-life` compares `foresight/20` against a mobile dependent's capped relationship sum. At expressed foresight 4000 or above, accompaniment is impossible even with maximal affection/trust. Hunger, route safety, remembered resources and self-provisioning are absent. Foresight should inform anticipated consequences, rather than prescribe refusal.

Related `family:independence` adds foresight as desire; `Independent` removes responsibility. Nurture's biological-child candidates require a remaining dependent record, so independence can suppress new voluntary care offers despite attachment/remembered need. Existing intentions or shared-care bonds can still help.

Previously, care could return to a remembered area but could not search after a child moved. It now explores personally remembered roads and retains acquisition/delivery intentions. It never queries the child's true remote location.

Next capture the last shared departure/first separation, child foresight/ties, carrying result, independence status, caregiver food/location memories, routes and selected offers. Test that foresight can favor safe accompaniment, independence preserves voluntary help, and searches reveal no true remote location. Preserve refusal; survival remains unguaranteed. The day 2176 extracts alone cannot attribute this child's death to these paths.

## Implementation record

Foresight now estimates dependence rather than directly rewarding independence
or vetoing accompaniment. Affection, trust, fear, grievance, learned preferences,
risk tolerance, remembered resources, and personal needs can support either choice.
Biological attachment remains a candidate for voluntary help after independence.
Known-road searches can find a beneficiary who left the remembered meeting place.

Actual feeding yields `child-care` practice. Personal care observations can support
inference; understood lessons and physical books can transmit useful knowledge to
later caregivers after the author dies. Acquired experience is not inherited.
Practice improves cue interpretation and anticipation without prescribing kindness.

Campaign, observation, and literature regressions cover these paths, refusal,
physical delivery, and cross-worker save/load. These corrections do not retroactively
prove the cause of every observed infant death or guarantee population persistence.

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
