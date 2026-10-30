# Environmental exposure

Region-defined exposure and finite shelter are applied by the physical owner before conflict, medical care, and family mortality.

Optional `[[environment.seasons]]` entries define a repeating calendar with positive durations and per-thousand food growth and exposure factors. Tick one starts the first season. Food growth and net exposure after shelter use that tick's season; factors of 1000 preserve the usual rules and an empty calendar leaves them unchanged. The calendar is deterministic across save/load and the current ruler can observe its name. See `content/parallel/seasons.toml`.

Seasons can independently disable planting, tending, or harvesting (`planting_allowed`, `tending_allowed`, `harvest_allowed`); each defaults to true. Disabled field operations consume neither seed nor labor and existing fields retain their progress until the operation is allowed again.
