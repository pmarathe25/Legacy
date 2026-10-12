# game-health

`game-health` adds bodily needs, illness, care, and shelter rules to the game. It tracks thirst and cold exposure, accumulates enteric and respiratory infection, and models symptom timing, recovery, and treatment. Water lots retain contamination through collection, transfer, consumption, and purification; shelters track insulation, capacity, occupancy, and acknowledged access.

The crate owns action validation, work transitions, treatment consent, shelter admission, and clinical calculations. It plans water procurement from a person's remembered stocks, springs, and routes. The world adapter supplies physical and contact facts, then applies inventory, movement, activity, and event effects.

All health state lives in the host's shared component store. Clinical state, jobs, occupancy, knowledge, and caregiver offers attach to person entities; water quality attaches to inventory entities and shelters use shelter entities. `Runtime` retains only rules and the last review time. Prepared actions guard their affected rows and exact related job, offer, and occupant membership, then apply one atomic batch before returning effects. Intrinsic schema checks validate each row's structure; catalog validation checks clock, person, stock, location, and activity references.

`prepare_interval` advances supplied clinical and job rows through the same numerical rules and retains sparse guarded changes. The host validates the prepared interval before publishing other frame writes, then commits through the atomic batch. Closing job policy can preserve intentional cancellation without accepting replacement work or recreating a removed job.

Local exposure and observation systems consume temporary spatial, activity, body, and geometry facts. Health owns shelter inheritance, contact counting, symptom visibility, and remembered observations. Geometry providers supply visibility and reach facts without exposing their runtimes. Observations commit as a guarded batch and preserve water knowledge.

`prepare_local_index` builds one immutable index for a preparation. Physical systems can reuse its scalar exposure lookups, and perception can prepare observations from the same cohorts. Its borrowed store prevents using the index after health state changes.

Individual affordances can resolve shelter through a small carrier-chain lookup or inspect symptoms and pair visibility directly, without indexing the population.

Collected water mixes its contamination load with existing water and updates the holder's own observations. Clean water added to an unknown or unsafe stock does not erase that history. Prepared water and memory writes can join a cargo deposit in one guarded batch.

Prepared placements select the actual effective shelter through a small factual carrier/activity/body port. They guard every consulted actor, Occupancy and Shelter row, coalesce departures and compatible placements, and stage projected shelter observations in the same shared batch as motion. A receiving owner supplies projected physical facts; Health owns occupancy inheritance and visibility policy.
