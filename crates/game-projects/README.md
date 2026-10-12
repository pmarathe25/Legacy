# game-projects

`Runtime` owns construction rules and the event cursor. Projects, enterprises, construction jobs, personal knowledge, and search deadlines are registered shared-store components. Typed ports read and update those rows without a private entity map.

The owner decides readiness, progress, cancellation, completion, and failure from actor, land, stock, and agreement facts. Personal views keep dated observations. Workshop projects select a `RecipeId`; borrowed Workshop definitions own its goods, quantities, and duration. Enterprise accounts record generic input and output quantities, actual labor, payments, and defaults.

Prepared transitions retain selected rows, rules, recipes, and membership reads. `stage_into` adds their edits to a shared batch; call `ProjectFinalization::validate_batch` after all staging, apply once, then call `finish` in the same serialized transition. Standalone `commit` performs this sequence. Finishing publishes only the admitted cursor and effects.

Scoped area, plot, project, and enterprise-site guards reject new or moved competing rows while allowing independent changes elsewhere. Material consumption, input accounting, and asset creation compose with receiving owners before publication. Failed admission leaves their rows, cursor, and effects unchanged.
