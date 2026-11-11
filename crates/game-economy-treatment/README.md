# Personal treatment

Personal choices attempt to consume real, unexpired medicine. A matching infection known to the simulation at the start of the boundary is cleared only when consumption succeeds; an ineffective course does not clear another strain or a newly acquired infection. Named remedy content is resolved through the goods catalog at world creation.

Optional `health_at_most` sets the observed-health threshold for an autonomous preference. When omitted, the person considers treatment below the world's maximum health. A controlled person can attempt their chosen medicine regardless of infection. The actual remedy may be ineffective.

The interval host's opt-in personal comparison uses `opportunities_now`: an explicit personal threshold is honored, but an omitted threshold does not prevent a person from choosing medicine while feeling well. The actual dose and affected disease episodes are still committed by their physical and disease owners.

The ignored `independent_multistrain_treatment_benchmark` measures episode selection and clearing for `LEGACY_PEOPLE` patients, each with two strains. `LEGACY_WORKERS` sets concurrency. It excludes physical medicine consumption and the rest of the simulation boundary.
