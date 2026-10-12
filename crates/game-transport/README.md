# Transport

Transport owns pending routes, carrying groups, offers, timed lifts, road approaches, paused intentions and personal support knowledge as eight optional components on existing person entities in the shared store. An empty pending route has no component. Carrying `Runtime` holds rules and a bounded response journal; routes also work without that optional runtime.

Typed route replacements guard their opening rows and issue admissions bound to the staging batch. Transport plans itineraries through Spatial's directed graph path function and consumes the first leg in the same edit as a prepared departure. `BodyInput` and `CarryWork` supply actual body limits and activity facts; Transport owns their interpretation. `BodyFacts` reads only affected people during ordinary group operations.

Prepared motion handles stopping, arrival, birth attachment, lift completion, departure, approach, retreat, resume, putdown, confinement release and load reconciliation. Spatial remains the Location authority. Motion stages Transport and Location edits together, provides projected location facts for receiving owners, and emits ordered effects for activity interruption, shelter movement, deadlines, dependent indexing and public events.

`MotionPlan` owns the prepared variants and their staging dispatcher. Transport also owns automatic lifting after an offer, the closing failure reaction, interrupted continuation, personal views and arrival overflow preflight. `IncomingCargo` supplies only carrying admission over Cargo's proposed conserved prefix; Cargo retains inventory balances, transfer selection and physical stock effects.

Support review captures common Spatial candidate IDs once, then reads current placement and tactical contact before its stable witness bound. A stationary review finalization rejects actual Location changes or any undeclared Location edit before publication, so subsequent recipients can reuse the shared per-cell candidates while evaluating their current eligibility.

A `MotionBatch` collects independent plans and coalesces compatible sparse rows. Its finalization must validate the complete CarryGroup/Lift footprint after all participants stage and before the host applies the shared batch. Undeclared carrying edges reject publication. Exact Location/Route read reservations reject conflicting foreign writes or removals through ordinary host admission. Selected body, topology, road, time and lift action facts reject stale plans.

Health owns effective-shelter selection, occupancy and observations through its own prepared placement interface. Motion never chooses a shelter. Tactical reach and obstacles belong to Conflict; its contact result accompanies Spatial-resolved anchor identities.

Intrinsic schemas validate native rows. `validate_routes` checks actual actor placements and directed topology. `validate_composition` checks both sides of carrier/group and lift/action links. Owner tests and a composed Transport–Health–generic host proof exercise these interfaces without the game assembly.
