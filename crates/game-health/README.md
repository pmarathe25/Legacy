# game-health

`game-health` adds bodily needs, illness, care, and shelter rules to the game. It tracks thirst and cold exposure, accumulates enteric and respiratory infection, and models symptom timing, recovery, and treatment. Water lots retain contamination through collection, transfer, consumption, and purification; shelters track insulation, capacity, occupancy, and acknowledged access.

The crate owns action validation, work transitions, treatment consent, shelter admission, and clinical calculations. It plans water procurement from a person's remembered stocks, springs, and routes. The world adapter supplies physical and contact facts, then applies inventory, movement, activity, and event effects.

`Clinical` and `Job` are plain components on person entities in the host's shared store. Typed access and schema validation belong to this crate; `Runtime` retains rules and the remaining water, shelter, consent, and knowledge state. Prepared actions guard only their affected component rows and related job membership, then apply one atomic component batch before publishing runtime changes and effects. Catalog validation checks clock, person, stock, location, and activity references.

`prepare_interval` advances supplied clinical and job rows through the same numerical rules and retains sparse guarded changes. The host validates the prepared interval before publishing other frame writes, then commits through the atomic batch. Closing job policy can preserve intentional cancellation without accepting replacement work or recreating a removed job.
