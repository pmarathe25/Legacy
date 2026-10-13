# game-lifecycle

`game-lifecycle` owns biological aging, reproduction, dependent actions, preferences, and caregiving records. `Runtime` retains lifecycle rules; canonical `Life` and `Pregnancy` components live in the shared store. Intrinsic component validation and contextual time, parent, body, and deadline validation remain separate.

`conception_candidates` selects bilateral willing pairs in stable order from relationships and current choice facts. Prepared transitions decide conception completion, pregnancy loss, birth, stage changes, and lifespan death. Birth effects describe an actual child and position; receiving owners construct its body, inherited traits, inventory, and mind. `prepare_lifespan_change` receives accepted lifespan penalties and survival facts, guarding the life row, rules, and clock.

`prepare_dependent_start` owns Signal, Play, Comfort, and Care eligibility and deadlines. Physiology supplies fatigue availability; spatial and resource owners supply contact and usable-dose facts. `prepare_dependent_reaction` receives completed signals and actual care, clears satisfied preferences, and records caregivers. Childhood learning and relationships belong to Characters. `prepare_prenatal_history` admits authored elapsed history using actual age and configured stages, rejecting a reached lifespan before writes.

Prepared operations stage into a shared batch and retain generic `ReadGuard` witnesses. Callers validate finalizations after all owners have staged, then publish once and finish their effects. Guards protect exact rows, relevant pregnancy membership, and supplied factual joins. Failed admission cannot publish only a child, pregnancy, care record, or resource debit.

The optional `heredity` integration interprets configured allele combinations as reproductive roles. `prepare_reproduction` receives an unpublished inherited genome through Genetics' opaque allele interface and can stage its role beside the genetic birth.

Independent tests cover admission, deadlines, stale inputs, save continuation, and actual Cargo–Body–Life–Characters–Lineage care composition. Strict stock, body, life, or guardian schema rejection preserves all participating rows; repeated delivery receipts make no second progress. No owner test requires the playable simulation or assembly crate.
