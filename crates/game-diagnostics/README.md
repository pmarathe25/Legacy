# game-diagnostics

Optional wall-clock spans measure feature costs without changing simulation state, schedules, randomness, or saves. Set `LEGACY_PROFILE=1` before launching a process. `snapshot` returns inclusive timing totals; nested measurements overlap. `failure` counts observed failed requests and prints each distinct reason once.

Diagnostics are process-local developer measurements. They are excluded from deterministic gameplay and journal equality checks.
