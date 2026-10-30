# Work goals

Personal, authored craft goals select a feasible work policy by priority, use the physical production owner's actual input reservations, and advance only on completed work. Explicit work policies take precedence. If work cannot be granted, the selected goal backs off for `retry_interval` ticks (default 3), allowing a lower-priority goal to be attempted. Completed goals remain inspectable and the next unfinished goal becomes eligible.
`content/parallel/goals.toml` demonstrates a person pursuing two completed units of a craft. A goal does not conjure inputs or bypass stock, actor, or property reservations; completed goals stop producing work requests. Goals currently require the fused host and an `[autowork]` owner.

Players can inspect their own goals with `goals PERSON` and change an aspiration with `goal-set PERSON RECIPE PRIORITY TARGET enable|disable`. Existing progress is retained, and lowering a target below earned progress is rejected.

Set `plan_inputs = true` in `[goals]` to let goal-driven workers plan missing local ingredients through authored recipes. Planning searches bounded recipe chains, detects cycles, picks alternatives deterministically, and rechecks the committed inventory each tick. Intermediate jobs reserve and produce real goods; only the final job advances its goal. Competing workers still use normal stock arbitration, and a failed intermediate plan backs off the original goal. Explicit work policies retain precedence. See `content/parallel/goal_planning.toml`.
