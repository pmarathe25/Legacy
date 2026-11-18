# Nutrition

> Historical note: references below to removed experimental hosts and their fixtures describe the previous implementation. See the [current runner](../game-economy-cli/README.md) for supported scenarios and commands.

Person-owned nutrient reserves and data-defined metabolism. Reserves missing
from a save start at the configured initial level, including at birth.

In an interval scenario, set `nutrition.rules.interval_millis` to schedule
metabolism and deficiency damage at exact elapsed deadlines. This replaces
`nutrition.rules.interval`, which counts physical boundaries in older saves.
The simulation applies a due metabolism event before new choices at that
instant. It credits nutrients when actual food consumption commits. Delayed
health effects from food still use the existing settlement schedule.

The reserve arrays use energy, protein, fat, fibre, vitamins, and minerals in
that order. The model does not yet support adding new nutrient types in content.
