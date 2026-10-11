# game-projects

`Runtime` owns construction rules and the event cursor. Projects, enterprises, construction jobs, personal knowledge, and search deadlines are registered shared-store components. Typed ports read and update those rows without a private entity map.

The owner decides readiness, progress, cancellation, completion, and failure from actor, land, stock, and agreement facts. Personal views keep dated observations. Workshop projects select a `RecipeId`; borrowed Workshop definitions own its goods, quantities, and duration. Enterprise accounts record generic input and output quantities, actual labor, payments, and defaults.

Prepared transitions guard their opening rows, relevant construction and site membership, rules, cursor, and selected recipes. `stage_into` adds their row edits to a caller's batch; apply that batch once before calling `ProjectFinalization::finish` in the same serialized transition. Standalone `commit` performs this sequence. Failed admission publishes neither the cursor nor effects. Material consumption and asset creation use typed effects for the physical owners.
