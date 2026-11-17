# Fatigue

Data-driven fatigue and rest are staged from per-person activity snapshots. Travel, fighting, and caregiving do not provide a rest opportunity; fighting and caregiving add exertion. A host applies exhaustion damage to unpublished physical health and pauses work while the previously committed rest state is active. The legacy whole-boundary path still starts and ends rest at shared thresholds.

`TimedFatigue` accepts accumulated work, travel, rest, and idle durations for one quantum. It applies a boundary-weighted update and persists fractional remainders, so splitting that quantum into smaller progress segments does not multiply rounding or activity effects. This rule aggregates durations rather than preserving their chronological order; rest transitions and exhaustion are evaluated at the boundary.

An optional `rate_period_millis` gives those activity rates and saved fractions an explicit elapsed-time denominator, independent of the boundary duration. Older saves without the field retain boundary-based rates. `TimedFatigue` does not start or end rest automatically: `Fatigue::choose_rest` stages each person's already selected choice. The legacy whole-boundary `Fatigue::prepare` still uses automatic thresholds and has not yet been migrated.
