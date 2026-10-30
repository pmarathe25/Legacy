# Dependent care

Available local parents may spend an action caring for young children according to their own preferences and feelings. Sharded arbitration assigns at most one child per parent per tick; children try either parent and rotating deterministic priorities distribute contention over time. Unattended local children suffer data-defined neglect damage. Parents and children can also share care while traveling on the same journey. The owner exposes events and actor reservations; the fused host applies effects only after the full tick validates.

Run `cargo run --release -p game-parallel-dependent-care --example benchmark -- 1 100000 spread 4`, varying worker count and using `hot` to concentrate demand on two parents. Import is outside the timed interval.

The runnable [`caregiving.toml`](../../content/parallel/caregiving.toml) fixture supplies two willing parents and a dependent child.
Players can change their caregiver preference with `care-policy PERSON enable|disable` and inspect their own care events with `caregiving PERSON`; the session interface restricts both commands to the current ruler in player mode.

With `feed_from_cargo = true` and personal food access enabled, a selected caregiver may feed a hungry dependent using one meal from the caregiver's own food. The caregiver and child must be together and available, and a child who can eat their own carried meal is not fed a second one. Food is debited and hunger changes only on tick commit, including while traveling together. This does not create food or automatically assign a parent an entitlement to regional stores.

With `fetch_local_food = true`, a selected caregiver who is not feeding from their own food may instead attempt to take a meal from the physically reachable local regional source and give it to their dependent in the same care action. This competes with other local acquisition attempts; scarcity or inadequate carrying capacity can leave the child hungry. The current model treats the region as one reachable source.

Optional `inherit_births` seeds each actual newborn's personal caregiving preference from one recorded parent; a missing parent preference yields no willingness. The child cannot act as an adult caregiver before adulthood and can later change their own preference. This is an initial personal disposition, not a binding obligation.

Optional `pressure_threshold` also lets a parent decide to care from their own experienced fear or felt obligation toward the child even when their private willingness is false. Those feelings can change, and neither the rule nor a request compels them to keep caring.
