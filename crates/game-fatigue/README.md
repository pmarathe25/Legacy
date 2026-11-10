# Fatigue

Data-driven fatigue and rest are staged from per-person activity snapshots. Rest begins and ends at distinct thresholds, allowing sustained recovery without switching back and forth each tick. Travel, fighting, and caregiving do not provide a rest opportunity; fighting and caregiving add exertion. A host applies exhaustion damage to unpublished physical health and pauses work while the previously committed rest state is active.

`TimedFatigue` accepts accumulated work, travel, rest, and idle durations for one quantum. It applies a boundary-weighted update and persists fractional remainders, so splitting that quantum into smaller progress segments does not multiply rounding or activity effects. This rule aggregates durations rather than preserving their chronological order; rest transitions and exhaustion are evaluated at the boundary.
