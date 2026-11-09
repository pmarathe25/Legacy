# Individual claims

People may make competing assertions over regional stock. Each person chooses whether to respect others' assertions; the physical store decides only whether the requested goods exist. Food pickup, work, cultivation and dependent care consult the acting person's preference. Recorded use and personal reactions derive from accepted physical movements and a bounded sample of up to four present claimants. A person's output preference can lead to a new assertion after a harvest or delivery actually succeeds.

An observer may independently interpret witnessed stock use with a signed personal affinity change, including a relationship-specific override; this does not create an obligation.

When local walking is enabled, witnesses must be at the regional stock endpoint. Other claimants remain claimants but do not learn the use from a different store. The optional `LEGACY_CLAIMS_LOCAL=1` mode of `concentrated_claims_benchmark` places half of the claimants elsewhere. For 100,000 people, one and six workers took 75.6 and 33.2 ms with matching state fingerprints and only local witnesses; this isolated workload scales sublinearly.

People can also assert conflicting claims about physical work sites. Site use and demolition consult each actor's own choice; assertions do not evict existing work. Up to four physically present claimants can observe each actual use or destruction. An assertion may remain after a site is destroyed and can be withdrawn; site identities are not reused. Observers may interpret events through optional signed personal affinity preferences; no reaction is imposed.

Field claims are separate personal assertions over cultivated fields, with no authoritative owner or automatic transfer of seed or harvest. A person may choose to respect or disregard competing field assertions, and may choose to assert a field after their own successful planting or harvest. A claimant's request signals permission only for the requested action; other claimants' assertions still enter the worker's decision. Up to four locally present claimants observe an actual accepted use, and an attempted action with no physical receipt produces no observation. Existing assertions are observed before new post-work assertions are added. Field assertions are treated as locally available information at the regional stock endpoint; individual memories and reports of these assertions are not yet modeled.

Each person's separate `assert_built_sites` preference can lead them to claim a site only after their own construction physically succeeds. Building does not automatically imply a claim; people may change this preference.

When `inherit_births` and family birth enrollment are enabled, newborns may receive a parent's preferences for making or disregarding assertions. They inherit no stock or site assertions, relationships, or obligations, and may change their own choices.
