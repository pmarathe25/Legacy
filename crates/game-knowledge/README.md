# knowledge owner

An individual may enable `remember_people` to retain where and when they personally met others. This is a bounded personal memory of an actual local encounter, not another person's current location or private thoughts. It is independent of whether either person shares a food report. Each mind retains at most `max_records` people and at most `max_records` regional food records. Disabling new memories does not erase old ones.

Each mind may set `reporting = { willing = false, pressure_threshold = 5 }` to withhold reports unless its own fear plus felt obligation toward the listener reaches five. A mind without a policy keeps observations private. The listener independently decides whether to accept a report.

Each configured mind keeps at most `max_records` regional food observations. The elapsed host records a living person's stock observation when they have access during a food search, encounter, or survey. At an actual encounter, either person can share one remembered remote observation, motivated by their own willingness or felt pressure. Reports carry the original witness and observation time and add the immediate sender. Direct observation takes precedence for the current region, and a remote report can replace an older unprotected record when memory is full.

Knowledge prepares against immutable state and publishes only after the physical and family phases succeed. A mind may privately configure `misreport = { food_delta = 50, target = 1 }` to change the quantity it tells one listener; omit `target` to address everyone. Negative offsets understate stock, and quantities saturate at zero and the maximum representable amount. This changes a statement, not the speaker's memory or the stock. Observation time and witness are claims carried from the remembered record, not independently verified provenance. The recipient sees the claim and immediate speaker, never an objective label identifying a lie. Honest reporting is the default. If `[opinions.rules]` sets `minimum_report_trust`, recipients ignore reports from a sender they distrust; direct observations remain available. The threshold is directed and uses committed trust, including trust changed by previous agreements.

An individual's opinion sensitivity may override the shared report threshold with `report_reception = "always"`, `"never"`, or `{ trust_at_least = 0 }`. This is a personal judgment about whether to accept a statement, independent of whether it is true. The information model is deliberately limited to food stock. It does not model perception errors or private-property access. The economy host can enroll newborn minds and can use observations for travel decisions. Player observation exposes only the ruler's records.

An optional `[[knowledge.retention]]` entry gives a `person` a positive
`duration_millis` for food-memory recall. The person can recall the record
until that duration elapses after its last reinforcement, initially its
observation time. Direct observation, accepted reports, retelling, reading a
physical page, and using a report for an actual food-search departure can
reinforce recall while leaving the evidence date intact. A passive view leaves
recall unchanged. Legacy `ticks` and evidence with boundary dates can still
be loaded.

The ignored `personal_memory_retention_benchmark` measures knowledge preparation for `LEGACY_PEOPLE` people; `personal_memory_retelling_benchmark` includes independent encounters and repeated reports. `LEGACY_RETENTION_STRIDE` selects how sparsely personal retention is configured (1 means everyone, 100 means one in a hundred), and `LEGACY_WORKERS` selects concurrency. For retelling, `LEGACY_ENCOUNTER_STRIDE` controls the fraction of pairs that meet. These benchmarks exclude the rest of the economy boundary.

When a person directly observes a region and the amount differs from a report they remember, they can interpret that experience with a signed `affinity_report_discrepancy` in their own opinion sensitivity. A `report_discrepancy` in a target-specific relationship case overrides it. The change is applied once, toward the immediate speaker, and defaults to zero. A difference is not evidence of a lie: stock may have changed since an honest or mistaken report.

The listener may separately set `trust_report_discrepancy`, with a target-specific `report_discrepancy_trust` override. This signed personal change can alter later acceptance of that person's reports. It also defaults to no change and does not depend on the speaker's actual intent.
