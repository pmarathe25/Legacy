# Environmental exposure

> Historical note: references below to removed experimental hosts and their fixtures describe the previous implementation. See the [current runner](../game-economy-cli/README.md) for supported scenarios and commands.

Region-defined exposure and finite shelter are applied by the physical owner before conflict, medical care, and family mortality.

Optional `[[environment.seasons]]` entries define a repeating calendar with positive durations and per-thousand food growth and exposure factors. Tick one starts the first season. Food growth and net exposure after shelter use that tick's season; factors of 1000 preserve the usual rules and an empty calendar leaves them unchanged. The calendar is deterministic across save/load and the current ruler can observe its name. See `content/scenarios/seasons.toml`.

Seasons can independently disable planting, tending, or harvesting (`planting_allowed`, `tending_allowed`, `harvest_allowed`); each defaults to true. Disabled field operations consume neither seed nor labor and existing fields retain their progress until the operation is allowed again.

Interval scenarios can set `season_unit_millis` for the elapsed calendar and
`exposure_period_millis` for regional damage and shelter rotation. At a due
exposure the host uses the season in effect just before that instant and the
available shelter. Health changes commit alongside physical progress.
